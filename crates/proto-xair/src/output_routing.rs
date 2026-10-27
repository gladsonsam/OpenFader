//! XR18 physical output source and tap controls. OSC has no atomic compare-and-swap.
use super::*;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationValue};
use openfader_core::routing::{OutputPlane, RouteState};
use std::sync::atomic::Ordering;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

const TAP_LABELS: [&str; 11] = [
    "Analog input",
    "Analog input + mute",
    "Input",
    "Input + mute",
    "Pre EQ",
    "Pre EQ + mute",
    "Post EQ",
    "Post EQ + mute",
    "Pre fader",
    "Pre fader + mute",
    "Post fader",
];
const SHARED: &[(&str, &str, usize)] = &[
    ("Ch", "Channels", 16),
    ("Aux", "Aux input", 2),
    ("FxReturn", "FX returns", 8),
    ("Bus", "Buses", 6),
    ("FxSend", "FX sends", 4),
    ("Main", "Main L/R", 2),
    ("USB", "USB returns", 18),
];
const DIGITAL: &[(&str, &str, usize)] = &[
    ("Ch", "Channels", 16),
    ("Aux", "Aux input", 2),
    ("FxReturn", "FX returns", 8),
    ("Bus", "Buses", 6),
    ("FxSend", "FX sends", 4),
    ("Main", "Main L/R", 2),
];
const STEREO: &[(&str, &str, usize)] = &[
    ("Main", "Main L/R", 1),
    ("Monitor", "Monitor L/R", 1),
    ("USBPair", "USB pairs", 9),
];

pub fn planes() -> Vec<OutputPlane> {
    [
        ("physical/aux", "Aux output", 6, SHARED),
        ("physical/p16", "Ultranet output", 16, SHARED),
        ("physical/usb", "USB output", 18, DIGITAL),
    ]
    .into_iter()
    .map(|(key, label, count, sources)| OutputPlane {
        key,
        label,
        count,
        clearable: false,
        sources: sources.to_vec(),
    })
    .collect()
}
pub fn stereo_sources() -> Vec<(&'static str, &'static str, usize)> {
    STEREO.to_vec()
}

#[derive(Clone)]
struct Target {
    id: String,
    label: String,
    src: String,
    tap: Option<String>,
    groups: &'static [(&'static str, &'static str, usize)],
}
fn targets() -> Vec<Target> {
    let mut all = vec![
        Target {
            id: "physical/main".into(),
            label: "Main output".into(),
            src: "/routing/main/01/src".into(),
            tap: None,
            groups: STEREO,
        },
        Target {
            id: "physical/phones".into(),
            label: "Phones output".into(),
            src: "/routing/main/02/src".into(),
            tap: None,
            groups: STEREO,
        },
    ];
    for (family, wire, label, count, groups) in [
        ("aux", "aux", "Aux output", 6, SHARED),
        ("p16", "p16", "Ultranet output", 16, SHARED),
        ("usb", "usb", "USB output", 18, DIGITAL),
    ] {
        for n in 1..=count {
            let root = format!("/routing/{wire}/{n:02}");
            all.push(Target {
                id: format!("physical/{family}/{n:02}"),
                label: format!("{label} {n}"),
                src: format!("{root}/src"),
                tap: Some(format!("{root}/pos")),
                groups,
            });
        }
    }
    all
}
fn target(id: &str) -> anyhow::Result<Target> {
    targets()
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow::anyhow!("Unknown physical output"))
}
fn decode(groups: &[(&str, &str, usize)], mut raw: i32) -> Option<String> {
    if raw < 0 {
        return None;
    }
    for (key, _, count) in groups {
        if raw < *count as i32 {
            return Some(format!("{key} {}", raw + 1));
        }
        raw -= *count as i32;
    }
    None
}
fn encode(groups: &[(&str, &str, usize)], label: &str) -> Option<i32> {
    let (group, index) = label.split_once(' ')?;
    let index: usize = index.parse().ok()?;
    let mut offset = 0;
    for (key, _, count) in groups {
        if *key == group && (1..=*count).contains(&index) {
            return Some(offset + index as i32 - 1);
        }
        offset += *count as i32;
    }
    None
}
#[derive(Clone, Default)]
struct Cell {
    source: Option<i32>,
    tap: Option<i32>,
    revision: u64,
    pending: bool,
    source_at: Option<Instant>,
    tap_at: Option<Instant>,
}
#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    next_revision: u64,
    cells: BTreeMap<String, Cell>,
}
type Shared = Arc<StdMutex<Cache>>;
fn observe(cache: &Shared, generation: u64, when: Instant, msg: &OscMessage) {
    if !msg.addr.starts_with("/routing/") {
        return;
    }
    let [OscType::Int(value)] = msg.args.as_slice() else {
        return;
    };
    let Some((target, is_tap)) = targets().into_iter().find_map(|t| {
        if t.src == msg.addr {
            Some((t, false))
        } else if t.tap.as_deref() == Some(msg.addr.as_str()) {
            Some((t, true))
        } else {
            None
        }
    }) else {
        return;
    };
    if (is_tap && !(0..=10).contains(value)) || (!is_tap && decode(target.groups, *value).is_none())
    {
        return;
    }
    let mut cache = cache.lock().unwrap();
    if cache.generation != generation {
        return;
    }
    let changed = {
        let cell = cache.cells.entry(target.id.clone()).or_default();
        let (old, at) = if is_tap {
            (&mut cell.tap, &mut cell.tap_at)
        } else {
            (&mut cell.source, &mut cell.source_at)
        };
        if at.is_some_and(|prior| prior > when) {
            return;
        }
        let changed = *old != Some(*value);
        *old = Some(*value);
        *at = Some(when);
        changed
    };
    if changed {
        cache.next_revision = cache.next_revision.wrapping_add(1);
        let revision = cache.next_revision;
        cache.cells.get_mut(&target.id).unwrap().revision = revision;
    }
}
impl XAirDriver {
    pub(super) fn start_output_routing(&self, session: Arc<OscSession>, generation: u64) {
        let cache = self.output_routes.clone();
        *cache.lock().unwrap() = Cache {
            generation,
            next_revision: 0,
            cells: BTreeMap::new(),
        };
        let mut rx = session.subscribe_received_updates();
        let since = self.receive_since.clone();
        let active = self.connection_generation.clone();
        let writer_cache = cache.clone();
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((at, msg)) => {
                        if active.load(Ordering::Acquire) != generation {
                            break;
                        }
                        if at >= *since.lock().unwrap() {
                            observe(&writer_cache, generation, at, &msg);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let mut cache = writer_cache.lock().unwrap();
                        cache.cells.clear();
                        cache.next_revision = cache.next_revision.wrapping_add(1);
                    }
                    Err(_) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        self.refresh_output_routing(session, generation);
    }
    pub(super) fn refresh_output_routing(&self, session: Arc<OscSession>, generation: u64) {
        {
            let mut cache = self.output_routes.lock().unwrap();
            if cache.generation != generation {
                return;
            }
            cache.cells.clear();
            cache.next_revision = cache.next_revision.wrapping_add(1);
        }
        let active = self.connection_generation.clone();
        let task = tokio::spawn(async move {
            for t in targets() {
                for address in std::iter::once(&t.src).chain(t.tap.iter()) {
                    if active.load(Ordering::Acquire) != generation {
                        return;
                    }
                    let _ = session
                        .query_timeout(address, Duration::from_millis(350))
                        .await;
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }
    pub fn output_route(&self, id: &str) -> anyhow::Result<RouteState> {
        let t = target(id)?;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            self.connected.load(Ordering::Acquire),
            "Output session unavailable"
        );
        let cache = self.output_routes.lock().unwrap();
        anyhow::ensure!(cache.generation == generation, "Output session changed");
        let cell = cache
            .cells
            .get(id)
            .filter(|c| !c.pending)
            .ok_or_else(|| anyhow::anyhow!("Waiting for output readback"))?;
        let src = cell
            .source
            .and_then(|v| decode(t.groups, v))
            .ok_or_else(|| anyhow::anyhow!("Output source unavailable"))?;
        if t.tap.is_some() {
            anyhow::ensure!(cell.tap.is_some(), "Output tap unavailable")
        };
        let since = *self.receive_since.lock().unwrap();
        anyhow::ensure!(
            cell.source_at.is_some_and(|at| at >= since)
                && (t.tap.is_none() || cell.tap_at.is_some_and(|at| at >= since)),
            "Waiting for current output readback"
        );
        Ok(RouteState {
            source: src,
            revision: cell.revision,
            generation,
        })
    }
    pub(super) fn routing_controls(&self) -> Vec<DestinationControl> {
        let generation = self.connection_generation.load(Ordering::Acquire);
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let cache = self.output_routes.lock().unwrap();
        if cache.generation != generation {
            return vec![];
        }
        let since = *self.receive_since.lock().unwrap();
        targets()
            .into_iter()
            .filter(|t| t.tap.is_some())
            .filter_map(|t| {
                let cell = cache.cells.get(&t.id).filter(|c| {
                    !c.pending
                        && c.source.is_some()
                        && c.source_at.is_some_and(|at| at >= since)
                        && c.tap_at.is_some_and(|at| at >= since)
                })?;
                let value = cell.tap?;
                Some(DestinationControl {
                    revision: Some(cell.revision),
                    generation: Some(generation),
                    preset: false,
                    members: vec![t.id.clone()],
                    target: t.id,
                    target_label: t.label,
                    target_name: None,
                    send_slot: None,
                    surface: "routing".into(),
                    description: "Select the signal tap for this physical output.".into(),
                    id: "tap".into(),
                    label: "Tap position".into(),
                    tap_tempo: false,
                    gesture: None,
                    choices: TAP_LABELS
                        .iter()
                        .enumerate()
                        .map(|(i, label)| DestinationChoice {
                            value: i as f32,
                            label: (*label).into(),
                        })
                        .collect(),
                    range: None,
                    current: Some(DestinationValue {
                        value: value as f32,
                        verified: true,
                    }),
                })
            })
            .collect()
    }
    pub async fn set_output_route_guarded(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        let t = target(id)?;
        let value = encode(t.groups, source)
            .ok_or_else(|| anyhow::anyhow!("Output source is not advertised"))?;
        self.write_output(&t, false, value, expected, check).await
    }
    pub(super) async fn set_routing_tap(
        &self,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(
            value.is_finite() && value.fract() == 0.0 && (0.0..=10.0).contains(&value),
            "Invalid tap"
        );
        let before = self
            .routing_controls()
            .into_iter()
            .find(|c| c.target == id)
            .ok_or_else(|| anyhow::anyhow!("Tap unavailable"))?;
        anyhow::ensure!(
            snapshot == [before.clone()]
                && before
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "Output tap changed"
        );
        let route = self.output_route(id)?;
        anyhow::ensure!(
            before.revision == Some(route.revision),
            "Output source or tap changed"
        );
        let t = target(id)?;
        self.write_output(&t, true, value as i32, &route, check)
            .await?;
        self.routing_controls()
            .into_iter()
            .find(|c| c.target == id)
            .ok_or_else(|| anyhow::anyhow!("Tap write result uncertain"))
    }
    async fn write_output(
        &self,
        t: &Target,
        tap: bool,
        value: i32,
        expected: &RouteState,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        let _lane = self.output_route_write.lock().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            check() && self.output_route(&t.id)? == *expected,
            "Output session or route changed"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        let src = read_int(&session, &t.src).await?;
        let pos = if let Some(addr) = &t.tap {
            Some(read_int(&session, addr).await?)
        } else {
            None
        };
        {
            let cache = self.output_routes.lock().unwrap();
            let cell = cache
                .cells
                .get(&t.id)
                .ok_or_else(|| anyhow::anyhow!("Output unavailable"))?;
            anyhow::ensure!(
                cell.source == Some(src)
                    && cell.tap == pos
                    && !cell.pending
                    && cell.revision == expected.revision
                    && generation == cache.generation,
                "Output source or tap changed"
            );
        }
        anyhow::ensure!(
            check() && self.connection_generation.load(Ordering::Acquire) == generation,
            "Output session changed"
        );
        {
            let mut cache = self.output_routes.lock().unwrap();
            anyhow::ensure!(
                cache
                    .cells
                    .get(&t.id)
                    .is_some_and(|cell| cell.revision == expected.revision && !cell.pending),
                "Output changed"
            );
            cache.next_revision = cache.next_revision.wrapping_add(1);
            let revision = cache.next_revision;
            let cell = cache.cells.get_mut(&t.id).unwrap();
            cell.pending = true;
            cell.revision = revision;
        }
        let address = if tap {
            t.tap
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No tap for output"))?
        } else {
            &t.src
        };
        if let Err(e) = session.send(address, vec![OscType::Int(value)]).await {
            anyhow::bail!("Output command delivery uncertain: {e}")
        }
        // Drain earlier socket traffic before requesting independent readback.
        // OSC over UDP has no request IDs; this barrier reduces, but cannot
        // eliminate, ambiguity from delayed peer broadcasts.
        if session
            .query_timeout(addrs::INFO, Duration::from_millis(500))
            .await
            .is_err()
        {
            anyhow::bail!("Output command sent but readback barrier failed; result uncertain")
        }
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            if !check() || self.connection_generation.load(Ordering::Acquire) != generation {
                anyhow::bail!("Output command result uncertain: session changed")
            };
            if let Ok(read) = read_int(&session, address).await {
                if read == value {
                    let mut cache = self.output_routes.lock().unwrap();
                    anyhow::ensure!(
                        cache.generation == generation
                            && cache.cells.get(&t.id).is_some_and(|cell| cell.pending
                                && if tap {
                                    cell.source == Some(src)
                                } else {
                                    cell.tap == pos
                                }),
                        "Output command result uncertain: companion setting changed"
                    );
                    cache.next_revision = cache.next_revision.wrapping_add(1);
                    let revision = cache.next_revision;
                    let cell = cache
                        .cells
                        .get_mut(&t.id)
                        .ok_or_else(|| anyhow::anyhow!("Output command result uncertain"))?;
                    if tap {
                        cell.tap = Some(read);
                        cell.tap_at = Some(Instant::now())
                    } else {
                        cell.source = Some(read);
                        cell.source_at = Some(Instant::now())
                    }
                    cell.pending = false;
                    cell.revision = revision;
                    return Ok(());
                }
            }
        }
        anyhow::bail!("Output command sent but readback unconfirmed; reconnect to refresh")
    }
}
async fn read_int(session: &OscSession, address: &str) -> anyhow::Result<i32> {
    let reply = session
        .query_with_args_timeout(address, Vec::new(), Duration::from_millis(500), |msg| {
            matches!(msg.args.as_slice(), [OscType::Int(_)])
        })
        .await?;
    match reply.args.as_slice() {
        [OscType::Int(v)] => Ok(*v),
        _ => anyhow::bail!("Invalid output routing reply for {address}"),
    }
}
