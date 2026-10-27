//! Shared RTA detector and decay preferences, read from the desk.
use super::*;
use openfader_core::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};
use std::{sync::atomic::Ordering, time::Duration};
const ADDRS: [&str; 2] = ["/-prefs/rta/det", "/-prefs/rta/decay"];
const TIMEOUT: Duration = Duration::from_millis(350);
#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    revision: u64,
    values: [Option<f32>; 2],
    pending: bool,
}
struct Pending {
    cache: std::sync::Arc<std::sync::Mutex<Cache>>,
    generation: u64,
}
impl Drop for Pending {
    fn drop(&mut self) {
        let mut state = self.cache.lock().unwrap();
        if state.generation == self.generation && state.pending {
            state.pending = false;
            state.revision = state.revision.wrapping_add(1);
            state.values = [None; 2];
        }
    }
}
fn decode(index: usize, msg: &OscMessage) -> Option<f32> {
    match (index, msg.args.as_slice()) {
        (0, [OscType::Int(i @ 0..=1)]) => Some(*i as f32),
        (1, [OscType::Float(v)]) if v.is_finite() && (0.0..=1.0).contains(v) => Some(*v),
        _ => None,
    }
}
impl XAirDriver {
    pub(super) fn start_rta_settings(&self, session: std::sync::Arc<OscSession>, generation: u64) {
        *self.rta_settings.lock().unwrap() = Cache {
            generation,
            ..Cache::default()
        };
        let cache = self.rta_settings.clone();
        let active = self.connection_generation.clone();
        let mut updates = session.subscribe_received_updates();
        let refresh = session.clone();
        let task = tokio::spawn(async move {
            for address in ADDRS {
                let _ = refresh.query_timeout(address, TIMEOUT).await;
            }
        });
        self.tasks.lock().unwrap().push(task);
        let task = tokio::spawn(async move {
            while let Ok((_, msg)) = updates.recv().await {
                if active.load(Ordering::Acquire) != generation {
                    break;
                }
                let Some(index) = ADDRS.iter().position(|a| *a == msg.addr) else {
                    continue;
                };
                let value = decode(index, &msg);
                let mut state = cache.lock().unwrap();
                if state.generation != generation || state.pending {
                    continue;
                }
                if state.values[index] != value {
                    state.revision = state.revision.wrapping_add(1);
                    state.values[index] = value;
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }
    pub(super) fn rta_setting_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let state = self.rta_settings.lock().unwrap();
        if state.generation != generation {
            return vec![];
        }
        (0..2)
            .map(|i| DestinationControl {
                revision: Some(state.revision),
                generation: Some(generation),
                preset: false,
                members: vec![],
                target: "rta".into(),
                target_label: "RTA".into(),
                target_name: None,
                send_slot: None,
                surface: "output_rta".into(),
                description: "Changes the shared mixer RTA settings.".into(),
                id: ["detector", "decay"][i].into(),
                label: ["Detector", "Decay"][i].into(),
                tap_tempo: false,
                gesture: None,
                choices: if i == 0 {
                    ["Peak", "RMS"]
                        .iter()
                        .enumerate()
                        .map(|(v, s)| DestinationChoice {
                            value: v as f32,
                            label: (*s).into(),
                        })
                        .collect()
                } else {
                    vec![]
                },
                range: if i == 1 {
                    Some(DestinationRange {
                        min: 0.,
                        max: 1.,
                        unit: "".into(),
                        decimals: 2,
                        step: None,
                    })
                } else {
                    None
                },
                current: if state.pending {
                    None
                } else {
                    state.values[i].map(|value| DestinationValue {
                        value,
                        verified: true,
                    })
                },
            })
            .collect()
    }
    pub(super) async fn set_rta_setting(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(target == "rta", "Unknown RTA target");
        let index = ["detector", "decay"]
            .iter()
            .position(|s| *s == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown RTA setting"))?;
        anyhow::ensure!(
            value.is_finite()
                && (0.0..=1.0).contains(&value)
                && (index != 0 || value.fract() == 0.0),
            "Invalid RTA value"
        );
        let _lane = self.rta_settings_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            check()
                && self.rta_setting_controls() == snapshot
                && snapshot[index]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "RTA settings changed; reopen control"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        let mut before = [0.; 2];
        for i in 0..2 {
            before[i] = decode(i, &session.query_timeout(ADDRS[i], TIMEOUT).await?)
                .ok_or_else(|| anyhow::anyhow!("RTA setting readback unavailable"))?;
        }
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && snapshot
                    .iter()
                    .enumerate()
                    .all(|(i, r)| r.current.as_ref().is_some_and(|v| v.value == before[i])),
            "RTA settings changed; reopen control"
        );
        {
            let mut state = self.rta_settings.lock().unwrap();
            anyhow::ensure!(
                state.generation == generation
                    && !state.pending
                    && snapshot
                        .iter()
                        .enumerate()
                        .all(|(i, r)| r.revision == Some(state.revision)
                            && r.current
                                .as_ref()
                                .is_some_and(|v| Some(v.value) == state.values[i])),
                "RTA session changed"
            );
            state.pending = true;
        }
        let _pending = Pending {
            cache: self.rta_settings.clone(),
            generation,
        };
        let wire = if index == 0 {
            OscType::Int(value as i32)
        } else {
            OscType::Float(value)
        };
        let outcome = async {
            anyhow::ensure!(check(), "RTA access changed");
            session.send(ADDRS[index], vec![wire]).await?;
            for _ in 0..4 {
                tokio::time::sleep(Duration::from_millis(25)).await;
                anyhow::ensure!(
                    check() && self.connection_generation.load(Ordering::Acquire) == generation,
                    "RTA result uncertain: session changed"
                );
                let mut now = [0.; 2];
                for i in 0..2 {
                    now[i] = decode(i, &session.query_timeout(ADDRS[i], TIMEOUT).await?)
                        .ok_or_else(|| {
                            anyhow::anyhow!("RTA result uncertain: readback unavailable")
                        })?;
                }
                anyhow::ensure!(
                    now[1 - index] == before[1 - index],
                    "RTA result uncertain: another setting changed"
                );
                if (now[index] - value).abs() < 0.00001 {
                    return Ok(now);
                }
            }
            anyhow::bail!("RTA result uncertain: readback did not match")
        }
        .await;
        {
            let mut state = self.rta_settings.lock().unwrap();
            if state.generation == generation {
                state.pending = false;
                state.revision = state.revision.wrapping_add(1);
                state.values = outcome
                    .as_ref()
                    .ok()
                    .map(|a| [Some(a[0]), Some(a[1])])
                    .unwrap_or([None; 2]);
            }
        }
        outcome?;
        self.rta_setting_controls()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("RTA result uncertain"))
    }
}
