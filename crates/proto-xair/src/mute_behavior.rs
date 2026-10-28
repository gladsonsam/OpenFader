//! XR18 mute behavior preferences with verified admission.
use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
struct Row {
    revision: u64,
    value: Option<bool>,
    observed: Option<Instant>,
    floor: Option<Instant>,
    pending: bool,
}
impl Row {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.value = None;
        self.observed = None;
        self.floor = Some(Instant::now());
    }
}
#[derive(Default)]
pub(super) struct Cache {
    pub generation: u64,
    rows: [Row; 2],
}
const IDS: [&str; 2] = ["hardmute", "dcamute"];
fn address(i: usize) -> String {
    format!("/-prefs/{}", IDS[i])
}
fn value(msg: &OscMessage, index: usize) -> anyhow::Result<bool> {
    anyhow::ensure!(msg.addr == address(index), "Unexpected preference reply");
    match msg.args.as_slice() {
        [OscType::Int(0)] => Ok(false),
        [OscType::Int(1)] => Ok(true),
        _ => anyhow::bail!("Invalid preference"),
    }
}
pub(super) fn invalidate(cache: &std::sync::Mutex<Cache>, generation: u64) {
    let mut c = cache.lock().unwrap();
    if c.generation == generation {
        for row in &mut c.rows {
            row.invalidate();
        }
    }
}
pub(super) fn record(
    cache: &std::sync::Mutex<Cache>,
    msg: &OscMessage,
    at: Instant,
    generation: u64,
) {
    let Some(index) = IDS
        .iter()
        .position(|id| msg.addr == format!("/-prefs/{id}"))
    else {
        return;
    };
    let mut c = cache.lock().unwrap();
    if c.generation != generation {
        return;
    }
    for (i, row) in c.rows.iter_mut().enumerate() {
        if i != index
            || row.pending
            || row.floor.is_some_and(|floor| at < floor)
            || row.observed.is_some_and(|prior| at < prior)
        {
            continue;
        }
        let decoded = value(msg, i);
        match decoded {
            Ok(v) => {
                if row.value != Some(v) {
                    row.revision = row.revision.wrapping_add(1);
                }
                row.value = Some(v);
                row.observed = Some(at);
            }
            Err(_) => row.invalidate(),
        }
    }
}
struct Pending<'a> {
    driver: &'a XAirDriver,
    generation: u64,
    index: usize,
    confirmed: bool,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut c = self.driver.mute_behavior.lock().unwrap();
        if c.generation == self.generation {
            let row = &mut c.rows[self.index];
            if !self.confirmed {
                row.invalidate();
            }
            row.pending = false;
        }
    }
}
impl XAirDriver {
    pub(super) fn refresh_mute_behavior(&self, session: std::sync::Arc<OscSession>) {
        if self.model != addrs::XAirModel::Xr18 {
            return;
        }
        let task = tokio::spawn(async move {
            for i in 0..2 {
                let _ = session
                    .query_timeout(&address(i), Duration::from_millis(350))
                    .await;
            }
        });
        self.tasks.lock().unwrap().push(task);
    }
    pub(super) fn mute_behavior_controls(&self) -> Vec<DestinationControl> {
        if self.model != addrs::XAirModel::Xr18 || !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let c = self.mute_behavior.lock().unwrap();
        if c.generation != generation {
            return vec![];
        }
        let topology = self.topology();
        let Some(all) = (0..self.strip_count())
            .map(|i| openfader_core::strip_key_for_index(&topology, i))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };
        c.rows
            .iter()
            .enumerate()
            .map(|(i, row)| DestinationControl {
                revision: Some(row.revision),
                generation: Some(generation),
                preset: false,
                members: all.clone(),
                target: "mute_behavior".into(),
                target_label: "Mute behavior".into(),
                target_name: None,
                send_slot: None,
                surface: "mute_behavior".into(),
                description: if i == 0 {
                    "Preserves individually muted channels when a mute group is unmuted."
                } else {
                    "Enables muting through DCA assignments."
                }
                .into(),
                id: IDS[i].into(),
                label: if i == 0 { "Hard Mutes" } else { "DCA Groups" }.into(),
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
                current: row
                    .value
                    .filter(|_| {
                        !row.pending
                            && row
                                .observed
                                .is_some_and(|at| at.elapsed() <= Duration::from_secs(5))
                    })
                    .map(|v| DestinationValue {
                        value: f32::from(v),
                        verified: true,
                    }),
            })
            .collect()
    }
    pub(super) async fn set_mute_behavior_checked(
        &self,
        id: &str,
        requested: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let i = IDS
            .iter()
            .position(|item| *item == id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported mute behavior"))?;
        anyhow::ensure!(
            self.model == addrs::XAirModel::Xr18 && (requested == 0.0 || requested == 1.0),
            "Invalid mute behavior"
        );
        let _lane = self.mute_behavior_write.lock().await;
        let _dispatch = self.bus_dispatch.write().await;
        let _preamp = self.preamp_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let valid = || {
            self.connected.load(Ordering::Acquire)
                && self.connection_generation.load(Ordering::Acquire) == generation
                && check()
        };
        let current = || self.mute_behavior_controls();
        anyhow::ensure!(
            valid()
                && snapshot.len() == 2
                && current() == snapshot
                && snapshot[i]
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected)
                && snapshot
                    .iter()
                    .all(|r| r.current.as_ref().is_some_and(|v| v.verified)),
            "Mute behavior state or access changed"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        for (j, row) in snapshot.iter().enumerate() {
            let (_, reply) = session
                .query_received_timeout(&address(j), Duration::from_millis(350))
                .await?;
            anyhow::ensure!(
                valid()
                    && f32::from(value(&reply, j)?) == row.current.as_ref().unwrap().value
                    && current() == snapshot,
                "Mute behavior state or access changed"
            );
        }
        let pending_revision = {
            let mut c = self.mute_behavior.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && c.rows.iter().zip(snapshot).all(|(row, expected_row)| {
                        !row.pending
                            && Some(row.revision) == expected_row.revision
                            && row.value.map(f32::from)
                                == expected_row.current.as_ref().map(|v| v.value)
                            && row
                                .observed
                                .is_some_and(|at| at.elapsed() <= Duration::from_secs(5))
                    }),
                "Mute behavior changed"
            );
            c.rows[i].pending = true;
            c.rows[i].invalidate();
            c.rows[i].revision
        };
        let block_current = || {
            let c = self.mute_behavior.lock().unwrap();
            c.generation == generation
                && c.rows.iter().enumerate().all(|(j, row)| {
                    if j == i {
                        row.pending && row.revision == pending_revision
                    } else {
                        !row.pending
                            && Some(row.revision) == snapshot[j].revision
                            && row.value.map(f32::from)
                                == snapshot[j].current.as_ref().map(|v| v.value)
                            && row
                                .observed
                                .is_some_and(|at| at.elapsed() <= Duration::from_secs(5))
                    }
                })
        };
        let mut pending = Pending {
            driver: self,
            generation,
            index: i,
            confirmed: false,
        };
        let guard = self.session().await?;
        anyhow::ensure!(
            valid()
                && guard
                    .as_ref()
                    .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session)),
            "Mute behavior session changed"
        );
        if requested != expected {
            session
                .send_checked(&address(i), vec![OscType::Int(requested as i32)], &|| {
                    anyhow::ensure!(
                        valid() && block_current(),
                        "Mute behavior access, state or session changed"
                    );
                    Ok(())
                })
                .await?;
        }
        let (at, reply) = session
            .query_received_timeout(&address(i), Duration::from_millis(350))
            .await?;
        anyhow::ensure!(
            valid() && block_current() && f32::from(value(&reply, i)?) == requested,
            "Mute behavior result uncertain"
        );
        {
            let mut c = self.mute_behavior.lock().unwrap();
            anyhow::ensure!(
                c.generation == generation
                    && c.rows[i].pending
                    && c.rows[i].floor.is_none_or(|floor| at >= floor),
                "Mute behavior confirmation invalidated"
            );
            let row = &mut c.rows[i];
            row.value = Some(requested == 1.0);
            row.observed = Some(at);
            row.floor = Some(at);
            row.revision = row.revision.wrapping_add(1);
        }
        anyhow::ensure!(valid(), "Mute behavior result uncertain");
        pending.confirmed = true;
        drop(pending);
        current()
            .into_iter()
            .find(|r| r.id == id)
            .filter(|r| {
                r.current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == requested)
            })
            .ok_or_else(|| anyhow::anyhow!("Mute behavior session changed"))
    }
}
