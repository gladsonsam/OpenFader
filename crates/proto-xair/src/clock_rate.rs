use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const ADDRESS: &str = "/-prefs/clockrate";
#[derive(Default)]
pub(super) struct Cache {
    pub generation: u64,
    revision: u64,
    value: Option<i32>,
    observed: Option<Instant>,
    not_before: Option<Instant>,
    pending: bool,
}
impl Cache {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.value = None;
        self.observed = None;
        self.not_before = Some(Instant::now());
    }
}
fn parse(msg: &OscMessage) -> Option<i32> {
    if msg.addr != ADDRESS {
        return None;
    }
    match msg.args.as_slice() {
        [OscType::Int(v @ 0..=1)] => Some(*v),
        _ => None,
    }
}
pub(super) fn record(
    cache: &std::sync::Mutex<Cache>,
    msg: &OscMessage,
    at: Instant,
    generation: u64,
) {
    if msg.addr != ADDRESS {
        return;
    }
    let mut c = cache.lock().unwrap();
    if c.generation != generation
        || c.pending
        || c.not_before.is_some_and(|floor| at < floor)
        || c.observed.is_some_and(|prior| at < prior)
    {
        return;
    }
    if let Some(value) = parse(msg) {
        if c.value != Some(value) {
            c.revision = c.revision.wrapping_add(1);
        }
        c.value = Some(value);
        c.observed = Some(at);
    } else {
        c.invalidate();
    }
}
pub(super) fn invalidate(cache: &std::sync::Mutex<Cache>, generation: u64) {
    let mut c = cache.lock().unwrap();
    if c.generation == generation {
        c.invalidate();
    }
}
struct Pending<'a> {
    driver: &'a XAirDriver,
    generation: u64,
    confirmed: bool,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut c = self.driver.clock_rate.lock().unwrap();
        if c.generation == self.generation {
            if !self.confirmed {
                c.invalidate();
            }
            c.pending = false;
        }
    }
}
impl XAirDriver {
    pub(super) fn refresh_clock_rate(&self, session: std::sync::Arc<OscSession>) {
        if self.model != addrs::XAirModel::Xr18 {
            return;
        }
        let task = tokio::spawn(async move {
            let _ = session
                .query_timeout(ADDRESS, Duration::from_millis(350))
                .await;
        });
        self.tasks.lock().unwrap().push(task);
    }

    pub(super) fn clock_rate_controls(&self) -> Vec<DestinationControl> {
        if self.model != addrs::XAirModel::Xr18 || !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let c = self.clock_rate.lock().unwrap();
        if c.generation != generation {
            return vec![];
        }
        let topology = self.topology();
        let Some(members) = (0..self.strip_count())
            .map(|i| openfader_core::strip_key_for_index(&topology, i))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };
        vec![DestinationControl {
            revision: Some(c.revision),
            generation: Some(generation),
            preset: false,
            members,
            target: "device_setup".into(),
            target_label: "Device setup".into(),
            target_name: None,
            send_slot: None,
            surface: "clock_rate".into(),
            description: "Device clock rate.".into(),
            id: "clockrate".into(),
            label: "Clock Rate".into(),
            tap_tempo: false,
            gesture: None,
            choices: vec![
                DestinationChoice {
                    value: 0.0,
                    label: "48 kHz".into(),
                },
                DestinationChoice {
                    value: 1.0,
                    label: "44.1 kHz".into(),
                },
            ],
            range: None,
            current: c
                .value
                .filter(|_| {
                    !c.pending
                        && c.observed
                            .is_some_and(|at| at.elapsed() <= Duration::from_secs(5))
                })
                .map(|v| DestinationValue {
                    value: v as f32,
                    verified: true,
                }),
        }]
    }
    pub(super) async fn set_clock_rate_checked(
        &self,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(
            id == "clockrate" && (value == 0.0 || value == 1.0),
            "Invalid Clock rate control or value"
        );
        let generation = self.connection_generation.load(Ordering::Acquire);
        let _lane = self.clock_rate_write.lock().await;
        let _dispatch = self.bus_dispatch.write().await;
        let _preamp = self.preamp_write.lock().await;
        let valid = || {
            self.connected.load(Ordering::Acquire)
                && self.connection_generation.load(Ordering::Acquire) == generation
                && check()
        };
        anyhow::ensure!(
            valid()
                && snapshot.len() == 1
                && self.clock_rate_controls() == snapshot
                && snapshot[0]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "Clock rate state or access changed"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        anyhow::ensure!(valid(), "Clock rate session changed");
        let (read_at, reply) = session
            .query_received_timeout(ADDRESS, Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid()
                && parse(&reply).is_some_and(|v| v as f32 == expected)
                && self.clock_rate_controls() == snapshot,
            "Clock rate state or access changed"
        );
        if value == expected {
            anyhow::ensure!(valid(), "Clock rate access or session changed");
            {
                let mut c = self.clock_rate.lock().unwrap();
                anyhow::ensure!(
                    c.generation == generation
                        && !c.pending
                        && Some(c.revision) == snapshot[0].revision
                        && c.value == Some(expected as i32)
                        && c.not_before.is_none_or(|floor| read_at >= floor)
                        && c.observed.is_none_or(|prior| read_at >= prior),
                    "Clock rate state changed"
                );
                c.observed = Some(read_at);
                c.not_before = Some(read_at);
            }
            return self
                .clock_rate_controls()
                .into_iter()
                .next()
                .filter(|r| {
                    r.current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == value)
                })
                .ok_or_else(|| anyhow::anyhow!("Clock rate result unavailable"));
        }
        let _main_lane = self.main_mute_write.lock().await;
        let mute = session
            .query_timeout("/lr/mix/on", Duration::from_millis(350))
            .await?;
        anyhow::ensure!(valid(), "Clock rate access or session changed");
        anyhow::ensure!(
            mute.addr == "/lr/mix/on" && matches!(mute.args.as_slice(), [OscType::Int(0)]),
            "Mute Main LR before changing clock rate"
        );
        anyhow::ensure!(
            self.clock_rate_controls() == snapshot,
            "Clock rate state changed"
        );
        {
            let mut c = self.clock_rate.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && !c.pending
                    && Some(c.revision) == snapshot[0].revision
                    && c.value == Some(expected as i32)
                    && c.observed
                        .is_some_and(|at| at.elapsed() <= Duration::from_secs(5)),
                "Clock rate state changed"
            );
            c.pending = true;
            c.invalidate();
        }
        let mut pending = Pending {
            driver: self,
            generation,
            confirmed: false,
        };
        let guard = self.session().await?;
        anyhow::ensure!(
            valid()
                && guard
                    .as_ref()
                    .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session)),
            "Clock rate session changed"
        );
        session
            .send_checked(ADDRESS, vec![OscType::Int(value as i32)], &|| {
                anyhow::ensure!(valid(), "Clock rate access or session changed");
                Ok(())
            })
            .await?;
        drop(guard);
        let (received_at, reply) = session
            .query_received_timeout(ADDRESS, Duration::from_millis(350))
            .await?;
        let current_session = self.session().await?;
        anyhow::ensure!(
            valid()
                && current_session
                    .as_ref()
                    .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session))
                && parse(&reply) == Some(value as i32),
            "Clock rate result uncertain"
        );
        {
            let mut c = self.clock_rate.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && c.pending
                    && c.not_before.is_none_or(|floor| received_at >= floor),
                "Clock rate confirmation invalidated"
            );
            c.revision = c.revision.wrapping_add(1);
            c.value = Some(value as i32);
            c.observed = Some(received_at);
            c.not_before = Some(received_at);
            pending.confirmed = true;
        }
        if !valid() {
            pending.confirmed = false;
            anyhow::bail!("Clock rate result uncertain");
        }
        drop(pending);
        self.clock_rate_controls()
            .into_iter()
            .next()
            .filter(|r| {
                r.current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == value)
            })
            .ok_or_else(|| anyhow::anyhow!("Clock rate result unavailable"))
    }
}
