//! XR18 bus and main PEQ/GEQ/TEQ selector and 31 graphic-EQ bands.
//! Readback uses the existing OSC session and no page-owned ticker.
use super::*;
use openfader_core::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};
use std::{
    collections::BTreeMap,
    sync::atomic::Ordering,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

const FIELDS: usize = 32;
const TIMEOUT: Duration = Duration::from_millis(350);
#[derive(Clone, Default)]
struct Cell {
    value: Option<f32>,
    revision: u64,
    observed: Option<Instant>,
    pending: bool,
}
#[derive(Clone)]
struct Block {
    cells: Vec<Cell>,
}
impl Default for Block {
    fn default() -> Self {
        Self {
            cells: vec![Cell::default(); FIELDS],
        }
    }
}
#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    serial: u64,
    blocks: BTreeMap<String, Block>,
}
type Shared = Arc<StdMutex<Cache>>;
struct PendingGuard {
    cache: Shared,
    generation: u64,
    target: String,
    index: usize,
    session: Arc<OscSession>,
    active: Arc<std::sync::atomic::AtomicU64>,
    address: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        let mut cache = self.cache.lock().unwrap();
        if cache.generation != self.generation {
            return;
        }
        if let Some(cell) = cache
            .blocks
            .get_mut(&self.target)
            .and_then(|b| b.cells.get_mut(self.index))
        {
            if cell.pending {
                cell.pending = false;
                cell.value = None;
                cell.observed = None;
                let session = self.session.clone();
                let active = self.active.clone();
                let generation = self.generation;
                let address = self.address.clone();
                tokio::spawn(async move {
                    if active.load(Ordering::Acquire) == generation {
                        let _ = session.query_timeout(&address, TIMEOUT).await;
                    }
                });
            }
        }
    }
}

fn targets() -> Vec<(String, String, String)> {
    let mut rows = (1..=6)
        .map(|n| {
            (
                format!("bus/{n:02}"),
                format!("Bus {n}"),
                format!("/bus/{n}"),
            )
        })
        .collect::<Vec<_>>();
    rows.push(("main".into(), "Main LR".into(), "/lr".into()));
    rows
}
fn address(root: &str, index: usize) -> String {
    if index == 0 {
        format!("{root}/eq/mode")
    } else {
        format!("{root}/geq/{}", fx::GEQ_BANDS[index - 1])
    }
}
fn field_id(index: usize) -> String {
    if index == 0 {
        "eq_mode".into()
    } else {
        format!("geq{index:02}_gain")
    }
}
fn decode(index: usize, msg: &OscMessage) -> Option<f32> {
    match (index, msg.args.as_slice()) {
        (0, [OscType::Int(i @ 0..=2)]) => Some(*i as f32),
        (_, [OscType::Float(wire)])
            if index > 0 && wire.is_finite() && (0.0..=1.0).contains(wire) =>
        {
            // The firmware's lin(61) bands are spaced at 0.5 dB.
            Some((wire * 60.0).round() * 0.5 - 15.0)
        }
        _ => None,
    }
}
fn observe(cache: &Shared, generation: u64, at: Instant, msg: &OscMessage) {
    let Some((target, index)) = targets().into_iter().find_map(|(target, _, root)| {
        (0..FIELDS)
            .find(|&i| address(&root, i) == msg.addr)
            .map(|i| (target, i))
    }) else {
        return;
    };
    let value = decode(index, msg);
    let mut cache = cache.lock().unwrap();
    if cache.generation != generation {
        return;
    }
    let cell = &cache.blocks.entry(target.clone()).or_default().cells[index];
    if cell.pending || cell.observed.is_some_and(|prior| prior > at) {
        return;
    }
    let changed = cell.value != value;
    if changed {
        cache.serial = cache.serial.wrapping_add(1);
    }
    let serial = cache.serial;
    let cell = &mut cache.blocks.get_mut(&target).unwrap().cells[index];
    if changed {
        cell.value = value;
        cell.revision = serial;
    }
    cell.observed = Some(at);
}
async fn hydrate(
    session: Arc<OscSession>,
    active: Arc<std::sync::atomic::AtomicU64>,
    generation: u64,
) {
    for (_, _, root) in targets() {
        for index in 0..FIELDS {
            if active.load(Ordering::Acquire) != generation {
                return;
            }
            let _ = session.query_timeout(&address(&root, index), TIMEOUT).await;
        }
    }
}
impl XAirDriver {
    pub(super) fn start_output_geq(&self, session: Arc<OscSession>, generation: u64) {
        let cache = self.output_geq.clone();
        *cache.lock().unwrap() = Cache {
            generation,
            serial: 0,
            blocks: BTreeMap::new(),
        };
        let mut rx = session.subscribe_received_updates();
        let active = self.connection_generation.clone();
        let since = self.receive_since.clone();
        let updates = cache.clone();
        let recovery_session = session.clone();
        let recovery_active = active.clone();
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((at, msg)) => {
                        if active.load(Ordering::Acquire) != generation {
                            break;
                        }
                        if at >= *since.lock().unwrap() {
                            observe(&updates, generation, at, &msg);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        {
                            let mut cache = updates.lock().unwrap();
                            if cache.generation != generation {
                                break;
                            }
                            cache.serial = cache.serial.wrapping_add(1);
                            cache.blocks.clear();
                        }
                        let session = recovery_session.clone();
                        let active = recovery_active.clone();
                        tokio::spawn(async move { hydrate(session, active, generation).await });
                    }
                    Err(_) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        self.refresh_output_geq(session, generation);
    }

    pub(super) fn refresh_output_geq(&self, session: Arc<OscSession>, generation: u64) {
        {
            let mut cache = self.output_geq.lock().unwrap();
            if cache.generation != generation {
                return;
            }
            cache.serial = cache.serial.wrapping_add(1);
            cache.blocks.clear();
        }
        let active = self.connection_generation.clone();
        let task = tokio::spawn(async move { hydrate(session, active, generation).await });
        self.tasks.lock().unwrap().push(task);
    }
    pub(super) fn geq_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return Vec::new();
        }
        let cache = self.output_geq.lock().unwrap();
        if cache.generation != generation {
            return Vec::new();
        }
        Self::geq_controls_from_cache(&cache, generation)
    }
    fn geq_controls_from_cache(cache: &Cache, generation: u64) -> Vec<DestinationControl> {
        let mut controls = Vec::new();
        for (target, label, _) in targets() {
            let block = cache.blocks.get(&target);
            for index in 0..FIELDS {
                let cell = block.map(|b| &b.cells[index]);
                let current = cell
                    .filter(|c| !c.pending)
                    .and_then(|c| c.value)
                    .map(|value| DestinationValue {
                        value,
                        verified: true,
                    });
                controls.push(DestinationControl {
                    revision: Some(cell.map_or(0, |c| c.revision)),
                    generation: Some(generation),
                    preset: true,
                    members: vec![target.clone()],
                    target: target.clone(),
                    target_label: label.clone(),
                    target_name: None,
                    send_slot: None,
                    surface: "output_geq".into(),
                    description: "Changes processing for this mixer output.".into(),
                    id: field_id(index),
                    label: if index == 0 {
                        "EQ mode".into()
                    } else {
                        format!("{} Hz", fx::GEQ_BANDS[index - 1])
                    },
                    tap_tempo: false,
                    gesture: None,
                    choices: if index == 0 {
                        ["PEQ", "GEQ", "TEQ"]
                            .into_iter()
                            .enumerate()
                            .map(|(i, label)| DestinationChoice {
                                value: i as f32,
                                label: label.into(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    range: if index == 0 {
                        None
                    } else {
                        Some(DestinationRange {
                            min: -15.0,
                            max: 15.0,
                            unit: "dB".into(),
                            decimals: 1,
                            step: Some(0.5),
                        })
                    },
                    current,
                });
            }
        }
        controls
    }
    pub(super) async fn read_output_geq_snapshot(
        &self,
        target: &str,
    ) -> anyhow::Result<Vec<DestinationControl>> {
        let (_, _, root) = targets()
            .into_iter()
            .find(|(key, _, _)| key == target)
            .ok_or_else(|| anyhow::anyhow!("Unknown GEQ target"))?;
        let _dispatch = self.bus_dispatch.read().await;
        let _lane = self.output_geq_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let check = || -> anyhow::Result<()> {
            anyhow::ensure!(
                self.connected.load(Ordering::Acquire)
                    && self.connection_generation.load(Ordering::Acquire) == generation,
                "Output EQ session changed"
            );
            Ok(())
        };
        check()?;
        let session = self.session().await?.as_ref().unwrap().clone();
        check()?;
        let mut values = [0.0; FIELDS];
        let mut receipts = [None; FIELDS];
        for pass in 0..2 {
            for i in 0..FIELDS {
                check()?;
                let (at, reply) = session
                    .query_received_timeout(&address(&root, i), TIMEOUT)
                    .await?;
                check()?;
                let value = decode(i, &reply).ok_or_else(|| {
                    anyhow::anyhow!("Invalid output EQ readback: {}", field_id(i))
                })?;
                if pass == 0 {
                    values[i] = value;
                } else {
                    anyhow::ensure!(
                        values[i] == value,
                        "Output EQ changed during capture: {}",
                        field_id(i)
                    );
                    receipts[i] = Some(at);
                }
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            check()?;
            let session_guard = self.session().await?;
            anyhow::ensure!(
                session_guard
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &session)),
                "Output EQ session changed"
            );
            {
                let cache = self.output_geq.lock().unwrap();
                anyhow::ensure!(
                    cache.generation == generation,
                    "Output EQ cache session changed"
                );
                if cache.blocks.get(target).is_some_and(|block| {
                    block.cells.iter().enumerate().all(|(i, cell)| {
                        !cell.pending
                            && cell.value == Some(values[i])
                            && cell
                                .observed
                                .zip(receipts[i])
                                .is_some_and(|(at, receipt)| at >= receipt)
                    })
                }) {
                    check()?;
                    return Ok(Self::geq_controls_from_cache(&cache, generation)
                        .into_iter()
                        .filter(|c| c.target == target)
                        .collect());
                }
            }
            drop(session_guard);
            anyhow::ensure!(
                Instant::now() < deadline,
                "Output EQ changed or readback incomplete during capture"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub(super) async fn set_geq_control(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let (_, _, root) = targets()
            .into_iter()
            .find(|(key, _, _)| key == target)
            .ok_or_else(|| anyhow::anyhow!("Unknown GEQ target"))?;
        let index = (0..FIELDS)
            .find(|i| field_id(*i) == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown GEQ control"))?;
        let wire = if index == 0 {
            anyhow::ensure!(
                value.is_finite() && value.fract() == 0.0 && (0.0..=2.0).contains(&value),
                "Invalid EQ mode"
            );
            OscType::Int(value as i32)
        } else {
            anyhow::ensure!(
                value.is_finite()
                    && (-15.0..=15.0).contains(&value)
                    && (value * 2.0 - (value * 2.0).round()).abs() < 1e-4,
                "Graphic EQ gain must use 0.5 dB steps"
            );
            OscType::Float((value + 15.0) / 30.0)
        };
        let _dispatch = self.bus_dispatch.read().await;
        let _lane = self.output_geq_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let block = || {
            self.geq_controls()
                .into_iter()
                .filter(|c| c.target == target)
                .collect::<Vec<_>>()
        };
        anyhow::ensure!(
            check() && block() == snapshot,
            "Output EQ changed; reopen the control"
        );
        let _selected = snapshot
            .get(index)
            .filter(|c| {
                c.id == id
                    && c.current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == expected)
            })
            .ok_or_else(|| anyhow::anyhow!("Output EQ value unavailable or changed"))?;
        anyhow::ensure!(
            snapshot.len() == FIELDS
                && snapshot
                    .iter()
                    .all(|c| c.current.as_ref().is_some_and(|v| v.verified)),
            "Wait for complete output EQ readback"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        for (i, row) in snapshot.iter().enumerate() {
            anyhow::ensure!(
                check() && self.connection_generation.load(Ordering::Acquire) == generation,
                "Output EQ session changed"
            );
            let reply = session.query_timeout(&address(&root, i), TIMEOUT).await?;
            anyhow::ensure!(
                decode(i, &reply) == row.current.as_ref().map(|v| v.value),
                "Output EQ changed; reopen the control"
            );
        }
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && block() == snapshot,
            "Output EQ changed; reopen the control"
        );
        {
            let mut cache = self.output_geq.lock().unwrap();
            anyhow::ensure!(cache.generation == generation, "Output EQ session changed");
            let current = cache
                .blocks
                .get(target)
                .ok_or_else(|| anyhow::anyhow!("Output EQ unavailable"))?;
            anyhow::ensure!(
                current
                    .cells
                    .iter()
                    .zip(snapshot)
                    .all(|(cell, row)| !cell.pending
                        && Some(cell.revision) == row.revision
                        && cell.value == row.current.as_ref().map(|v| v.value)),
                "Output EQ changed"
            );
            cache.serial = cache.serial.wrapping_add(1);
            let serial = cache.serial;
            let cell = &mut cache.blocks.get_mut(target).unwrap().cells[index];
            cell.pending = true;
            cell.value = None;
            cell.revision = serial;
        }
        let _pending = PendingGuard {
            cache: self.output_geq.clone(),
            generation,
            target: target.into(),
            index,
            session: session.clone(),
            active: self.connection_generation.clone(),
            address: address(&root, index),
        };
        let session_guard = self.session().await?;
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && session_guard
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &session)),
            "Output EQ session changed"
        );
        if let Err(error) = session
            .send_checked(&address(&root, index), vec![wire], &|| {
                anyhow::ensure!(
                    check() && self.connection_generation.load(Ordering::Acquire) == generation,
                    "Output processor session changed"
                );
                Ok(())
            })
            .await
        {
            anyhow::bail!("Output EQ command delivery uncertain: {error}");
        }
        drop(session_guard);
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            anyhow::ensure!(
                check() && self.connection_generation.load(Ordering::Acquire) == generation,
                "Output EQ result uncertain: session changed"
            );
            if let Ok(reply) = session.query_timeout(&address(&root, index), TIMEOUT).await {
                if decode(index, &reply) == Some(value) {
                    let mut cache = self.output_geq.lock().unwrap();
                    anyhow::ensure!(
                        cache.generation == generation,
                        "Output EQ result uncertain: session changed"
                    );
                    let cell = &mut cache
                        .blocks
                        .get_mut(target)
                        .ok_or_else(|| anyhow::anyhow!("Output EQ result uncertain"))?
                        .cells[index];
                    cell.value = Some(value);
                    cell.pending = false;
                    cell.observed = Some(Instant::now());
                    drop(cache);
                    let current = block();
                    anyhow::ensure!(
                        current.len() == FIELDS
                            && current
                                .iter()
                                .enumerate()
                                .all(|(i, row)| i == index || *row == snapshot[i]),
                        "Output EQ result uncertain: another setting changed"
                    );
                    return current
                        .into_iter()
                        .nth(index)
                        .ok_or_else(|| anyhow::anyhow!("Output EQ result uncertain"));
                }
            }
        }
        anyhow::bail!("Output EQ result uncertain: mixer readback did not match")
    }
}
