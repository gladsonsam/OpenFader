//! Shared Monitor detector and decay preferences, read from the desk.
use super::*;
use openfader_core::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
const ADDRS: [&str; 10] = [
    "/config/solo/level",
    "/config/solo/source",
    "/config/solo/sourcetrim",
    "/config/solo/chmode",
    "/config/solo/busmode",
    "/config/solo/dim",
    "/config/solo/mono",
    "/config/solo/mute",
    "/config/solo/dimpfl",
    "/config/solo/dimatt",
];
const IDS: [&str; 10] = [
    "level",
    "source",
    "trim",
    "channel_mode",
    "bus_mode",
    "dim",
    "mono",
    "mute",
    "dim_pfl",
    "dim_gain",
];
const LABELS: [&str; 10] = [
    "Monitor level",
    "Monitor source",
    "Source trim",
    "Channel solo",
    "Bus solo",
    "Dim",
    "Mono",
    "Mute",
    "Dim on PFL",
    "Dim gain",
];
fn numeric(i: usize) -> bool {
    matches!(i, 0 | 2 | 9)
}
fn choices(i: usize) -> Vec<DestinationChoice> {
    let labels: &[&str] = match i {
        1 => &[
            "Off",
            "LR",
            "LR PFL",
            "LR AFL",
            "Aux",
            "USB 17/18",
            "Bus 1",
            "Bus 2",
            "Bus 3",
            "Bus 4",
            "Bus 5",
            "Bus 6",
            "Bus 1/2",
            "Bus 3/4",
            "Bus 5/6",
        ],
        3 | 4 => &["PFL", "AFL"],
        5..=8 => &["Off", "On"],
        _ => &[],
    };
    labels
        .iter()
        .enumerate()
        .map(|(v, label)| DestinationChoice {
            value: v as f32,
            label: (*label).into(),
        })
        .collect()
}
const TIMEOUT: Duration = Duration::from_millis(350);
#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    revision: u64,
    values: [Option<f32>; 10],
    pending: bool,
    observed: [Option<Instant>; 10],
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
            state.values = [None; 10];
            state.observed = [None; 10];
        }
    }
}
fn decode(index: usize, msg: &OscMessage) -> Option<f32> {
    match msg.args.as_slice() {
        [OscType::Int(i)] if !numeric(index) && *i >= 0 && (*i as usize) < choices(index).len() => {
            Some(*i as f32)
        }
        [OscType::Float(v)] if numeric(index) && v.is_finite() && (0.0..=1.0).contains(v) => {
            Some(*v)
        }
        _ => None,
    }
}
impl XAirDriver {
    pub(super) fn start_monitor_settings(
        &self,
        session: std::sync::Arc<OscSession>,
        generation: u64,
    ) {
        *self.monitor_settings.lock().unwrap() = Cache {
            generation,
            ..Cache::default()
        };
        let cache = self.monitor_settings.clone();
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
            loop {
                let (at, msg) = match updates.recv().await {
                    Ok(update) => update,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let mut state = cache.lock().unwrap();
                        if state.generation == generation {
                            state.values = [None; 10];
                            state.observed = [None; 10];
                            state.revision = state.revision.wrapping_add(1);
                        }
                        continue;
                    }
                    Err(_) => break,
                };
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
                state.observed[index] = Some(at);
                if state.values[index] != value {
                    state.revision = state.revision.wrapping_add(1);
                    state.values[index] = value;
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }
    pub(super) fn monitor_setting_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let state = self.monitor_settings.lock().unwrap();
        if state.generation != generation {
            return vec![];
        }
        Self::monitor_controls_from_cache(&state, generation)
    }
    fn monitor_controls_from_cache(state: &Cache, generation: u64) -> Vec<DestinationControl> {
        (0..10)
            .map(|i| DestinationControl {
                revision: Some(state.revision),
                generation: Some(generation),
                preset: false,
                members: vec![],
                target: "monitor".into(),
                target_label: "Monitor".into(),
                target_name: None,
                send_slot: None,
                surface: "output_monitor".into(),
                description: "Changes the shared mixer Monitor settings.".into(),
                id: IDS[i].into(),
                label: LABELS[i].into(),
                tap_tempo: false,
                gesture: None,
                choices: choices(i),
                range: if numeric(i) {
                    Some(DestinationRange {
                        min: 0.,
                        max: 1.,
                        unit: "normalized".into(),
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
    /// Two independently read passes detect changes, but OSC cannot provide an
    /// atomic snapshot when the desk is edited concurrently.
    pub(super) async fn read_monitor_snapshot(&self) -> anyhow::Result<Vec<DestinationControl>> {
        let _lane = self.monitor_settings_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let check = || -> anyhow::Result<()> {
            anyhow::ensure!(
                self.connected.load(Ordering::Acquire)
                    && self.connection_generation.load(Ordering::Acquire) == generation,
                "Monitor capture session changed"
            );
            Ok(())
        };
        check()?;
        let session = self.session().await?.as_ref().unwrap().clone();
        check()?;
        let mut values = [0.0; 10];
        let mut receipts = [None; 10];
        for pass in 0..2 {
            for i in 0..10 {
                check()?;
                let (at, message) = session.query_received_timeout(ADDRS[i], TIMEOUT).await?;
                check()?;
                let value = decode(i, &message)
                    .ok_or_else(|| anyhow::anyhow!("Invalid Monitor readback: {}", IDS[i]))?;
                if pass == 0 {
                    values[i] = value;
                } else {
                    anyhow::ensure!(
                        values[i] == value,
                        "Monitor changed during capture: {}",
                        IDS[i]
                    );
                    receipts[i] = Some(at);
                }
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            check()?;
            {
                let state = self.monitor_settings.lock().unwrap();
                anyhow::ensure!(
                    state.generation == generation && !state.pending,
                    "Monitor cache unavailable"
                );
                if (0..10).all(|i| {
                    state.values[i] == Some(values[i])
                        && state.observed[i]
                            .zip(receipts[i])
                            .is_some_and(|(at, receipt)| at >= receipt)
                }) {
                    check()?;
                    return Ok(Self::monitor_controls_from_cache(&state, generation));
                }
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Monitor changed or readback incomplete during capture"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub(super) async fn set_monitor_setting(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(target == "monitor", "Unknown Monitor target");
        let index = IDS
            .iter()
            .position(|s| *s == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown Monitor setting"))?;
        anyhow::ensure!(
            value.is_finite()
                && if numeric(index) {
                    (0.0..=1.0).contains(&value)
                } else {
                    value.fract() == 0.0 && value >= 0.0 && (value as usize) < choices(index).len()
                },
            "Invalid Monitor value"
        );
        let _lane = self.monitor_settings_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            check()
                && self.monitor_setting_controls() == snapshot
                && snapshot[index]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "Monitor settings changed; reopen control"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        let mut before = [0.; 10];
        for i in 0..10 {
            before[i] = decode(i, &session.query_timeout(ADDRS[i], TIMEOUT).await?)
                .ok_or_else(|| anyhow::anyhow!("Monitor setting readback unavailable"))?;
        }
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && snapshot
                    .iter()
                    .enumerate()
                    .all(|(i, r)| r.current.as_ref().is_some_and(|v| v.value == before[i])),
            "Monitor settings changed; reopen control"
        );
        {
            let mut state = self.monitor_settings.lock().unwrap();
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
                "Monitor session changed"
            );
            state.pending = true;
            state.observed = [None; 10];
        }
        let _pending = Pending {
            cache: self.monitor_settings.clone(),
            generation,
        };
        let wire = if numeric(index) {
            OscType::Float(value)
        } else {
            OscType::Int(value as i32)
        };
        let outcome = async {
            anyhow::ensure!(check(), "Monitor access changed");
            session
                .send_checked(ADDRS[index], vec![wire], &|| {
                    anyhow::ensure!(
                        check() && self.connection_generation.load(Ordering::Acquire) == generation,
                        "Monitor access or session changed before dispatch"
                    );
                    Ok(())
                })
                .await?;
            for _ in 0..4 {
                tokio::time::sleep(Duration::from_millis(25)).await;
                anyhow::ensure!(
                    check() && self.connection_generation.load(Ordering::Acquire) == generation,
                    "Monitor result uncertain: session changed"
                );
                let mut now = [0.; 10];
                for i in 0..10 {
                    now[i] = decode(i, &session.query_timeout(ADDRS[i], TIMEOUT).await?)
                        .ok_or_else(|| {
                            anyhow::anyhow!("Monitor result uncertain: readback unavailable")
                        })?;
                }
                anyhow::ensure!(
                    (0..10).all(|i| i == index || now[i] == before[i]),
                    "Monitor result uncertain: another setting changed"
                );
                if (now[index] - value).abs() < 0.00001 {
                    return Ok(now);
                }
            }
            anyhow::bail!("Monitor result uncertain: readback did not match")
        }
        .await;
        {
            let mut state = self.monitor_settings.lock().unwrap();
            if state.generation == generation {
                state.pending = false;
                state.observed = [None; 10];
                state.revision = state.revision.wrapping_add(1);
                state.values = outcome
                    .as_ref()
                    .ok()
                    .map(|a| a.map(Some))
                    .unwrap_or([None; 10]);
            }
        }
        outcome?;
        self.monitor_setting_controls()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("Monitor result uncertain"))
    }
}
