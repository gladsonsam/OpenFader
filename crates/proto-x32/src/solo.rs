//! Solo switches: `/-stat/solosw/[01..80]` (`1` = soloed).
//!
//! Index map (v4.09-02): 01..32 ch, 33..40 auxin, 41..48 fxrtn,
//! 49..64 bus, 65..70 mtx, 71 main LR, 72 mono, 73..80 DCA 1..8.
//!
//! Descriptors serve only fresh independently-observed rows on the live
//! session (same TTL rule as the route plane); anything cold or stale is
//! omitted, never synthesized. Canonical UI targets: `dca` masters key
//! as `vca/NN`, every other family keys as its strip key (`ch/NN`,
//! `auxin/NN`, `fxrtn/NN`, `bus/NN`, `mtx/NN`, `main`, `mainm`).
use crate::{mapping::fam_of, X32Driver};
use anyhow::ensure;
use openfader_core::{
    destinations::{DestinationChoice, DestinationControl, DestinationValue},
    StripId,
};
use std::{
    collections::HashMap,
    sync::atomic::AtomicU64,
    time::{Duration, Instant},
};

/// How long a verified solo observation stays servable to sync readers.
pub const SOLO_CACHE_TTL: Duration = Duration::from_secs(5);
/// Renewal cadence for the session-owned maintenance task.
const SOLO_RENEW_EVERY: Duration = Duration::from_secs(2);
const SOLO_RENEW_AFTER: Duration = Duration::from_secs(3);
const SOLO_FANOUT: usize = 8;

struct CachedSolo {
    value: bool,
    revision: u64,
    at: Instant,
}

static SOLO_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<(u64, u8), CachedSolo>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
static NEXT_SOLO_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> anyhow::Result<u64> {
    let mut current = NEXT_SOLO_REVISION.load(std::sync::atomic::Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("x32: solo revision counter exhausted"))?;
        match NEXT_SOLO_REVISION.compare_exchange_weak(
            current,
            next,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        ) {
            Ok(_) => return Ok(next),
            Err(actual) => current = actual,
        }
    }
}

fn remember_solo(index: u8, generation: u64, value: bool) -> anyhow::Result<u64> {
    let mut cache = SOLO_CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: solo cache unavailable"))?;
    if cache.len() > 256 {
        // Several independent engine sessions may be live at once. Prune
        // expired observations without erasing another session's fresh bank.
        cache.retain(|_, entry| entry.at.elapsed() < SOLO_CACHE_TTL);
    }
    let key = (generation, index);
    let revision = match cache.get(&key) {
        Some(prev) if prev.value == value => prev.revision,
        _ => next_revision()?,
    };
    cache.insert(
        key,
        CachedSolo {
            value,
            revision,
            at: Instant::now(),
        },
    );
    Ok(revision)
}

fn invalidate_one(generation: u64, index: u8) {
    if let Ok(mut cache) = SOLO_CACHE.lock() {
        cache.remove(&(generation, index));
    }
}

/// Drop every cached solo row for a session (missed/lagged pushes make
/// the whole view suspect; the maintenance task re-reads all of it).
pub(crate) fn invalidate_all_solos(generation: u64) {
    if let Ok(mut cache) = SOLO_CACHE.lock() {
        cache.retain(|(gen, _), _| *gen != generation);
    }
}

fn stale_solo_ids(generation: u64, older_than: Duration) -> Vec<u8> {
    SOLO_CACHE
        .lock()
        .map(|cache| {
            cache
                .iter()
                .filter(|((gen, _), entry)| *gen == generation && entry.at.elapsed() > older_than)
                .map(|((_, index), _)| *index)
                .collect()
        })
        .unwrap_or_default()
}

/// Demand signal from the push plane to the session-owned solo task.
#[derive(Debug)]
pub(crate) enum SoloRefresh {
    One(u8),
    All,
}

/// Push-plane entry: record a well-formed solo leaf directly, invalidate
/// and re-read on malformed leaves. Stale generations are ignored.
pub(crate) fn note_solo_push(
    msg: &rosc::OscMessage,
    generation: u64,
    dirty: &tokio::sync::mpsc::UnboundedSender<SoloRefresh>,
) {
    let Some(rest) = msg.addr.strip_prefix("/-stat/solosw/") else {
        return;
    };
    let Ok(index) = rest.parse::<u8>() else {
        return;
    };
    if !(1..=80).contains(&index) {
        return;
    }
    match crate::mapping::strict_bool(msg) {
        Ok(value) => {
            let _ = remember_solo(index, generation, value);
        }
        Err(_) => {
            invalidate_one(generation, index);
            let _ = dirty.send(SoloRefresh::One(index));
        }
    }
}

pub fn solosw_index(strip: StripId) -> anyhow::Result<u8> {
    let i = strip.0 as usize;
    let fam = crate::addrs::X32_TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .ok_or_else(|| anyhow::anyhow!("x32: strip out of range"))?;
    let index = match fam.key {
        "ch" => i - fam.first + 1,
        "auxin" => 33 + (i - fam.first),
        "fxrtn" => 41 + (i - fam.first),
        "bus" => 49 + (i - fam.first),
        "mtx" => 65 + (i - fam.first),
        "main" => 71,
        "mainm" => 72,
        "dca" => 73 + (i - fam.first),
        _ => anyhow::bail!("x32: unknown family"),
    };
    Ok(index as u8)
}

/// Canonical UI target + label for a 1-based solosw index.
pub fn solo_target(index: u8) -> anyhow::Result<(String, String)> {
    Ok(match index {
        1..=32 => (format!("ch/{index:02}"), format!("Ch {index:02}")),
        33..=40 => {
            let n = index - 32;
            (format!("auxin/{n:02}"), format!("AuxIn {n:02}"))
        }
        41..=48 => {
            let n = index - 40;
            (format!("fxrtn/{n:02}"), format!("FxRtn {n:02}"))
        }
        49..=64 => {
            let n = index - 48;
            (format!("bus/{n:02}"), format!("Bus {n:02}"))
        }
        65..=70 => {
            let n = index - 64;
            (format!("mtx/{n:02}"), format!("Mtx {n:02}"))
        }
        71 => ("main".to_string(), "Main LR".to_string()),
        72 => ("mainm".to_string(), "Main M/C".to_string()),
        73..=80 => {
            let n = index - 72;
            (format!("vca/{n:02}"), format!("DCA {n}"))
        }
        _ => anyhow::bail!("x32: solo index {index} out of range 1..80"),
    })
}

/// Reverse map: canonical UI target -> solosw index.
pub fn solo_index_for_target(target: &str) -> anyhow::Result<u8> {
    if target == "main" {
        return Ok(71);
    }
    if target == "mainm" {
        return Ok(72);
    }
    if let Some(rest) = target.strip_prefix("vca/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=8).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(72 + n);
    }
    if let Some(rest) = target.strip_prefix("ch/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=32).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(n);
    }
    if let Some(rest) = target.strip_prefix("auxin/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=8).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(32 + n);
    }
    if let Some(rest) = target.strip_prefix("fxrtn/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=8).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(40 + n);
    }
    if let Some(rest) = target.strip_prefix("bus/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=16).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(48 + n);
    }
    if let Some(rest) = target.strip_prefix("mtx/") {
        let n: u8 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: unknown solo target {target:?}"))?;
        ensure!((1..=6).contains(&n), "x32: unknown solo target {target:?}");
        return Ok(64 + n);
    }
    anyhow::bail!("x32: unknown solo target {target:?}")
}

fn solo_row(
    target: &str,
    label: &str,
    observed: Option<(bool, u64)>,
    generation: u64,
) -> DestinationControl {
    DestinationControl {
        revision: observed.map(|(_, revision)| revision),
        generation: Some(generation),
        preset: false,
        members: vec![target.to_string()],
        target: target.to_string(),
        target_label: label.to_string(),
        target_name: None,
        send_slot: None,
        surface: "solo".to_string(),
        description: "Monitoring Solo switch.".to_string(),
        id: "solo".to_string(),
        label: "Solo".to_string(),
        tap_tempo: false,
        gesture: None,
        choices: vec![
            DestinationChoice {
                value: 0.0,
                label: "Off".to_string(),
            },
            DestinationChoice {
                value: 1.0,
                label: "On".to_string(),
            },
        ],
        range: None,
        current: observed.map(|(value, _)| DestinationValue {
            value: if value { 1.0 } else { 0.0 },
            verified: true,
        }),
    }
}

async fn refresh_solo_on(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    index: u8,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        crate::routing::generation_for_session(session)? == generation,
        "x32: solo session changed during refresh"
    );
    let path = format!("/-stat/solosw/{index:02}");
    let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
    anyhow::ensure!(
        crate::routing::generation_for_session(session)? == generation,
        "x32: solo session changed during refresh"
    );
    let value = crate::mapping::strict_bool(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
    let _ = remember_solo(index, generation, value);
    Ok(())
}

async fn refresh_ids(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    ids: &[u8],
) {
    let mut set = tokio::task::JoinSet::new();
    for chunk in ids.chunks(ids.len().div_ceil(SOLO_FANOUT).max(1)) {
        let session = std::sync::Arc::clone(session);
        let chunk = chunk.to_vec();
        set.spawn(async move {
            for index in &chunk {
                if let Err(error) = refresh_solo_on(&session, generation, *index).await {
                    eprintln!("x32: solo refresh {index:02} unavailable: {error:#}");
                    invalidate_one(generation, *index);
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
}

pub(crate) async fn hydrate_all_solos(
    session: std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
) {
    let all: Vec<u8> = (1..=80).collect();
    refresh_ids(&session, generation, &all).await;
}

/// Session-owned solo maintenance: initial full hydration of all 80
/// solosw leaves, then desk-push refresh plus TTL renewal. Aborted with
/// the link; fanout children drop with the task.
pub(crate) async fn solo_maintenance_task(
    session: std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    mut dirty: tokio::sync::mpsc::UnboundedReceiver<SoloRefresh>,
) {
    hydrate_all_solos(session.clone(), generation).await;
    let mut ticker = tokio::time::interval(SOLO_RENEW_EVERY);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    loop {
        tokio::select! {
            msg = dirty.recv() => {
                let Some(first) = msg else { break };
                let mut ids = Vec::new();
                let mut full = matches!(first, SoloRefresh::All);
                if let SoloRefresh::One(index) = first {
                    ids.push(index);
                }
                while let Ok(msg) = dirty.try_recv() {
                    match msg {
                        SoloRefresh::All => {
                            full = true;
                            break;
                        }
                        SoloRefresh::One(index) => ids.push(index),
                    }
                }
                if full {
                    invalidate_all_solos(generation);
                    hydrate_all_solos(session.clone(), generation).await;
                } else {
                    ids.sort_unstable();
                    ids.dedup();
                    if ids.len() > 40 {
                        hydrate_all_solos(session.clone(), generation).await;
                    } else {
                        refresh_ids(&session, generation, &ids).await;
                    }
                }
            }
            _ = ticker.tick() => {
                let stale = stale_solo_ids(generation, SOLO_RENEW_AFTER);
                if !stale.is_empty() {
                    refresh_ids(&session, generation, &stale).await;
                }
            }
        }
    }
}

impl X32Driver {
    async fn solo_addr(&self, strip: StripId) -> anyhow::Result<String> {
        let _ = fam_of(strip)?;
        Ok(format!("/-stat/solosw/{:02}", solosw_index(strip)?))
    }

    /// Authoritative solo read: fresh independent query, strict 0/1.
    /// Unreadable or out-of-domain reads fail, never default to off.
    pub async fn get_solo_state(&self, strip: StripId) -> anyhow::Result<bool> {
        let path = self.solo_addr(strip).await?;
        let session = self.session()?;
        let msg = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        crate::mapping::strict_bool(&msg).map_err(|e| anyhow::anyhow!("x32: {e}"))
    }

    /// Verified solo write: pinned session under the write lane, exact
    /// 0/1 readback before success. No successful send is treated as
    /// confirmation.
    pub async fn set_solo_state(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let path = self.solo_addr(strip).await?;
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session
            .send(&path, vec![rosc::OscType::Int(i32::from(soloed))])
            .await?;
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let actual =
            crate::mapping::strict_bool(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        ensure!(
            actual == soloed,
            "x32: solo readback did not match the requested write"
        );
        Ok(())
    }

    /// Clear all solos via the desk action, then verify every solosw
    /// reads back off. Any unreadable or still-on leaf fails the result
    /// as uncertain rather than reporting fake success.
    pub async fn clear_solos(&self) -> anyhow::Result<()> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session
            .send(crate::addrs::CLEAR_SOLO, vec![rosc::OscType::Int(1)])
            .await?;
        self.ensure_current(&session)?;
        for index in 1..=80u8 {
            let path = format!("/-stat/solosw/{index:02}");
            let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
            self.ensure_current(&session)?;
            let actual =
                crate::mapping::strict_bool(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
            ensure!(
                !actual,
                "x32: clear solo uncertain: solosw/{index:02} still on"
            );
        }
        Ok(())
    }

    /// Complete solo bank: every strip fails the snapshot on missing or
    /// out-of-domain read, never a default unsoloed guess.
    pub async fn solo_bank_states(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        ensure!(
            channels <= crate::addrs::X32_STRIPS,
            "x32: solo bank out of range"
        );
        let mut out = Vec::with_capacity(channels);
        for ch in 0..channels as u16 {
            out.push(self.get_solo_state(StripId(ch)).await?);
        }
        Ok(out)
    }

    /// Sync solo rows: only fresh independently-verified observations on
    /// the live session, never guesses or own-write echoes. Cold/stale
    /// rows keep their descriptors with no current value. This preserves
    /// complete bank coverage, so advertised clear rejects an unreadable
    /// member instead of silently clearing only the observed subset.
    pub fn solo_control_rows(&self) -> Vec<DestinationControl> {
        let Ok(session) = self.session() else {
            return Vec::new();
        };
        let Ok(generation) = crate::routing::generation_for_session(&session) else {
            return Vec::new();
        };
        let Ok(cache) = SOLO_CACHE.lock() else {
            return Vec::new();
        };
        let mut rows = Vec::with_capacity(80);
        for index in 1..=80u8 {
            let observed = cache
                .get(&(generation, index))
                .filter(|hit| hit.at.elapsed() < SOLO_CACHE_TTL)
                .map(|hit| (hit.value, hit.revision));
            let Ok((target, label)) = solo_target(index) else {
                continue;
            };
            rows.push(solo_row(&target, &label, observed, generation));
        }
        rows
    }

    /// Guarded target-solo write behind the neutral `set_target_solo`
    /// contract: the caller's snapshot block must still be the cached
    /// row (same revision, generation and verified value); the write
    /// lane then re-reads the live leaf and requires exact agreement
    /// before dispatch, and exact 0/1 readback after. Returns the fresh
    /// control row; a missing row after the write fails as uncertain.
    pub(crate) async fn write_target_solo(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        ensure!(control == "solo", "x32: unknown solo control {control:?}");
        ensure!(
            matches!(value, 0.0 | 1.0) && matches!(expected, 0.0 | 1.0),
            "x32: solo outside 0/1 domain"
        );
        let index = solo_index_for_target(target)?;
        let before = self
            .solo_control_rows()
            .into_iter()
            .find(|row| row.target == target && row.id == "solo");
        let before = before.ok_or_else(|| anyhow::anyhow!("x32: solo unavailable at {target}"))?;
        ensure!(
            snapshot.contains(&before),
            "x32: solo changed; reopen the control"
        );
        ensure!(
            before
                .current
                .as_ref()
                .is_some_and(|v| v.verified && v.value == expected),
            "x32: solo changed; reopen the control"
        );
        ensure!(check(), "x32: solo engine session or policy changed");
        let session = self.session()?;
        let generation = crate::routing::generation_for_session(&session)?;
        ensure!(
            before.generation == Some(generation),
            "x32: solo session changed"
        );
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        ensure!(
            check() && crate::routing::generation_for_session(&session)? == generation,
            "x32: solo engine session or policy changed"
        );
        // Re-read the live leaf under the lane: the cached row alone
        // cannot authorize a dispatch.
        let path = format!("/-stat/solosw/{index:02}");
        let live_msg = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let live =
            crate::mapping::strict_bool(&live_msg).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        ensure!(
            f32::from(live) == expected,
            "x32: solo changed; reopen the control"
        );
        // Snapshot must still match after the live re-read (peer edit
        // between the two checks fails closed).
        let current_rows = self.solo_control_rows();
        let current = current_rows
            .iter()
            .find(|row| row.target == target && row.id == "solo")
            .ok_or_else(|| anyhow::anyhow!("x32: solo changed; reopen the control"))?;
        ensure!(
            snapshot.contains(current),
            "x32: solo changed; reopen the control"
        );
        // Withhold the row across the dispatch so no reader serves the
        // pre-write value as post-write confirmation.
        invalidate_one(generation, index);
        if f32::from(live) != value {
            session
                .send(&path, vec![rosc::OscType::Int(value as i32)])
                .await?;
        }
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        ensure!(
            check() && crate::routing::generation_for_session(&session)? == generation,
            "x32: solo result uncertain: session changed"
        );
        let actual =
            crate::mapping::strict_bool(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        ensure!(
            f32::from(actual) == value,
            "x32: solo readback did not match the requested write"
        );
        let _ = remember_solo(index, generation, actual);
        let rows = self.solo_control_rows();
        let receipt = rows
            .into_iter()
            .find(|row| row.target == target && row.id == "solo")
            .ok_or_else(|| anyhow::anyhow!("x32: solo result uncertain: no fresh readback"))?;
        ensure!(
            receipt
                .current
                .as_ref()
                .is_some_and(|v| v.verified && v.value == value),
            "x32: solo result uncertain"
        );
        ensure!(check(), "x32: solo result uncertain: context changed");
        Ok(receipt)
    }
}
