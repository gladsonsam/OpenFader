//! DL32S output patching from Master Fader 5.2 (28602), x86_64.
//! Executable SHA256 efee2537ba2a65315d4952f837c2a85452769e53f9f1790e8ce206f13c4d1e57.
//! DLF32 descriptor rows (52 bytes) at file 0x6357f58: type5 PID5214..5257.
//! DLDeviceChannelModel::getOutputSources at VM0x1000f6ad0 returns member+a0;
//! createChannels 0x1000f3304..0x1000f38f4 populates that list. getIPadSources
//! at 0x1000f6c50 returns+b8, populated at 0x1000f3ba4..0x1000f3f84.
//! DLFresh32Definition::channelInstanceMap at 0x1001244b0 bounds input32,
//! return2, FXreturn4 (two sides), subgroup6, aux8, headphone2, matrix absent.
//! Sources are those explicit loops/constants, NOT the 0..17000 envelope.
//! NONE is a named enum but absent from these compiled selection lists: no
//! clear is advertised. USB omits generator/monitor; neither list has Dante.
//! Static intent only: firmware semantics and USB taps await owner live proof.
use crate::Dl32sDriver;
use openfader_core::routing::{OutputPlane, RouteState};
use openfader_transport::mackie::{MackieSession, REQUEST_TIMEOUT, VALUE_TYPE_PARAM};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::Ordering,
};

pub fn planes() -> Vec<OutputPlane> {
    [
        ("output/xlr", "XLR outputs", 10),
        ("output/headphone", "Headphones", 2),
        ("output/usb", "USB sends", 32),
    ]
    .into_iter()
    .map(|(key, label, count)| OutputPlane {
        key,
        label,
        count,
        clearable: false,
        sources: groups(key),
    })
    .collect()
}

fn groups(plane: &str) -> Vec<(&'static str, &'static str, usize)> {
    let mut groups = vec![
        ("Mic", "Mic Pre", 32),
        ("Input", "Input post DSP", 32),
        ("Return", "Return post DSP", 2),
        ("FxReturn", "FX return post DSP", 8),
        ("MainPre", "Main pre fader", 2),
        ("Main", "Main", 2),
        ("Sub", "Subgroup", 6),
        ("Aux", "Aux", 8),
        ("FxSend", "FX send", 4),
    ];
    if plane != "output/usb" {
        groups.extend([
            ("Osc", "Oscillator", 1),
            ("Monitor", "Monitor", 2),
            ("USB", "USB return", 32),
        ]);
    }
    groups
}

fn source_base(group: &str) -> Option<u32> {
    Some(match group {
        "Mic" => 1000,
        "USB" => 2000,
        "Input" => 4000,
        "Return" => 5000,
        "FxSend" => 6000,
        "FxReturn" => 7000,
        "MainPre" => 9000,
        "Main" => 10000,
        "Monitor" => 11000,
        "Sub" => 12000,
        "Aux" => 13000,
        "Osc" => 17000,
        _ => return None,
    })
}

fn address(id: &str) -> anyhow::Result<(u32, &str)> {
    let (plane, slot) = id
        .rsplit_once('/')
        .ok_or_else(|| anyhow::anyhow!("Unknown output"))?;
    let slot: u16 = slot.parse()?;
    let (start, count) = match plane {
        "output/xlr" => (5214, 10),
        "output/headphone" => (5224, 2),
        "output/usb" => (5226, 32),
        _ => anyhow::bail!("Unknown output plane"),
    };
    anyhow::ensure!(
        slot > 0 && slot <= count && id == format!("{plane}/{slot:02}"),
        "Unknown output slot"
    );
    Ok((start + u32::from(slot - 1), plane))
}

fn raw_source(plane: &str, source: &str) -> anyhow::Result<u32> {
    let (group, index) = source
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("Unknown source"))?;
    let index: usize = index.parse()?;
    anyhow::ensure!(
        groups(plane)
            .iter()
            .any(|(g, _, n)| *g == group && index > 0 && index <= *n),
        "Source is not advertised for this output"
    );
    Ok(source_base(group).expect("listed source") + index as u32 - 1)
}

fn label(plane: &str, raw: u32) -> Option<String> {
    // Display NONE if independently observed, without licensing a clear write.
    if raw == 0 {
        return Some("—".into());
    }
    groups(plane).into_iter().find_map(|(group, _, count)| {
        let base = source_base(group)?;
        (raw >= base && raw < base + count as u32).then(|| format!("{group} {}", raw - base + 1))
    })
}

#[derive(Debug, Default)]
pub(super) struct OutputRoutes {
    rows: BTreeMap<u32, (u32, u64)>,
    pending: BTreeSet<u32>,
}
impl OutputRoutes {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.rows.clear();
        self.pending.clear();
        for pid in 5214..=5257 {
            if let Some(raw) = session.cached_verified(pid) {
                self.rows.insert(pid, (raw, 1));
            }
        }
    }
    pub(super) fn ingest(&mut self, start: u32, raws: &[u32]) {
        for (i, raw) in raws.iter().enumerate() {
            let pid = start + i as u32;
            if !(5214..=5257).contains(&pid) {
                continue;
            }
            let row = self.rows.entry(pid).or_insert((*raw, 0));
            if self.pending.remove(&pid) || row.0 != *raw || row.1 == 0 {
                row.0 = *raw;
                row.1 = row.1.wrapping_add(1);
            }
        }
    }
}

impl Dl32sDriver {
    pub fn output_route(&self, id: &str) -> anyhow::Result<RouteState> {
        let (pid, plane) = address(id)?;
        let generation = self.generation.load(Ordering::Acquire);
        let native = self
            .native
            .lock()
            .map_err(|_| anyhow::anyhow!("Output cache unavailable"))?;
        anyhow::ensure!(
            native.generation == generation && native.view.connected,
            "Output session unavailable"
        );
        anyhow::ensure!(
            !native.output_routes.pending.contains(&pid),
            "Output write awaits independent readback"
        );
        let (raw, revision) = native
            .output_routes
            .rows
            .get(&pid)
            .ok_or_else(|| anyhow::anyhow!("Waiting for independent output readback"))?;
        Ok(RouteState {
            source: label(plane, *raw)
                .ok_or_else(|| anyhow::anyhow!("Unresolved output source {raw}"))?,
            revision: *revision,
            generation,
        })
    }

    pub async fn set_output_route(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
    ) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, &|| true)
            .await
    }

    /// Recheck the owning engine epoch/policy after all provider lock waits.
    pub async fn set_output_route_guarded(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        let (pid, plane) = address(id)?;
        let raw = raw_source(plane, source)?;
        anyhow::ensure!(
            label(plane, raw).as_deref() == Some(source),
            "Output source must use its canonical identity"
        );
        let _lane = self.output_write.lock().await;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("session checked");
        anyhow::ensure!(context(), "Output engine session or policy changed");
        anyhow::ensure!(
            session.is_alive() && &self.output_route(id)? == expected,
            "Output source/revision/session changed"
        );
        anyhow::ensure!(
            self.generation.load(Ordering::Acquire) == expected.generation
                && session
                    .cached_verified(pid)
                    .and_then(|raw| label(plane, raw))
                    .as_ref()
                    == Some(&expected.source),
            "Output native source/session changed before write"
        );
        // Reserve a revision and hide the row BEFORE writing. ACK never creates
        // confirmed state. A subsequent native push must repopulate this PID.
        {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Output cache unavailable"))?;
            // The native cache lock can also queue behind telemetry. Recheck
            // engine authority after that final wait, before reserving a write.
            anyhow::ensure!(context(), "Output engine session or policy changed");
            anyhow::ensure!(
                session.is_alive()
                    && native.generation == expected.generation
                    && !native.output_routes.pending.contains(&pid),
                "Output changed before write"
            );
            let row = native
                .output_routes
                .rows
                .get_mut(&pid)
                .expect("checked above");
            anyhow::ensure!(
                row.1 == expected.revision
                    && label(plane, row.0).as_ref() == Some(&expected.source),
                "Output changed before write"
            );
            row.1 = row.1.wrapping_add(1);
            native.output_routes.pending.insert(pid);
        }
        session
            .write_int(pid, raw, VALUE_TYPE_PARAM)
            .await
            .map_err(|e| anyhow::anyhow!("Output write not confirmed; result uncertain: {e}"))?;
        session.request_value_snapshot().await.map_err(|e| {
            anyhow::anyhow!("Output accepted but readback request failed; result uncertain: {e}")
        })?;
        drop(guard);
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == expected.generation,
                "Output session changed; result uncertain"
            );
            // Transport provenance is independent of ACK: cached_verified
            // withholds pending own writes even when their numeric value agrees.
            let session_guard = self.session().await?;
            let session = session_guard.as_ref().expect("session checked");
            if let Some(observed) = session.cached_verified(pid) {
                anyhow::ensure!(observed == raw, "Output readback differs; result uncertain");
                if let Ok(state) = self.output_route(id) {
                    anyhow::ensure!(
                        state.source == source
                            && state.revision == expected.revision.wrapping_add(2),
                        "Output changed during readback; result uncertain"
                    );
                    return Ok(());
                }
            }
            drop(session_guard);
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Output accepted without independent readback; result uncertain"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
