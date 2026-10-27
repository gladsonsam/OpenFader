//! XR18 shared automix group enable and lock settings.
//! Readback uses the existing OSC session and no page-owned ticker.
use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use std::{
    collections::BTreeMap,
    sync::atomic::Ordering,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

const FIELDS: usize = 4;
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
    vec![("automix".into(), "Automix".into(), "".into())]
}
fn address(_: &str, index: usize) -> String {
    [
        "/config/amixenable/X",
        "/config/amixenable/Y",
        "/config/amixlock/X",
        "/config/amixlock/Y",
    ][index]
        .into()
}
fn field_id(index: usize) -> String {
    ["enable_x", "enable_y", "lock_x", "lock_y"][index].into()
}
fn decode(_: usize, msg: &OscMessage) -> Option<f32> {
    match msg.args.as_slice() {
        [OscType::Int(i @ 0..=1)] => Some(*i as f32),
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
    pub(super) async fn read_automix_snapshot(&self) -> anyhow::Result<[bool; 4]> {
        let _lane = self.automix_settings_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let session = self.session().await?.as_ref().unwrap().clone();
        let mut values = [false; 4];
        for pass in 0..2 {
            for (i, value) in values.iter_mut().enumerate() {
                anyhow::ensure!(
                    self.connection_generation.load(Ordering::Acquire) == generation,
                    "Automix session changed"
                );
                let reply = session.query_timeout(&address("", i), TIMEOUT).await?;
                let v = decode(i, &reply)
                    .ok_or_else(|| anyhow::anyhow!("Automix setting unavailable"))?
                    == 1.0;
                if pass == 0 {
                    *value = v;
                } else {
                    anyhow::ensure!(*value == v, "Automix settings changed during capture");
                }
                observe(&self.automix_settings, generation, Instant::now(), &reply);
            }
        }
        anyhow::ensure!(
            self.connection_generation.load(Ordering::Acquire) == generation,
            "Automix session changed"
        );
        Ok(values)
    }
    pub(super) fn start_automix_settings(&self, session: Arc<OscSession>, generation: u64) {
        let cache = self.automix_settings.clone();
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
        self.refresh_automix_settings(session, generation);
    }

    pub(super) fn refresh_automix_settings(&self, session: Arc<OscSession>, generation: u64) {
        {
            let mut cache = self.automix_settings.lock().unwrap();
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
    pub(super) fn automix_setting_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return Vec::new();
        }
        let cache = self.automix_settings.lock().unwrap();
        if cache.generation != generation {
            return Vec::new();
        }
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
                    preset: false,
                    members: vec![],
                    target: target.clone(),
                    target_label: label.clone(),
                    target_name: None,
                    send_slot: None,
                    surface: "automix_settings".into(),
                    description: "Changes a shared mixer automix group.".into(),
                    id: field_id(index),
                    label: [
                        "Group X enabled",
                        "Group Y enabled",
                        "Group X locked",
                        "Group Y locked",
                    ][index]
                        .into(),
                    tap_tempo: false,
                    gesture: None,
                    choices: ["Off", "On"]
                        .iter()
                        .enumerate()
                        .map(|(i, label)| DestinationChoice {
                            value: i as f32,
                            label: (*label).into(),
                        })
                        .collect(),
                    range: None,
                    current,
                });
            }
        }
        controls
    }
    pub(super) async fn set_automix_setting(
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
            .ok_or_else(|| anyhow::anyhow!("Unknown automix target"))?;
        let index = (0..FIELDS)
            .find(|i| field_id(*i) == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown automix control"))?;
        anyhow::ensure!(
            value.is_finite() && value.fract() == 0.0 && (0.0..=1.0).contains(&value),
            "Invalid automix setting"
        );
        let wire = OscType::Int(value as i32);
        let _lane = self.automix_settings_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let block = || {
            self.automix_setting_controls()
                .into_iter()
                .filter(|c| c.target == target)
                .collect::<Vec<_>>()
        };
        anyhow::ensure!(
            check() && block() == snapshot,
            "Automix settings changed; reopen the control"
        );
        let _selected = snapshot
            .get(index)
            .filter(|c| {
                c.id == id
                    && c.current
                        .as_ref()
                        .is_some_and(|v| v.verified && v.value == expected)
            })
            .ok_or_else(|| anyhow::anyhow!("Automix settings value unavailable or changed"))?;
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
                "Automix settings session changed"
            );
            let reply = session.query_timeout(&address(&root, i), TIMEOUT).await?;
            anyhow::ensure!(
                decode(i, &reply) == row.current.as_ref().map(|v| v.value),
                "Automix settings changed; reopen the control"
            );
        }
        anyhow::ensure!(
            check()
                && self.connection_generation.load(Ordering::Acquire) == generation
                && block() == snapshot,
            "Automix settings changed; reopen the control"
        );
        {
            let mut cache = self.automix_settings.lock().unwrap();
            anyhow::ensure!(
                cache.generation == generation,
                "Automix settings session changed"
            );
            let current = cache
                .blocks
                .get(target)
                .ok_or_else(|| anyhow::anyhow!("Automix settings unavailable"))?;
            anyhow::ensure!(
                current
                    .cells
                    .iter()
                    .zip(snapshot)
                    .all(|(cell, row)| !cell.pending
                        && Some(cell.revision) == row.revision
                        && cell.value == row.current.as_ref().map(|v| v.value)),
                "Automix settings changed"
            );
            cache.serial = cache.serial.wrapping_add(1);
            let serial = cache.serial;
            let cell = &mut cache.blocks.get_mut(target).unwrap().cells[index];
            cell.pending = true;
            cell.value = None;
            cell.revision = serial;
        }
        let _pending = PendingGuard {
            cache: self.automix_settings.clone(),
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
            "Automix settings session changed"
        );
        if let Err(error) = session.send(&address(&root, index), vec![wire]).await {
            anyhow::bail!("Automix settings command delivery uncertain: {error}");
        }
        drop(session_guard);
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            anyhow::ensure!(
                check() && self.connection_generation.load(Ordering::Acquire) == generation,
                "Automix settings result uncertain: session changed"
            );
            if let Ok(reply) = session.query_timeout(&address(&root, index), TIMEOUT).await {
                if decode(index, &reply) == Some(value) {
                    let mut cache = self.automix_settings.lock().unwrap();
                    anyhow::ensure!(
                        cache.generation == generation,
                        "Automix settings result uncertain: session changed"
                    );
                    let cell = &mut cache
                        .blocks
                        .get_mut(target)
                        .ok_or_else(|| anyhow::anyhow!("Automix settings result uncertain"))?
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
                        "Automix settings result uncertain: another setting changed"
                    );
                    return current
                        .into_iter()
                        .nth(index)
                        .ok_or_else(|| anyhow::anyhow!("Automix settings result uncertain"));
                }
            }
        }
        anyhow::bail!("Automix settings result uncertain: mixer readback did not match")
    }
}
