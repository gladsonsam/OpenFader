//! DL32S global wire type 2 controls. Direct compiled app 5.2 intent from
//! /tmp/openfader-native-resource-protocol-evidence-20261027.json, global table
//! SHA256 c7bf31fe435b7d564aa331831e947f38dd780513c0fcdfefc98fdcf642679fcb.
//! This is static provenance, not new hardware proof. PARAM is wire type 5;
//! the transport's historical MUTE_GROUP constant also names the global bank.
use crate::Dl32sDriver;
use openfader_core::destinations::{DestinationControl, DestinationValue};
use openfader_transport::mackie::{MackieSession, REQUEST_TIMEOUT, VALUE_TYPE_MUTE_GROUP};
use std::{
    collections::BTreeMap,
    sync::{atomic::Ordering, OnceLock},
};

fn specs() -> &'static [(u32, DestinationControl)] {
    static SPECS: OnceLock<Vec<(u32, DestinationControl)>> = OnceLock::new();
    SPECS.get_or_init(|| {
        openfader_core::global_controls::reference_controls()
            .into_iter()
            .map(|row| {
                let address = match (row.target.as_str(), row.id.as_str()) {
                    ("solo", "mode") => 1,
                    ("solo", "location") => 2,
                    ("solo", "in_place") => 3,
                    ("solo", "level") => 85,
                    ("oscillator", "enable") => 17,
                    ("oscillator", "level") => 18,
                    ("oscillator", "waveform") => 19,
                    ("oscillator", "frequency") => 20,
                    ("talkback", "source") => 86,
                    ("talkback", "enable") => 87,
                    ("talkback", "level") => 88,
                    ("talkback", "assign_lr") => 89,
                    ("talkback", id) if id.starts_with("assign_aux_") => {
                        89 + id[11..].parse::<u32>().expect("reference Aux")
                    }
                    ("talkback", id) if id.starts_with("assign_subgroup_") => {
                        97 + id[16..].parse::<u32>().expect("reference subgroup")
                    }
                    ("monitor", "lr_defeat") => 104,
                    ("monitor", "level") => 105,
                    ("monitor", "delay_enable") => 106,
                    ("monitor", "distance") => 107,
                    _ => unreachable!("reference global descriptor without DL32S mapping"),
                };
                (address, row)
            })
            .collect()
    })
}
fn raw(row: &DestinationControl, value: f32) -> u32 {
    if row.range.is_some() {
        value.to_bits()
    } else {
        value as u32
    }
}
fn decode(row: &DestinationControl, word: u32) -> Option<DestinationValue> {
    let value = if row.range.is_some() {
        f32::from_bits(word)
    } else {
        word as f32
    };
    row.allows_value(value).then_some(DestinationValue {
        value,
        verified: true,
    })
}
#[derive(Debug, Default)]
struct Entry {
    current: Option<DestinationValue>,
    revision: u64,
}
#[derive(Debug, Default)]
pub(crate) struct GlobalControls {
    entries: BTreeMap<u32, Entry>,
}
impl GlobalControls {
    pub(crate) fn seed(&mut self, session: &MackieSession) {
        self.entries = specs()
            .iter()
            .map(|(address, row)| {
                (
                    *address,
                    Entry {
                        current: session
                            .cached_type2_verified(*address)
                            .and_then(|word| decode(row, word)),
                        revision: 0,
                    },
                )
            })
            .collect();
    }
    pub(crate) fn ingest(&mut self, start: u32, words: &[u32]) {
        for (address, row) in specs() {
            if let Some(word) = address
                .checked_sub(start)
                .and_then(|i| words.get(i as usize))
            {
                let entry = self.entries.entry(*address).or_default();
                let current = decode(row, *word);
                if entry.current != current {
                    entry.revision = entry.revision.wrapping_add(1);
                    entry.current = current;
                }
            }
        }
    }
    pub(crate) fn controls(&self, generation: u64) -> Vec<DestinationControl> {
        specs()
            .iter()
            .map(|(address, spec)| {
                let mut row = spec.clone();
                row.generation = Some(generation);
                row.revision = self.entries.get(address).map(|entry| entry.revision);
                row.current = self
                    .entries
                    .get(address)
                    .and_then(|entry| entry.current.clone());
                row
            })
            .collect()
    }
}
impl Dl32sDriver {
    pub(crate) async fn write_global_control(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: Option<&[DestinationControl]>,
    ) -> anyhow::Result<Option<DestinationControl>> {
        let Some((address, spec)) = specs()
            .iter()
            .find(|(_, row)| row.target == target && row.id == control)
        else {
            return Ok(None);
        };
        anyhow::ensure!(
            spec.allows_value(value) && spec.allows_value(expected),
            "Global value outside advertised domain"
        );
        let generation = self.generation.load(Ordering::Acquire);
        let _write = self.output_write.lock().await;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("session checked");
        anyhow::ensure!(
            session.is_alive() && self.generation.load(Ordering::Acquire) == generation,
            "Global connection changed"
        );
        let revision = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Global cache unavailable"))?;
            anyhow::ensure!(
                native.generation == generation && native.view.connected,
                "Global connection changed"
            );
            let rows = native.globals.controls(generation);
            if let Some(snapshot) = snapshot {
                let block: Vec<_> = rows.iter().filter(|row| row.target == target).collect();
                anyhow::ensure!(
                    snapshot.len() == block.len()
                        && snapshot.iter().all(|row| block.contains(&row)),
                    "Global configuration changed; reopen the control"
                );
            }
            let entry = native
                .globals
                .entries
                .get_mut(address)
                .ok_or_else(|| anyhow::anyhow!("Global value unavailable"))?;
            anyhow::ensure!(
                entry
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected)
                    && session.cached_type2_verified(*address) == Some(raw(spec, expected)),
                "Global value changed or unconfirmed; reopen the control"
            );
            entry.current = None;
            entry.revision = entry.revision.wrapping_add(1);
            entry.revision
        };
        session
            .write_int(*address, raw(spec, value), VALUE_TYPE_MUTE_GROUP)
            .await?;
        let readback_revision = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Global cache unavailable"))?;
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation
                    && native.generation == generation
                    && native.view.connected,
                "Global connection changed; result uncertain"
            );
            let entry = native
                .globals
                .entries
                .get_mut(address)
                .expect("seeded entry");
            if entry.revision == revision {
                entry.current = Some(DestinationValue {
                    value,
                    verified: false,
                });
                entry.revision = entry.revision.wrapping_add(1);
            }
            if entry.current.as_ref().is_some_and(|v| v.verified) {
                entry.revision
            } else {
                entry.revision.wrapping_add(1)
            }
        };
        session.request_value_snapshot().await.map_err(|error| {
            anyhow::anyhow!(
                "Global write accepted; readback request failed: {error}. Result uncertain"
            )
        })?;
        drop(guard);
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation,
                "Global connection changed; result uncertain"
            );
            {
                let native = self
                    .native
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Global cache unavailable; result uncertain"))?;
                anyhow::ensure!(
                    native.generation == generation && native.view.connected,
                    "Global connection changed; result uncertain"
                );
                let rows = native.globals.controls(generation);
                if let Some(snapshot) = snapshot {
                    anyhow::ensure!(
                        snapshot
                            .iter()
                            .filter(|row| row.id != control)
                            .all(|row| rows.contains(row)),
                        "Global configuration changed during readback; result uncertain"
                    );
                }
                let entry = native.globals.entries.get(address).expect("seeded entry");
                if let Some(current) = entry.current.as_ref().filter(|v| v.verified) {
                    anyhow::ensure!(
                        entry.revision == readback_revision && current.value == value,
                        "Global mixer readback differs or changed again; result uncertain"
                    );
                    // Capture while the native lock and write lane are held;
                    // later cache collection must not attribute a peer edit to us.
                    return Ok(rows
                        .into_iter()
                        .find(|row| row.target == target && row.id == control));
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Global write accepted without independent mixer readback; result uncertain"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
