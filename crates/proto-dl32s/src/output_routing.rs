//! DL32S output patching from manufacturer mixer software 5.2 (28602), x86_64.
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
use openfader_transport::mackie::{
    MackieSession, MackieValueCache, REQUEST_TIMEOUT, VALUE_TYPE_PARAM,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
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
    cache: Option<MackieValueCache>,
    // Local reservations/readbacks invalidate guards even for no-op writes.
    revisions: BTreeMap<u32, u64>,
    pending: BTreeSet<u32>,
}
impl OutputRoutes {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.cache = Some(session.value_cache());
        self.revisions.clear();
        self.pending.clear();
    }
    pub(super) fn ingest(&mut self, start: u32, raws: &[u32]) {
        for i in 0..raws.len() {
            let pid = start + i as u32;
            if self.pending.remove(&pid) {
                let revision = self.revisions.entry(pid).or_default();
                *revision = revision.wrapping_add(1);
            }
        }
    }
    fn observed(&self, pid: u32) -> Option<(u32, u64, u64)> {
        self.cache.as_ref()?.verified(pid)
    }
    fn revision(&self, pid: u32, wire_revision: u64) -> u64 {
        wire_revision.wrapping_add(self.revisions.get(&pid).copied().unwrap_or_default())
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
        let (raw, wire_revision, _) = native
            .output_routes
            .observed(pid)
            .ok_or_else(|| anyhow::anyhow!("Waiting for independent output readback"))?;
        Ok(RouteState {
            source: label(plane, raw)
                .ok_or_else(|| anyhow::anyhow!("Unresolved output source {raw}"))?,
            revision: native.output_routes.revision(pid, wire_revision),
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

    /// Recheck engine authority and received route evidence at socket dispatch.
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
        // confirmed state. A subsequent push must release the reservation.
        let (observed, reservation) = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Output cache unavailable"))?;
            // The native cache lock can also queue behind telemetry. Recheck
            // engine authority after that wait, before reserving a write.
            anyhow::ensure!(context(), "Output engine session or policy changed");
            anyhow::ensure!(
                session.is_alive()
                    && native.generation == expected.generation
                    && !native.output_routes.pending.contains(&pid),
                "Output changed before write"
            );
            let observed = native
                .output_routes
                .observed(pid)
                .ok_or_else(|| anyhow::anyhow!("Waiting for independent output readback"))?;
            anyhow::ensure!(
                native.output_routes.revision(pid, observed.1) == expected.revision
                    && label(plane, observed.0).as_ref() == Some(&expected.source),
                "Output changed before write"
            );
            let revision = native.output_routes.revisions.entry(pid).or_default();
            *revision = revision.wrapping_add(1);
            let reservation = *revision;
            native.output_routes.pending.insert(pid);
            (observed, reservation)
        };
        // No native/output/session lock acquisition here: the caller already
        // owns the session and output lanes. Transport evidence is updated by
        // RX before native ingest, so even blocked-ingest A→B→A is rejected.
        let cache = session.value_cache();
        let rejected = AtomicBool::new(false);
        let authorize = || {
            let allowed = context()
                && session.is_alive()
                && self.generation.load(Ordering::Acquire) == expected.generation
                && cache
                    .verified(pid)
                    .is_some_and(|v| v.0 == observed.0 && v.1 == observed.1);
            if !allowed {
                rejected.store(true, Ordering::Release);
            }
            allowed
        };
        if let Err(error) = session
            .write_int_guarded(pid, raw, VALUE_TYPE_PARAM, &authorize)
            .await
        {
            if rejected.load(Ordering::Acquire) {
                // Only callback rejection proves no bytes left the socket.
                // Keep the advanced revision, but release this reservation so
                // readers need no subsequent push to recover. Ingest or a new
                // generation/reservation must never be undone by this cleanup.
                if let Ok(mut native) = self.native.lock() {
                    if self.generation.load(Ordering::Acquire) == expected.generation
                        && native.generation == expected.generation
                        && native.output_routes.revisions.get(&pid) == Some(&reservation)
                    {
                        native.output_routes.pending.remove(&pid);
                    }
                }
                anyhow::bail!("Output not sent; dispatch authorization rejected: {error}");
            }
            anyhow::bail!("Output write not confirmed; result uncertain: {error}");
        }
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
            // Transport provenance is independent of ACK: verified
            // withholds pending own writes even when their numeric value agrees.
            let session_guard = self.session().await?;
            if let Some(evidence) = cache.verified(pid) {
                anyhow::ensure!(
                    evidence.0 == raw
                        && evidence.1 == observed.1.wrapping_add(u64::from(raw != observed.0))
                        && evidence.2 != observed.2,
                    "Output readback differs; result uncertain"
                );
                if let Ok(state) = self.output_route(id) {
                    anyhow::ensure!(
                        state.generation == expected.generation
                            && state.source == source
                            && state.revision
                                == expected
                                    .revision
                                    .wrapping_add(2)
                                    .wrapping_add(u64::from(raw != observed.0)),
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
