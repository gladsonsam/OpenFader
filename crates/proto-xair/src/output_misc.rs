//! XR18 bus/main insert selection and main stereo balance.
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

const FIELDS: usize = 3;
fn fields(root: &str) -> usize {
    if root == "/lr" {
        3
    } else {
        2
    }
}
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
    rows.extend((1..=16).map(|n| {
        (
            format!("ch/{n:02}"),
            format!("Channel {n}"),
            format!("/ch/{n:02}"),
        )
    }));
    rows
}
fn address(root: &str, index: usize) -> String {
    match index {
        0 => format!("{root}/insert/on"),
        1 => format!("{root}/insert/sel"),
        _ => format!("{root}/mix/pan"),
    }
}
fn field_id(index: usize) -> String {
    ["insert_on", "insert_sel", "balance"][index].into()
}
fn decode(root: &str, index: usize, msg: &OscMessage) -> Option<f32> {
    match (index, msg.args.as_slice()) {
        (0, [OscType::Int(i @ 0..=1)]) => Some(*i as f32),
        (1, [OscType::Int(i)]) if *i >= 0 && *i <= if root == "/lr" { 4 } else { 8 } => {
            Some(*i as f32)
        }
        (2, [OscType::Float(v)]) if v.is_finite() && (0.0..=1.0).contains(v) => {
            Some((v * 100.0).round() * 2.0 - 100.0)
        }
        _ => None,
    }
}
fn observe(cache: &Shared, generation: u64, at: Instant, msg: &OscMessage) {
    let Some((target, root, index)) = targets().into_iter().find_map(|(target, _, root)| {
        (0..fields(&root))
            .find(|&i| address(&root, i) == msg.addr)
            .map(|i| (target, root, i))
    }) else {
        return;
    };
    let value = decode(&root, index, msg);
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
        for index in 0..fields(&root) {
            if active.load(Ordering::Acquire) != generation {
                return;
            }
            let _ = session.query_timeout(&address(&root, index), TIMEOUT).await;
        }
    }
}
impl XAirDriver {
    pub(super) fn start_output_misc(&self, session: Arc<OscSession>, generation: u64) {
        let cache = self.output_misc.clone();
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
        self.refresh_output_misc(session, generation);
    }

    pub(super) fn refresh_output_misc(&self, session: Arc<OscSession>, generation: u64) {
        {
            let mut cache = self.output_misc.lock().unwrap();
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
    pub(super) fn misc_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return Vec::new();
        }
        let cache = self.output_misc.lock().unwrap();
        if cache.generation != generation {
            return Vec::new();
        }
        let mut controls = Vec::new();
        for (target, label, root) in targets() {
            let block = cache.blocks.get(&target);
            for index in 0..fields(&root) {
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
                    preset: false,
                    members: if root.starts_with("/ch/") {
                        let n: u8 = target[3..].parse().unwrap();
                        let first = if n % 2 == 0 { n - 1 } else { n };
                        vec![format!("ch/{first:02}"), format!("ch/{:02}", first + 1)]
                    } else {
                        vec![target.clone()]
                    },
                    target: target.clone(),
                    target_label: label.clone(),
                    target_name: None,
                    send_slot: None,
                    surface: if root.starts_with("/ch/") {
                        "input_insert"
                    } else {
                        "output_controls"
                    }
                    .into(),
                    description: if root.starts_with("/ch/") {
                        "Changes this channel's insert."
                    } else {
                        "Changes processing for this mixer output."
                    }
                    .into(),
                    id: field_id(index),
                    label: ["Insert", "Insert source", "Stereo balance"][index].into(),
                    tap_tempo: false,
                    gesture: None,
                    choices: match index {
                        0 => ["Off", "On"]
                            .iter()
                            .enumerate()
                            .map(|(i, label)| DestinationChoice {
                                value: i as f32,
                                label: (*label).into(),
                            })
                            .collect(),
                        1 => {
                            let labels: &[&str] = if root == "/lr" {
                                &["Off", "FX 1", "FX 2", "FX 3", "FX 4"]
                            } else {
                                &[
                                    "Off", "FX 1 A", "FX 1 B", "FX 2 A", "FX 2 B", "FX 3 A",
                                    "FX 3 B", "FX 4 A", "FX 4 B",
                                ]
                            };
                            labels
                                .iter()
                                .enumerate()
                                .map(|(i, label)| DestinationChoice {
                                    value: i as f32,
                                    label: (*label).into(),
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    },
                    range: if index == 2 {
                        Some(DestinationRange {
                            min: -100.0,
                            max: 100.0,
                            unit: "".into(),
                            decimals: 0,
                            step: Some(2.0),
                        })
                    } else {
                        None
                    },
                    current,
                });
            }
        }
        controls
    }
    pub(super) async fn set_misc_control(
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
            .ok_or_else(|| anyhow::anyhow!("Unknown output control target"))?;
        let index = (0..fields(&root))
            .find(|i| field_id(*i) == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown output control control"))?;
        let wire = match index {
            0 => {
                anyhow::ensure!(
                    value.is_finite() && value.fract() == 0.0 && (0.0..=1.0).contains(&value),
                    "Invalid insert state"
                );
                OscType::Int(value as i32)
            }
            1 => {
                let max = if root == "/lr" { 4.0 } else { 8.0 };
                anyhow::ensure!(
                    value.is_finite() && value.fract() == 0.0 && (0.0..=max).contains(&value),
                    "Invalid insert source"
                );
                OscType::Int(value as i32)
            }
            _ => {
                anyhow::ensure!(
                    value.is_finite()
                        && (-100.0..=100.0).contains(&value)
                        && (value / 2.0 - (value / 2.0).round()).abs() < 1e-4,
                    "Balance must use 2-unit steps"
                );
                OscType::Float((value + 100.0) / 200.0)
            }
        };
        let _lane = self.output_misc_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let block = || {
            self.misc_controls()
                .into_iter()
                .filter(|c| c.target == target)
                .collect::<Vec<_>>()
        };
        anyhow::ensure!(
            check() && block() == snapshot,
            "Output control changed; reopen the control"
        );
        let _selected = snapshot
            .get(index)
            .filter(|c| {
                c.id == id
                    && c.current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == expected)
            })
            .ok_or_else(|| anyhow::anyhow!("Output control value unavailable or changed"))?;
        anyhow::ensure!(
            snapshot.len() == fields(&root)
                && snapshot
                    .iter()
                    .all(|c| c.current.as_ref().is_some_and(|v| v.verified)),
            "Wait for complete output EQ readback"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        for (i, row) in snapshot.iter().enumerate() {
            anyhow::ensure!(
                check() && self.connection_generation.load(Ordering::Acquire) == generation,
                "Output control session changed"
            );
            let reply = session.query_timeout(&address(&root, i), TIMEOUT).await?;
            anyhow::ensure!(
                decode(&root, i, &reply) == row.current.as_ref().map(|v| v.value),
                "Output control changed; reopen the control"
            );
        }
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && block() == snapshot,
            "Output control changed; reopen the control"
        );
        {
            let mut cache = self.output_misc.lock().unwrap();
            anyhow::ensure!(
                cache.generation == generation,
                "Output control session changed"
            );
            let current = cache
                .blocks
                .get(target)
                .ok_or_else(|| anyhow::anyhow!("Output control unavailable"))?;
            anyhow::ensure!(
                current
                    .cells
                    .iter()
                    .zip(snapshot)
                    .all(|(cell, row)| !cell.pending
                        && Some(cell.revision) == row.revision
                        && cell.value == row.current.as_ref().map(|v| v.value)),
                "Output control changed"
            );
            cache.serial = cache.serial.wrapping_add(1);
            let serial = cache.serial;
            let cell = &mut cache.blocks.get_mut(target).unwrap().cells[index];
            cell.pending = true;
            cell.value = None;
            cell.revision = serial;
        }
        let _pending = PendingGuard {
            cache: self.output_misc.clone(),
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
            "Output control session changed"
        );
        if let Err(error) = session.send(&address(&root, index), vec![wire]).await {
            anyhow::bail!("Output control command delivery uncertain: {error}");
        }
        drop(session_guard);
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            anyhow::ensure!(
                check() && self.connection_generation.load(Ordering::Acquire) == generation,
                "Output control result uncertain: session changed"
            );
            if let Ok(reply) = session.query_timeout(&address(&root, index), TIMEOUT).await {
                if decode(&root, index, &reply) == Some(value) {
                    let mut cache = self.output_misc.lock().unwrap();
                    anyhow::ensure!(
                        cache.generation == generation,
                        "Output control result uncertain: session changed"
                    );
                    let cell = &mut cache
                        .blocks
                        .get_mut(target)
                        .ok_or_else(|| anyhow::anyhow!("Output control result uncertain"))?
                        .cells[index];
                    cell.value = Some(value);
                    cell.pending = false;
                    cell.observed = Some(Instant::now());
                    drop(cache);
                    let current = block();
                    anyhow::ensure!(
                        current.len() == fields(&root)
                            && current
                                .iter()
                                .enumerate()
                                .all(|(i, row)| i == index || *row == snapshot[i]),
                        "Output control result uncertain: another setting changed"
                    );
                    return current
                        .into_iter()
                        .nth(index)
                        .ok_or_else(|| anyhow::anyhow!("Output control result uncertain"));
                }
            }
        }
        anyhow::bail!("Output control result uncertain: mixer readback did not match")
    }
}
