use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const ADDRESS: &str = "/-prefs/midiconfig";
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
        [OscType::Int(v @ 0..=127)] => Some(*v),
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
struct Failure<'a> {
    driver: &'a XAirDriver,
    generation: u64,
    success: bool,
}
impl Drop for Failure<'_> {
    fn drop(&mut self) {
        if !self.success {
            invalidate(&self.driver.midi_setup, self.generation);
        }
    }
}
struct Pending<'a> {
    driver: &'a XAirDriver,
    generation: u64,
    confirmed: bool,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut c = self.driver.midi_setup.lock().unwrap();
        if c.generation == self.generation {
            if !self.confirmed {
                c.invalidate();
            }
            c.pending = false;
        }
    }
}
impl XAirDriver {
    pub(super) fn refresh_midi_setup(&self, session: std::sync::Arc<OscSession>) {
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

    pub(super) fn midi_setup_controls(&self) -> Vec<DestinationControl> {
        if self.model != addrs::XAirModel::Xr18 || !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let c = self.midi_setup.lock().unwrap();
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
        const IDS: [(&str, &str); 7] = [
            ("din_rx", "DIN Rx"),
            ("din_tx", "DIN Tx"),
            ("din_osc", "DIN X-OSC"),
            ("usb_rx", "USB Rx"),
            ("usb_tx", "USB Tx"),
            ("usb_osc", "USB/OSC"),
            ("pass_thru", "USB-DIN Pass Thru"),
        ];
        IDS.iter()
            .enumerate()
            .map(|(bit, (id, label))| DestinationControl {
                revision: Some(c.revision),
                generation: Some(generation),
                preset: false,
                members: members.clone(),
                target: "midi_setup".into(),
                target_label: "MIDI setup".into(),
                target_name: None,
                send_slot: None,
                surface: "midi_setup".into(),
                description: "MIDI configuration.".into(),
                id: (*id).into(),
                label: (*label).into(),
                tap_tempo: false,
                gesture: None,
                choices: vec![
                    DestinationChoice {
                        value: 0.0,
                        label: "Off".into(),
                    },
                    DestinationChoice {
                        value: 1.0,
                        label: "On".into(),
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
                        value: ((v >> bit) & 1) as f32,
                        verified: true,
                    }),
            })
            .collect()
    }

    pub(super) async fn set_midi_setup_checked(
        &self,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(
            matches!(
                id,
                "din_rx" | "din_tx" | "din_osc" | "usb_rx" | "usb_tx" | "usb_osc" | "pass_thru"
            ) && (value == 0.0 || value == 1.0),
            "Invalid MIDI setup control or value"
        );
        let bit = [
            "din_rx",
            "din_tx",
            "din_osc",
            "usb_rx",
            "usb_tx",
            "usb_osc",
            "pass_thru",
        ]
        .iter()
        .position(|x| *x == id)
        .unwrap();
        let generation = self.connection_generation.load(Ordering::Acquire);
        let _lane = self.midi_setup_write.lock().await;
        let _dispatch = self.bus_dispatch.write().await;
        let _preamp = self.preamp_write.lock().await;
        let valid = || {
            self.connected.load(Ordering::Acquire)
                && self.connection_generation.load(Ordering::Acquire) == generation
                && check()
        };
        anyhow::ensure!(
            valid()
                && snapshot.len() == 7
                && self.midi_setup_controls() == snapshot
                && snapshot.iter().all(|r| r
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && (v.value == 0.0 || v.value == 1.0)))
                && snapshot[bit]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "MIDI setup state or access changed"
        );
        let mut failure = Failure {
            driver: self,
            generation,
            success: false,
        };
        let old = {
            let c = self.midi_setup.lock().unwrap();
            c.value
                .ok_or_else(|| anyhow::anyhow!("MIDI setup mask unknown"))?
        };
        let desired = (old & !(1 << bit)) | ((value as i32) << bit);
        let session = self
            .session()
            .await?
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MIDI setup session unavailable"))?
            .clone();
        anyhow::ensure!(valid(), "MIDI setup session changed");
        // OSC has no atomic compare-and-swap against other clients.
        let reply = session
            .query_timeout(ADDRESS, Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid() && parse(&reply) == Some(old) && self.midi_setup_controls() == snapshot,
            "MIDI setup state or access changed"
        );
        {
            let mut c = self.midi_setup.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && !c.pending
                    && Some(c.revision) == snapshot[0].revision
                    && c.value == Some(old)
                    && c.observed
                        .is_some_and(|at| at.elapsed() <= Duration::from_secs(5)),
                "MIDI setup state changed"
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
            "MIDI setup session changed"
        );
        session
            .send_checked(ADDRESS, vec![OscType::Int(desired)], &|| {
                anyhow::ensure!(valid(), "MIDI setup access or session changed");
                Ok(())
            })
            .await?;
        drop(guard);
        let (received_at, reply) = session
            .query_received_timeout(ADDRESS, Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid() && parse(&reply) == Some(desired),
            "MIDI setup result uncertain"
        );
        {
            let mut c = self.midi_setup.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && c.pending
                    && c.not_before.is_none_or(|floor| received_at >= floor),
                "MIDI setup confirmation invalidated"
            );
            c.revision = c.revision.wrapping_add(1);
            c.value = Some(desired);
            c.observed = Some(received_at);
            c.not_before = Some(received_at);
            pending.confirmed = true;
        }
        if !valid() {
            pending.confirmed = false;
            anyhow::bail!("MIDI setup result uncertain");
        }
        drop(pending);
        let result = self
            .midi_setup_controls()
            .into_iter()
            .find(|r| r.id == id)
            .filter(|r| {
                r.current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == value)
            })
            .ok_or_else(|| anyhow::anyhow!("MIDI setup result unavailable"))?;
        anyhow::ensure!(valid(), "MIDI setup result uncertain");
        failure.success = true;
        Ok(result)
    }
}
