//! X32 meter subscriptions and decoding.
//! Streams expire after 10 seconds and are resubscribed every 8 seconds.
//! Decode validates blob shape and values; stale or invalid data is unavailable.
//! Session generations prevent old meter data from leaking across connections.

use crate::{addrs, X32Driver};
use openfader_core::{
    output_meters::OutputCompressorGr, MeterFrame, OutputMeterFrame, OutputMeterSnapshot, StripId,
};
use openfader_transport::{session::keepalive_ticker, OscSession};
use rosc::{OscMessage, OscType};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Exact reply addresses owned by this decoder, including slashless desk
/// variants. Shared transport never interprets meter IDs or wire layouts.
pub(crate) fn received_meter_addresses() -> Vec<String> {
    (0..=16)
        .flat_map(|id| [format!("/meters/{id}"), format!("meters/{id}")])
        .collect()
}

/// Bare `/meters` request address (stream id travels as the string arg,
/// with any documented int params after it, e.g. `/meters/5` chn grp).
const METER_REQUEST: &str = "/meters";
/// Documented bare-id subscribes this module keeps alive (see module docs).
const SUBSCRIBED: &[&str] = &[
    "/meters/0",
    "/meters/1",
    "/meters/2",
    "/meters/4",
    "/meters/16",
];
/// `/meters/5` stream id plus its documented params (doc p. 18):
/// `chn_meter_id` 0 (ch 1..16, redundant with the subscribed groups) and
/// `grp_meter_id` 0 (DCA 1..8 — slots 16..23 of the 27-float blob).
const M5_ID: &str = "/meters/5";
const M5_CHN: i32 = 0;
const M5_GRP: i32 = 0;
/// Renewal period: desk-side expiry is 10 s, so 8 s leaves margin for
/// two lost datagrams without ever relying on `/renew` (desk no-op).
const METER_RENEW_INTERVAL: Duration = Duration::from_secs(8);
/// Staleness bound: ~10 missed 50 ms frames. Slow enough to ride out
/// jitter, short enough that a dead stream reads unavailable.
const METER_STALE_AFTER: Duration = Duration::from_millis(500);
/// dB per automix short: `log2(gain)*256` inverts to
/// `dB = short * 20*log10(2)/256`.
const AUTOMIX_DB_PER_SHORT: f32 = 6.020_6 / 256.0;

/// Every documented subscribe as one arg vec: the bare ids plus
/// `/meters/5` with its two documented params (chn 0, grp 0 → DCA 1..8).
fn subscribe_requests() -> Vec<Vec<OscType>> {
    let mut out: Vec<Vec<OscType>> = SUBSCRIBED
        .iter()
        .map(|id| vec![OscType::String(id.to_string())])
        .collect();
    out.push(vec![
        OscType::String(M5_ID.to_string()),
        OscType::Int(M5_CHN),
        OscType::Int(M5_GRP),
    ]);
    out
}

/// One validated level blob with its socket receive time.
struct Sample {
    at: Instant,
    values: Vec<f32>,
}

/// Generation-gated meter caches. Written only by the connection drain
/// task, read by the poll/get paths; `generation` ties every sample to
/// one connection (a connect or disconnect clears, never replays).
#[derive(Default)]
struct State {
    generation: u64,
    /// `/meters/0`: 70 levels (ch/aux/fxrtn/bus/mtx).
    page0: Option<Sample>,
    /// `/meters/1`: 96 floats (ch levels + linear gate/comp gains).
    chan: Option<Sample>,
    /// `/meters/2`: 49 floats (bus/mtx/main levels + linear output dynamics gains).
    mixbus: Option<Sample>,
    /// `/meters/4`: 82 floats (ch/aux/physical outputs).
    inout: Option<Sample>,
    /// `/meters/5`: 27 floats (DCA 1..8 in slots 16..23 under grp 0).
    page5: Option<Sample>,
    /// `/meters/16`: automix dB for ch 1..8.
    automix: Option<(Instant, [f32; 8])>,
}

pub(crate) struct Telemetry {
    generation: AtomicU64,
    state: Mutex<State>,
    hub: broadcast::Sender<MeterFrame>,
    /// Generation the subscribe tasks were spawned for (`u64::MAX` =
    /// none); makes repeat `subscribe_meters` calls share one task pair.
    tasks_for: AtomicU64,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            state: Mutex::new(State::default()),
            hub: broadcast::channel(64).0,
            tasks_for: AtomicU64::new(u64::MAX),
        }
    }
}

impl Telemetry {
    fn current(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Bump the epoch and drop every sample. Called on connect (after
    /// identity is verified) and on disconnect, so no sample ever
    /// survives a session change.
    fn reset(&self) -> u64 {
        let gen = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.state.lock().unwrap() = State {
            generation: gen,
            ..Default::default()
        };
        gen
    }

    /// Drop stream samples for `gen` after a receive gap: unknown desk
    /// movement happened in the gap, so frozen levels must read
    /// unavailable rather than stale-frozen.
    fn clear_samples(&self, gen: u64) {
        let mut state = self.state.lock().unwrap();
        if state.generation == gen {
            state.page0 = None;
            state.chan = None;
            state.mixbus = None;
            state.inout = None;
            state.page5 = None;
            state.automix = None;
        }
    }
}

/// Stable provider output identities behind `poll_output_meters`.
/// Mirrors `telemetry_output_snapshot` exactly: bus/mtx/main families
/// (documented strip order, direct mapping) then the physical outputs
/// from `/meters/4` (structural order, correlation pending → inferred).
pub(crate) fn output_targets() -> Vec<String> {
    let mut out = Vec::with_capacity(60);
    for n in 1..=16 {
        out.push(format!("bus/{n:02}"));
    }
    for n in 1..=6 {
        out.push(format!("mtx/{n:02}"));
    }
    out.push("main".to_string());
    out.push("mainm".to_string());
    for n in 1..=16 {
        out.push(format!("output/{n:02}"));
    }
    for n in 1..=16 {
        out.push(format!("p16/{n:02}"));
    }
    for n in 1..=2 {
        out.push(format!("aes/{n:02}"));
    }
    for n in 1..=2 {
        out.push(format!("monitor/{n:02}"));
    }
    out
}

/// Validate one level blob: exact content length (`4 + 4*expected`),
/// LE count-word agreement, then all-LE-f32 samples that are finite and
/// within [0, 8.0] (documented linear range plus headroom). Wire note:
/// the doc's BE length word is the OSC blob framing itself (consumed by
/// the OSC decoder — a `/meters/0` reply is 304 bytes on the wire per
/// the emulator baseline); the delivered blob content is the LE count
/// followed by the floats, verified here against `Xprepmeter` shapes
/// (commit unrecorded). Levels clip to 1.0 for strip frames. Anything else
/// → `None` (drop the sample, keep the old cache).
pub(crate) fn decode_level_blob(blob: &[u8], expected: usize) -> Option<Vec<f32>> {
    if blob.len() != 4 + expected.checked_mul(4)? {
        return None;
    }
    if u32::from_le_bytes(blob[0..4].try_into().ok()?) as usize != expected {
        return None;
    }
    let mut out = Vec::with_capacity(expected);
    for c in blob[4..].as_chunks::<4>().0 {
        let v = f32::from_le_bytes(*c);
        if !v.is_finite() || v < 0.0 || v > 8.0 {
            return None;
        }
        out.push(v.min(1.0));
    }
    Some(out)
}

/// Validate a `/meters/16` blob (LE count 48 + 48 words) and extract
/// the 8 automix gains for ch 1..8 as dB. The 88 linear-gain shorts must
/// be non-negative (they duplicate m1/m2 GR coverage and are validation
/// only); the last 8 shorts decode via `log2(gain)*256`.
pub(crate) fn decode_m16_automix(blob: &[u8]) -> Option<[f32; 8]> {
    if blob.len() != 4 + 48 * 4 {
        return None;
    }
    if u32::from_le_bytes(blob[0..4].try_into().ok()?) as usize != 48 {
        return None;
    }
    for raw in blob[4..4 + 88 * 2].as_chunks::<2>().0 {
        if i16::from_le_bytes(*raw) < 0 {
            return None;
        }
    }
    let mut db = [0.0f32; 8];
    for (i, raw) in blob[4 + 88 * 2..].as_chunks::<2>().0.iter().enumerate() {
        db[i] = f32::from(i16::from_le_bytes(*raw)) * AUTOMIX_DB_PER_SHORT;
    }
    if db.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(db)
}

/// Meter gains are linear (protocol reference v4.09-02 p.16, unity in the
/// worked /meters/6 packet; p.19 explicitly confirms the same convention
/// for packed dynamics gains). Convert attenuation to positive dB.
/// Zero gain is complete attenuation: use the smallest normal f32 as a
/// finite logarithm floor. Gains above unity carry no reduction.
fn gr_db(raw: f32) -> f32 {
    (-20.0 * raw.max(f32::MIN_POSITIVE).log10()).max(0.0)
}

/// Project the 80-strip frame from validated fresh samples.
/// `None` when no stream is fresh (unavailable, never fabricated).
/// Ch inputs prefer `/meters/1` (fall back to `/meters/0`); aux and
/// fxrtn ride `/meters/0` as eight independent mono strips each (no
/// stereo folding — the topology is eight separate mono faders per
/// family); bus/mtx prefer `/meters/2` (fall back to `/meters/0`);
/// mains ride `/meters/2` (LR folds with max); DCA 1..8 ride
/// `/meters/5` slots 16..23 (grp 0) and read 0.0 in an otherwise fresh
/// frame without a fresh m5 (no DCA evidence, never measured).
fn fresh_sample(sample: &Option<Sample>, len: usize, now: Instant) -> Option<&[f32]> {
    let s = sample.as_ref()?;
    if s.values.len() != len || now.duration_since(s.at) > METER_STALE_AFTER {
        return None;
    }
    Some(&s.values)
}

/// Project the 80-strip frame from validated fresh samples.
/// `None` when no stream is fresh (unavailable, never fabricated).
/// Ch inputs prefer `/meters/1` (fall back to `/meters/0`); aux and
/// fxrtn ride `/meters/0` as eight independent mono strips each (no
/// stereo folding — the topology is eight separate mono faders per
/// family); bus/mtx prefer `/meters/2` (fall back to `/meters/0`);
/// mains ride `/meters/2` (LR folds with max); DCA 1..8 ride
/// `/meters/5` slots 16..23 (grp 0) and read 0.0 in an otherwise fresh
/// frame without a fresh m5 (no DCA evidence, never measured).
fn project_frame(state: &State, now: Instant) -> Option<MeterFrame> {
    let page0 = fresh_sample(&state.page0, 70, now);
    let chan = fresh_sample(&state.chan, 96, now);
    let mixbus = fresh_sample(&state.mixbus, 49, now);
    let dca = fresh_sample(&state.page5, 27, now);
    if page0.is_none() && chan.is_none() && mixbus.is_none() && dca.is_none() {
        return None;
    }
    let zero = 0.0f32;
    let at = |bank: Option<&[f32]>, i: usize| -> f32 { bank.map(|b| b[i]).unwrap_or(zero) };
    let mut levels = vec![0.0f32; addrs::X32_STRIPS];
    for (i, slot) in levels.iter_mut().take(32).enumerate() {
        *slot = chan.map(|b| b[i]).unwrap_or_else(|| at(page0, i));
    }
    for i in 0..8 {
        levels[32 + i] = at(page0, 32 + i);
    }
    // Eight independent mono fxrtn faders: one float per strip, no fold.
    for i in 0..8 {
        levels[40 + i] = at(page0, 40 + i);
    }
    for i in 0..16 {
        levels[48 + i] = mixbus.map(|b| b[i]).unwrap_or_else(|| at(page0, 48 + i));
    }
    for i in 0..6 {
        levels[64 + i] = mixbus
            .map(|b| b[16 + i])
            .unwrap_or_else(|| at(page0, 64 + i));
    }
    if let Some(b) = mixbus {
        levels[70] = b[22].max(b[23]);
        levels[71] = b[24];
    }
    // DCA 1..8 are the grp-0 group meters of `/meters/5` (slots 16..23);
    // without a fresh m5 there is no DCA evidence, so 0.0 (never measured).
    for i in 0..8 {
        levels[72 + i] = dca.map(|b| b[16 + i]).unwrap_or(zero);
    }
    Some(MeterFrame { levels })
}

impl X32Driver {
    pub(crate) fn telemetry_reset(&self) {
        self.telemetry.reset();
    }

    /// Freshest receipt for streams that contribute to each projection.
    /// Neither replayed samples nor unrelated output/automix pages advance it.
    pub(crate) fn meter_timestamps(&self) -> (Option<Instant>, Option<Instant>) {
        let gen = self.telemetry.current();
        let state = self.telemetry.state.lock().unwrap();
        if state.generation != gen {
            return (None, None);
        }
        let now = Instant::now();
        let latest = |samples: &[&Option<Sample>]| {
            samples
                .iter()
                .filter_map(|sample| sample.as_ref())
                .filter(|sample| now.duration_since(sample.at) <= METER_STALE_AFTER)
                .map(|sample| sample.at)
                .max()
        };
        (
            latest(&[&state.page0, &state.chan, &state.mixbus, &state.page5]),
            latest(&[&state.chan, &state.mixbus]),
        )
    }

    /// Subscribe to the meter broadcast, arming the documented `/meters` streams (bare ids plus `/meters/5`
    /// chn 0 / grp 0 for DCA) plus the 8 s renewal and the strict drain task.
    /// Idempotent per connection: repeat calls share the task pair.
    /// Fails when not connected (never a phantom subscription).
    pub(crate) async fn telemetry_subscribe(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<MeterFrame>> {
        let session = self.session()?;
        let gen = self.telemetry.current();
        self.ensure_meter_tasks(&session, gen);
        Ok(self.telemetry.hub.subscribe())
    }

    /// Spawn the renewal + drain task pair for `gen` unless already
    /// running; owned by the link so a disconnect aborts both. A
    /// disconnect between spawn and store aborts the orphans.
    fn ensure_meter_tasks(&self, session: &Arc<OscSession>, gen: u64) {
        if self.telemetry.tasks_for.load(Ordering::Acquire) == gen {
            return;
        }
        let sender = tokio::spawn(meter_renewal_task(
            Arc::clone(session),
            self.telemetry.clone(),
            gen,
        ));
        let drain = tokio::spawn(meter_drain_task(
            Arc::clone(session),
            self.telemetry.clone(),
            gen,
        ));
        let mut link = self.link.write().unwrap();
        if self.telemetry.current() == gen {
            if let Some(l) = link.as_mut() {
                l.meter_tasks.push(sender);
                l.meter_tasks.push(drain);
            }
            self.telemetry.tasks_for.store(gen, Ordering::Release);
        } else {
            sender.abort();
            drain.abort();
        }
    }

    /// Re-arm the documented subscribes on the live session and require
    /// new samples (clears caches; no cached replay becomes fresh).
    /// Used after host suspension; same-session renewal otherwise runs
    /// on its own 8 s loop. Reuses the existing task pair when alive.
    pub(crate) async fn telemetry_refresh(&self) -> anyhow::Result<()> {
        let session = self.session()?;
        let gen = self.telemetry.current();
        self.telemetry.clear_samples(gen);
        for args in subscribe_requests() {
            session.send(METER_REQUEST, args).await?;
        }
        self.ensure_meter_tasks(&session, gen);
        Ok(())
    }

    /// Non-blocking strip frame for the 80-strip topology: ch inputs
    /// prefer `/meters/1` (fall back to `/meters/0`); aux and fxrtn ride
    /// `/meters/0` as eight independent mono strips each; bus/mtx prefer
    /// `/meters/2` (fall back to `/meters/0`); mains ride `/meters/2`
    /// (LR folds with max); DCA 1..8 ride `/meters/5` (grp 0).
    /// No fresh stream at all → empty (unavailable, same
    /// convention as X-Air), never fabricated silence.
    pub(crate) fn telemetry_poll(&self) -> MeterFrame {
        let gen = self.telemetry.current();
        let state = self.telemetry.state.lock().unwrap();
        if state.generation != gen {
            return MeterFrame { levels: Vec::new() };
        }
        project_frame(&state, Instant::now()).unwrap_or(MeterFrame { levels: Vec::new() })
    }

    /// Gate GR (dB >= 0) for channels 1..32 from `/meters/1` slots
    /// 32..64. Fresh only; anything else bails unavailable, never zero.
    pub(crate) fn cached_gate_gr(&self, strip: StripId) -> anyhow::Result<f32> {
        anyhow::ensure!(strip.0 < 32, "x32: gate GR is channels 1..32 only");
        let gen = self.telemetry.current();
        let state = self.telemetry.state.lock().unwrap();
        anyhow::ensure!(
            state.generation == gen,
            "x32: gate GR unavailable (not connected)"
        );
        let s = state.chan.as_ref().filter(|s| {
            s.values.len() == 96 && Instant::now().duration_since(s.at) <= METER_STALE_AFTER
        });
        let Some(s) = s else {
            anyhow::bail!("x32: gate GR unavailable or stale (desk not streaming /meters/1)");
        };
        Ok(gr_db(s.values[32 + strip.0 as usize]))
    }

    /// Comp GR (dB >= 0): ch 1..32 <- `/meters/1` slots 64..96;
    /// bus/mtx/main <- `/meters/2` output GR slots. Aux/fxrtn/DCA have
    /// no documented GR slot and bail. Fresh only, never zero-filled.
    pub(crate) fn cached_comp_gr(&self, strip: StripId) -> anyhow::Result<f32> {
        let gen = self.telemetry.current();
        let state = self.telemetry.state.lock().unwrap();
        anyhow::ensure!(
            state.generation == gen,
            "x32: comp GR unavailable (not connected)"
        );
        let now = Instant::now();
        let i = strip.0 as usize;
        if i < 32 {
            let s = state
                .chan
                .as_ref()
                .filter(|s| s.values.len() == 96 && now.duration_since(s.at) <= METER_STALE_AFTER);
            let Some(s) = s else {
                anyhow::bail!("x32: comp GR unavailable or stale (desk not streaming /meters/1)");
            };
            return Ok(gr_db(s.values[64 + i]));
        }
        let slot = match i {
            48..=63 => 25 + (i - 48),
            64..=69 => 41 + (i - 64),
            70 => 47,
            71 => 48,
            _ => anyhow::bail!("x32: no documented comp GR slot for strip {}", strip.0),
        };
        let s = state
            .mixbus
            .as_ref()
            .filter(|s| s.values.len() == 49 && now.duration_since(s.at) <= METER_STALE_AFTER);
        let Some(s) = s else {
            anyhow::bail!("x32: comp GR unavailable or stale (desk not streaming /meters/2)");
        };
        Ok(gr_db(s.values[slot]))
    }

    /// Automix gain (dB) for channels 1..8 from `/meters/16`. Fresh
    /// only; anything else bails unavailable, never synthesized.
    pub(crate) fn cached_automix_db(&self, strip: StripId) -> anyhow::Result<f32> {
        anyhow::ensure!(strip.0 < 8, "x32: automix gain is channels 1..8 only");
        let gen = self.telemetry.current();
        let state = self.telemetry.state.lock().unwrap();
        anyhow::ensure!(
            state.generation == gen,
            "x32: automix gain unavailable (not connected)"
        );
        let Some((_, db)) = state
            .automix
            .as_ref()
            .filter(|(at, _)| Instant::now().duration_since(*at) <= METER_STALE_AFTER)
        else {
            anyhow::bail!("x32: automix gain unavailable or stale (desk not streaming /meters/16)");
        };
        Ok(db[strip.0 as usize])
    }

    /// Physical + master output snapshot. `Some` always (supported);
    /// empty frames mean supported-but-unavailable, never fake levels.
    /// Bus/mtx/main frames carry `/meters/2` levels plus output GR;
    /// physical-out/P16/AES/monitor frames carry `/meters/4` levels
    /// (structural order, correlation pending → inferred, no GR).
    pub(crate) fn telemetry_output_snapshot(&self) -> OutputMeterSnapshot {
        let gen = self.telemetry.current();
        let mut snap = OutputMeterSnapshot {
            generation: gen,
            age_ms: 0,
            frames: Vec::new(),
        };
        let state = self.telemetry.state.lock().unwrap();
        if state.generation != gen {
            return snap;
        }
        let now = Instant::now();
        if let Some(b) = state
            .mixbus
            .as_ref()
            .filter(|s| s.values.len() == 49 && now.duration_since(s.at) <= METER_STALE_AFTER)
        {
            snap.age_ms = snap.age_ms.max(now.duration_since(b.at).as_millis() as u64);
            let v = &b.values;
            for i in 0..16 {
                snap.frames.push(OutputMeterFrame {
                    target: format!("bus/{:02}", i + 1),
                    levels: vec![v[i]],
                    inferred: false,
                    compressor_gr: Some(OutputCompressorGr {
                        reduction_db: Some(vec![gr_db(v[25 + i])]),
                        inferred: false,
                    }),
                });
            }
            for i in 0..6 {
                snap.frames.push(OutputMeterFrame {
                    target: format!("mtx/{:02}", i + 1),
                    levels: vec![v[16 + i]],
                    inferred: false,
                    compressor_gr: Some(OutputCompressorGr {
                        reduction_db: Some(vec![gr_db(v[41 + i])]),
                        inferred: false,
                    }),
                });
            }
            snap.frames.push(OutputMeterFrame {
                target: "main".to_string(),
                levels: vec![v[22], v[23]],
                inferred: false,
                compressor_gr: Some(OutputCompressorGr {
                    reduction_db: Some(vec![gr_db(v[47])]),
                    inferred: false,
                }),
            });
            snap.frames.push(OutputMeterFrame {
                target: "mainm".to_string(),
                levels: vec![v[24]],
                inferred: false,
                compressor_gr: Some(OutputCompressorGr {
                    reduction_db: Some(vec![gr_db(v[48])]),
                    inferred: false,
                }),
            });
        }
        if let Some(s) = state
            .inout
            .as_ref()
            .filter(|s| s.values.len() == 82 && now.duration_since(s.at) <= METER_STALE_AFTER)
        {
            snap.age_ms = snap.age_ms.max(now.duration_since(s.at).as_millis() as u64);
            let v = &s.values;
            let push = |snap: &mut OutputMeterSnapshot, target: String, level: f32| {
                snap.frames.push(OutputMeterFrame {
                    target,
                    levels: vec![level],
                    inferred: true,
                    compressor_gr: None,
                });
            };
            for i in 0..16 {
                push(&mut snap, format!("output/{:02}", i + 1), v[40 + i]);
            }
            for i in 0..16 {
                push(&mut snap, format!("p16/{:02}", i + 1), v[56 + i]);
            }
            for i in 0..2 {
                push(&mut snap, format!("aes/{:02}", i + 1), v[78 + i]);
            }
            for i in 0..2 {
                push(&mut snap, format!("monitor/{:02}", i + 1), v[80 + i]);
            }
        }
        snap
    }
}

/// Store one validated meter message into the generation-gated caches
/// and project the strip frame. Returns the frame when at least one
/// stream is fresh (empty frames are never broadcast).
fn ingest_meter_message(
    tele: &Telemetry,
    at: Instant,
    msg: &OscMessage,
    gen: u64,
) -> Option<MeterFrame> {
    let blob: &[u8] = msg.args.iter().find_map(|a| match a {
        OscType::Blob(b) => Some(b.as_slice()),
        _ => None,
    })?;
    let bare = msg.addr.strip_prefix('/').unwrap_or(&msg.addr);
    let id: u8 = bare
        .strip_prefix("meters/")
        .and_then(|n| n.parse().ok())
        .filter(|n| *n <= 16)?;
    let mut state = tele.state.lock().unwrap();
    if state.generation != gen {
        return None;
    }
    match id {
        0 => state.page0 = decode_level_blob(blob, 70).map(|v| Sample { at, values: v }),
        1 => state.chan = decode_level_blob(blob, 96).map(|v| Sample { at, values: v }),
        2 => state.mixbus = decode_level_blob(blob, 49).map(|v| Sample { at, values: v }),
        4 => state.inout = decode_level_blob(blob, 82).map(|v| Sample { at, values: v }),
        5 => state.page5 = decode_level_blob(blob, 27).map(|v| Sample { at, values: v }),
        16 => {
            state.automix = decode_m16_automix(blob).map(|db| (at, db));
        }
        // Subscribed nothing else; an unexpected id is ignored, never
        // guessed at (m6 selected-channel taps and m15 RTA stay
        // unavailable: no documented strip/selector map).
        _ => return None,
    }
    drop(state);
    // Project under a fresh lock with the shared projection.
    let gen_now = tele.current();
    if gen_now != gen {
        return None;
    }
    let state = tele.state.lock().unwrap();
    if state.generation != gen_now {
        return None;
    }
    project_frame(&state, Instant::now())
}

/// Drain task: strict-decode every forwarded `/meters/N` blob into the
/// generation-gated caches and broadcast fresh strip frames. Ends with
/// the connection (generation change) or the session.
async fn meter_drain_task(session: Arc<OscSession>, tele: Arc<Telemetry>, gen: u64) {
    let mut rx = session.subscribe_received_updates();
    loop {
        match rx.recv().await {
            Ok((at, msg)) => {
                if tele.generation.load(Ordering::Acquire) != gen {
                    break;
                }
                if !msg.addr.starts_with("/meters") && !msg.addr.starts_with("meters") {
                    continue;
                }
                if let Some(frame) = ingest_meter_message(&tele, at, &msg, gen) {
                    let _ = tele.hub.send(frame);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                tele.clear_samples(gen);
            }
            Err(_) => break,
        }
    }
}

/// Renewal task: initial `/meters` subscribes, then re-send every 8 s
/// (desk expiry is 10 s). `/xremote` keepalive does NOT renew meters
/// and `/renew` is a desk-side no-op, so only documented subscribes
/// are re-sent. Ends with the connection.
async fn meter_renewal_task(session: Arc<OscSession>, tele: Arc<Telemetry>, gen: u64) {
    for args in subscribe_requests() {
        let _ = session.send(METER_REQUEST, args).await;
    }
    let mut ticker = keepalive_ticker(METER_RENEW_INTERVAL);
    // The ticker fires immediately; consume it so the first renewal
    // lands one full interval after the initial subscribe.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if tele.generation.load(Ordering::Acquire) != gen {
            break;
        }
        for args in subscribe_requests() {
            if let Err(error) = session.send(METER_REQUEST, args).await {
                eprintln!("x32: meter resubscribe failed: {error}");
            }
        }
    }
}
