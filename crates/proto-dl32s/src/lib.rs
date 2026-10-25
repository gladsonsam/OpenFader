//! Mackie DL32S driver: thin mapping over [`MackieSession`].
//!
//! This crate owns ONLY the DL32S address table ([`addrs`]) plus argument
//! mapping. All sockets, framing, keepalive, caching and the init handshake
//! live in `openfader-transport::mackie`. New providers add a crate shaped
//! exactly like this one: table + mapping, zero core changes.

pub mod addrs;

use addrs::{dl32s_inputs, INPUT_COUNT};
use openfader_core::{
    validate_fader, AudioSpectrum, ChannelStrip, MeterFrame, MixerDriver, MixerInfo, MixerSnapshot,
    ParamUpdate, ParamValue, StripId,
};
use openfader_transport::mackie::{
    MackieSession, PushEvent, MUTE_GROUP_BASE_ADDRESS, MUTE_GROUP_COUNT, REQUEST_TIMEOUT,
    VALUE_TYPE_METERS, VALUE_TYPE_MUTE_GROUP, VALUE_TYPE_PARAM,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, MutexGuard};

// Type-2 input Solo: CH7=29 and CH8=30 independently verified against
// Master Fader, 2026-10-25. The contiguous input bank starts at CH1=23.
const INPUT_SOLO_FIRST_ADDRESS: u32 = 23;

/// Wave-1+2 canonical surface: every id below names a DL32S leaf whose
/// position was confirmed on hardware AND whose value domain was confirmed
/// live — by the read-only push-cache survey (varied values prove the desk
/// stores the domain) or by write+relay with net-zero restore 2026-10-20.
/// See DL32S protocol notes and the `verified` flags in [`addrs`].
///
/// Deliberately absent (reachable only by raw table key for experts):
/// - `icon` — position verified, but the live domain is ~1000+ (app icon
///   table ids), not the canonical 0..31. Unmapped until the icon table is
///   reversed (Wave-2 REA residue).
/// - `gate_mode`: native CH7 Modern=0/Vintage=1 verified with independent
///   readback and exact restore on 2026-10-25. Vintage uses discrete
///   attack/release controls; modern timing units must not be assumed.
/// - `comp_mode`/`comp_knee`/`eq_mode` polarity labels assumed
///   modern=0/vintage=1, soft=0/hard=1 per Mackie convention — both states
///   relay live for each; labels may swap (out-of-domain writes are
///   rejected, not faked).
/// - `comp_ratio` — direct floats confirmed (1.68..20.2 observed); canonical
///   allows to 100, desk clamps above its max.
/// - `eqN_type` — all four bands carry a leading type word (offsets
///   34/38/42/46). Raw 2 = Bell, 3 = LowShelf, 4 = HighShelf verified live
///   on B1/B4 (CH7 write+relay, net-zero restored 2026-10-24); native 0/1/5
///   meanings unverified, values relay exact, no Rust canonical remap
///   (frontend translates presentation LowShelf canonical 1 <-> native 3).
/// - `viewN`/`vcaN`/`mutegroupN`/`subN` membership — write-proven live, but
///   served through the engine group `Cmd`s, not strip caps.
/// - `source_a`/`source_b` — opaque native routing IDs (1000+/2000+).
///   Exposed as provider rows without inventing routing labels.
/// - `source_select` — native A/B selector, 0/1, exposed as a provider row.
/// - `sendN_tap`/`sendN_grpon`/`sendN_on` — no confirmed leaf (the desk is
///   silent; the UI collapses them, never faked).
/// - `hpf_slope` — reads constant 2, meaning inferred.
///
/// Per-provider caps ranges ride `param_range_overrides` (documented
/// modern-DL32S bounds, see `DL32S_RANGE_OVERRIDES`); the canonical
/// catalogue is untouched.
pub const DL32S_CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "pan",
    "gain",
    "phantom",
    "trim",
    "source_a",
    "source_b",
    "source_select",
    "polarity",
    "hpf_on",
    "hpf_freq",
    "lr_assign",
    "link",
    "color",
    "gate_on",
    "gate_thr",
    "gate_att",
    "gate_mode",
    "gate_rel",
    "gate_range",
    "gate_hold",
    "comp_on",
    "comp_mode",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_att",
    "comp_rel",
    "comp_makeup",
    "eq_on",
    "eq_mode",
    "eq1_gain",
    "eq1_freq",
    "eq1_q",
    "eq1_type",
    "eq2_gain",
    "eq2_freq",
    "eq2_q",
    "eq2_type",
    "eq3_gain",
    "eq3_freq",
    "eq3_q",
    "eq3_type",
    "eq4_gain",
    "eq4_freq",
    "eq4_q",
    "eq4_type",
    "aux1_level",
    "aux1_mute",
    "aux2_level",
    "aux2_mute",
    "aux3_level",
    "aux3_mute",
    "aux4_level",
    "aux4_mute",
    "aux5_level",
    "aux5_mute",
    "aux6_level",
    "aux6_mute",
    "aux7_level",
    "aux7_mute",
    "aux8_level",
    "aux8_mute",
    "fx1_level",
    "fx1_mute",
    "fx2_level",
    "fx2_mute",
    "fx3_level",
    "fx3_mute",
    "fx4_level",
    "fx4_mute",
    "send1_level",
    "send1_pan",
    "send2_level",
    "send2_pan",
    "send3_level",
    "send3_pan",
    "send4_level",
    "send4_pan",
    "send5_level",
    "send5_pan",
    "send6_level",
    "send6_pan",
    "send7_level",
    "send7_pan",
    "send8_level",
    "send8_pan",
    "send9_level",
    "send10_level",
    "send11_level",
    "send12_level",
];

/// Canonical id -> native table key for renamed leaves (`None` = same name).
/// Pure renames only — unit conversions (pan) and domain guards (modes)
/// stay as explicit arms in `set_param`/`get_param`/decode.
fn alias_target(param: &str) -> Option<&'static str> {
    Some(match param {
        "lr_assign" => "main_assign",
        "link" => "stereo_link",
        "color" => "colour",
        "gate_thr" => "gate_threshold",
        "gate_att" => "gate_attack",
        "gate_rel" => "gate_release",
        "comp_thr" => "comp_threshold",
        "comp_makeup" => "comp_gain",
        "comp_att" => "comp_attack",
        "comp_rel" => "comp_release",
        _ => return None,
    })
}

fn canonical_for_table_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "main_assign" => "lr_assign",
        "stereo_link" => "link",
        "colour" => "color",
        "gate_threshold" => "gate_thr",
        "gate_attack" => "gate_att",
        "gate_release" => "gate_rel",
        "comp_threshold" => "comp_thr",
        "comp_gain" => "comp_makeup",
        "comp_attack" => "comp_att",
        "comp_release" => "comp_rel",
        _ => return None,
    })
}

/// Generic send slot -> native table key. `send1-8` ride the aux sends
/// (level + pan), `send9-12` ride the FX sends (level only — FX sends have
/// no pan third word). Only confirmed leaves map; tap/grpon/on fall through
/// to the honest unknown-param error.
fn send_table_key(param: &str) -> Option<String> {
    let (slot, leaf) = param.strip_prefix("send")?.split_once('_')?;
    let n: u32 = slot.parse().ok()?;
    let native = match (n, leaf) {
        (1..=8, "level") => format!("aux{n}_level"),
        (1..=8, "pan") => format!("aux{n}_pan"),
        (9..=12, "level") => format!("fx{}_level", n - 8),
        _ => return None,
    };
    Some(native)
}

/// Decode one pushed raw into canonical `(strip, param, value)` triples,
/// mirroring `get_param` exactly (fader dB->level, pan x100, renames, send
/// mirrors). Only [`DL32S_CAPABILITIES`] ids are emitted — the push plane
/// equals the caps plane, so the engine never learns an id it didn't
/// advertise. Master/type-2/meter leaves have no strip plane and are
/// skipped (documented limitation: masters refresh on open + own writes).
fn decode_push_value(addr: u32, raw: u32) -> Vec<(StripId, String, ParamValue)> {
    use addrs::{INPUT_BASE, INPUT_STRIDE};
    if !(INPUT_BASE..INPUT_BASE + INPUT_COUNT * INPUT_STRIDE).contains(&addr) {
        return Vec::new();
    }
    let ch0 = ((addr - INPUT_BASE) / INPUT_STRIDE) as u16;
    let off = (addr - INPUT_BASE) % INPUT_STRIDE;
    let table = dl32s_inputs();
    let Some(field) = table.fields.iter().find(|f| f.offset == off) else {
        return Vec::new();
    };
    let strip = StripId(ch0);
    let f = f32::from_bits(raw);
    let mut out: Vec<(StripId, String, ParamValue)> = Vec::new();
    let mut push = |param: String, value: ParamValue| {
        out.push((strip, param, value));
    };
    match field.key {
        "fader" => push("fader".to_string(), ParamValue::Float(addrs::DL32S_FADER_SCALE.inverse(f))),
        "mute" => push("mute".to_string(), ParamValue::Bool(raw != 0)),
        "pan" => push("pan".to_string(), ParamValue::Float(f * 100.0)),
        key => {
            // Pure renames ride the canonical id.
            if let Some(canonical) = canonical_for_table_key(key) {
                let v = match field.encoding {
                    addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                    _ => ParamValue::Float(f),
                };
                push(canonical.to_string(), v);
                return out;
            }
            // Legacy + FX send leaves ride twice (native id and generic
            // slot), exactly like the poller reads them. Send pans convert
            // wire -1.0..+1.0 to canonical -100..+100.
            if let Some((slot, leaf)) = send_slot_for_table_key(key) {
                let val = if leaf == "pan" { f * 100.0 } else { f };
                push(format!("send{slot}_{leaf}"), ParamValue::Float(val));
            }
            if !DL32S_CAPABILITIES.contains(&key) {
                return out;
            }
            let value = match field.encoding {
                addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                addrs::Encoding::Db | addrs::Encoding::Float => ParamValue::Float(f),
                addrs::Encoding::Enum | addrs::Encoding::Int => {
                    ParamValue::Float(raw as f32)
                }
            };
            push(key.to_string(), value);
        }
    }
    out
}

fn send_slot_for_table_key(key: &str) -> Option<(u32, &'static str)> {
    if let Some(rest) = key.strip_prefix("aux") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=8).contains(&n) {
            return None;
        }
        return match leaf {
            "level" => Some((n, "level")),
            "pan" => Some((n, "pan")),
            _ => None,
        };
    }
    if let Some(rest) = key.strip_prefix("fx") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=4).contains(&n) || leaf != "level" {
            return None;
        }
        return Some((n + 8, "level"));
    }
    None
}

/// Decode one transport push into engine `ParamUpdate`s (engineering units,
/// same as `get_param`). Type-5 strip leaves fan out per [`decode_push_value`];
/// name slots 1-32 become `name` updates; snapshot relays are recorded in
/// the session cache only (the poller converges the wholesale change).
fn decode_push(push: PushEvent) -> Vec<ParamUpdate> {
    let received_at = std::time::Instant::now();
    match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } => {
            // Mute-group masters ride their own address space (type 2):
            // master for 1-based group g lives at
            // MUTE_GROUP_BASE_ADDRESS + g. The engine already routes
            // `_mute_group` ingest updates to MuteGroup events (same
            // strip convention as `get_mute_group`), so another client's
            // master change converges live instead of waiting for the
            // next masters refresh.
            if value_type == VALUE_TYPE_MUTE_GROUP {
                let mut out = Vec::new();
                for (i, raw) in raws.iter().enumerate() {
                    let address = start + i as u32;
                    let input0 = address.wrapping_sub(INPUT_SOLO_FIRST_ADDRESS);
                    if input0 < INPUT_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(input0 as u16),
                            param: "solo".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                    let group0 = address.wrapping_sub(MUTE_GROUP_BASE_ADDRESS + 1);
                    if group0 < MUTE_GROUP_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(group0 as u16),
                            param: "_mute_group".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                }
                return out;
            }
            if value_type != VALUE_TYPE_PARAM {
                return Vec::new();
            }
            let mut out = Vec::new();
            for (i, raw) in raws.iter().enumerate() {
                for (strip, param, value) in decode_push_value(start + i as u32, *raw) {
                    out.push(ParamUpdate { received_at, strip, param, value });
                }
            }
            out
        }
        PushEvent::Names { pairs } => pairs
            .into_iter()
            .filter(|(slot, _)| (1..=INPUT_COUNT).contains(slot))
            .map(|(slot, name)| ParamUpdate {
                received_at,
                strip: StripId((slot - 1) as u16),
                param: "name".to_string(),
                value: ParamValue::Text(name),
            })
            .collect(),
        PushEvent::SnapshotRecall(_) | PushEvent::SnapshotRename { .. } => Vec::new(),
    }
}

/// Observed input-meter push shape (type-1 `Values`): `start == 1` with at
/// least 32 words, where the first 32 words are the ordered input meters as
/// big-endian IEEE-754 dB. Rig-verified 2026-10-24 (CH7 A->B flip moved only
/// word 7 by >10 dB; word 7 restored with all 106 CH7 words): structurally
/// upstream, CH7 live-pinned. Words past the first 32 ride the rest of the
/// meter layout (returns, aux masters, mains) — unmodelled, never cached.
const METER_START: u32 = 1;

/// Staleness bound for the input-meter cache: the desk pushes at ~11 Hz, so
/// 1 s is ~11 missed frames — long enough to ride out jitter, short enough
/// that a dead stream reads unavailable (empty) instead of frozen session data.
const METER_STALE_AFTER: Duration = Duration::from_secs(1);

/// Cached pre-fader input levels (linear amplitude, one entry per input).
/// Written only by the connection ingest task, read by `poll_meters`; the
/// generation ties each sample to its connection (a new connect or a
/// disconnect clears it, so stale data never survives a session change).
#[derive(Debug, Default)]
struct MeterState {
    generation: u64,
    updated_at: Option<std::time::Instant>,
    levels: Vec<f32>,
}

/// Decode one meter push (type-1 `Values`) into the 32 linear input levels,
/// or `None` when the shape is not the observed one (wrong start, short
/// block) or any of the first 32 words is non-finite dB. Conversion is
/// `10^(db/20)` amplitude: valid 0 dB reads back 1, and an exact `0.0` word
/// is 0 dB (unity) — never silence-mapped. Only the first-32 scope keeps
/// phantom aux slots out; nothing here multiplies by fader state, so no
/// post-fader level is ever inferred.
fn decode_input_meters(push: &PushEvent) -> Option<Vec<f32>> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_METERS => (*start, raws),
        _ => return None,
    };
    if start != METER_START || raws.len() < INPUT_COUNT as usize {
        return None;
    }
    let mut out = Vec::with_capacity(INPUT_COUNT as usize);
    for raw in raws.iter().take(INPUT_COUNT as usize) {
        let db = f32::from_bits(*raw);
        if !db.is_finite() {
            return None;
        }
        out.push(10f32.powf(db / 20.0).clamp(0.0, 1.0));
    }
    Some(out)
}

/// Shared RTA selector (type-2) address: the desk's RTA source selector.
/// Rig r12 accepted: the initial bulk carries `5000` (Return 1–2) and
/// `4006` was verified live as CH7. No other selector ids are known —
/// unknown values report unavailable, never guessed. No selector writes in
/// this stage: the source is passive (the MF app selects; we only follow).
const RTA_SELECTOR_ADDR: u32 = 22;
/// Confirmed shared source: Return 1–2 (global, `strip: None`).
const RTA_SOURCE_RETURN_12: u32 = 5000;
/// Verified shared source: CH7 (attributed to [`StripId`]`(6)`).
const RTA_SOURCE_CH7: u32 = 4006;

/// RTA meter-frame geometry: the DL32S-only 91-entry layout (see
/// [`openfader_transport::mackie::dl32s_meter_addresses`]) pushes
/// `start == 1` blocks of exactly 91 BE-f32 dB words, with the 31 RTA bins
/// at indices 60..90. The first 32 input words are unchanged, so
/// [`decode_input_meters`] keeps accepting both the old 60-word and the new
/// 91-word frames. RTA decodes ONLY the exact 91-word shape — a 60-word
/// frame never yields bins.
const RTA_FRAME_LEN: usize = 91;
const RTA_BIN_OFFSET: usize = 60;
const RTA_BIN_COUNT: usize = 31;

/// Transition sentinel: the desk parks all RTA bins at −999 dB while the
/// shared source settles. An all-sentinel frame is dropped; finite quiet
/// bins (real low-level signal) are valid and kept.
const RTA_SENTINEL_DB: f32 = -999.0;

/// Source-settle window: after a selector/generation change, frames are
/// withheld this long so in-flight old-source words are never
/// misattributed. Enforced in [`Dl32sDriver::poll_spectrum`] off the
/// existing ingest/poll path — no extra timer.
const RTA_SETTLE: Duration = Duration::from_millis(500);

/// Nominal ISO 1/3-octave centers backing the 31-band RTA
/// (rig-corroborated: an independent two-tone 220 Hz peaked addr 13/14 and
/// 1 kHz addr 20, and the reference manual specifies 31 bands; the other 29
/// bands were not individually excited). Positions are trusted; exact band
/// shapes were not measured.
const RTA_CENTERS_HZ: [f32; RTA_BIN_COUNT] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
    400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0,
    5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

/// Nominal 32 band edges for [`RTA_CENTERS_HZ`]: geometric midpoints
/// between adjacent centers, ends geometrically extrapolated
/// (`edge0 = c0²/mid0`, `edge31 = c30²/mid29`). Nominal — not measured on
/// the desk; carried only because [`AudioSpectrum`] requires edges.
fn rta_band_edges_hz() -> Vec<f32> {
    let c = RTA_CENTERS_HZ;
    let mut edges = Vec::with_capacity(RTA_BIN_COUNT + 1);
    let mid0 = (c[0] * c[1]).sqrt();
    edges.push(c[0] * c[0] / mid0);
    for w in c.windows(2) {
        edges.push((w[0] * w[1]).sqrt());
    }
    let mid_last = (c[RTA_BIN_COUNT - 2] * c[RTA_BIN_COUNT - 1]).sqrt();
    edges.push(c[RTA_BIN_COUNT - 1] * c[RTA_BIN_COUNT - 1] / mid_last);
    edges
}

/// Cached passive RTA frame (shared Return 1–2 source): written only by the
/// connection ingest task, read by `poll_spectrum`. `selector` is the last
/// type-2 addr-22 value; `epoch` is monotonic across source/generation
/// changes (`generation << 32` base, +1 per source change); `switched_at`
/// gates the [`RTA_SETTLE`] window; `updated_at`/`bins_db` are the last
/// decoded 31-bin frame. Stale (>1 s, same bound as [`METER_STALE_AFTER`])
/// reads unavailable (empty) instead of frozen session data.
#[derive(Debug, Default)]
struct SpectrumState {
    generation: u64,
    selector: Option<u32>,
    epoch: u64,
    switched_at: Option<std::time::Instant>,
    updated_at: Option<std::time::Instant>,
    bins_db: Vec<f32>,
}

/// Private generation-tagged wrapper around the public
/// [`openfader_core::NativeSnapshots`] view: `generation` names the
/// connection that owns `view` (same pattern as [`MeterState`] /
/// [`SpectrumState`]). The ingest task compares inside the lock so a
/// stale task can never write into a newer session's view, and the
/// getter withholds a mismatched generation. The public core model stays
/// untagged.
#[derive(Debug, Default)]
struct NativeCache {
    generation: u64,
    view: openfader_core::NativeSnapshots,
}

/// Decode one meter push's RTA bins (indices 60..90, BE-f32 dB), or `None`
/// when the shape is not the exact 91-word one, any bin is non-finite, or
/// all bins sit at the [`RTA_SENTINEL_DB`] transition sentinel.
fn decode_rta_bins(push: &PushEvent) -> Option<Vec<f32>> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_METERS => (*start, raws),
        _ => return None,
    };
    if start != METER_START || raws.len() != RTA_FRAME_LEN {
        return None;
    }
    let mut out = Vec::with_capacity(RTA_BIN_COUNT);
    for raw in raws.iter().skip(RTA_BIN_OFFSET).take(RTA_BIN_COUNT) {
        let db = f32::from_bits(*raw);
        if !db.is_finite() {
            return None;
        }
        out.push(db);
    }
    if out.len() != RTA_BIN_COUNT {
        return None;
    }
    if out.iter().all(|v| *v == RTA_SENTINEL_DB) {
        return None;
    }
    Some(out)
}

/// Extract the shared RTA selector (type-2 addr 22) from a push that covers
/// it. Mirrors the transport type-2 cache; the ingest task feeds this so
/// source changes clear the cache/epoch without a session lock on the poll
/// path.
fn rta_selector_from_push(push: &PushEvent) -> Option<u32> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_MUTE_GROUP => (*start, raws),
        _ => return None,
    };
    let i = RTA_SELECTOR_ADDR.checked_sub(start)? as usize;
    raws.get(i).copied()
}

/// Bulk-state readiness bound: prior hardware runs saw the whole-space
/// push settle tens of seconds after init on a slow path, so 60 s keeps
/// headroom without hanging `Cmd::Connect` forever.
const INITIAL_STATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Wait until the mixer's initial bulk push has populated the session
/// cache. Sentinel choice is deliberate: CH1 and CH32 mute+fader are
/// verified input leaves at opposite ends of the bulk, so their presence
/// proves both chunks landed; the main fader word proves the master
/// block did (its field domain is unverified, but the word itself is
/// expected in the whole-space push). Returns an `awaiting initial mixer
/// state` error on timeout — never TCP-success with zero audio data.
async fn wait_for_initial_state(session: &MackieSession) -> anyhow::Result<()> {
    let table = dl32s_inputs();
    let sentinels: [(&str, u32); 5] = [
        ("ch1 mute", table.address(1, "mute").expect("table static")),
        ("ch1 fader", table.address(1, "fader").expect("table static")),
        ("ch32 mute", table.address(32, "mute").expect("table static")),
        ("ch32 fader", table.address(32, "fader").expect("table static")),
        (
            "main fader",
            addrs::dl32s_main().address(1, "fader").expect("table static"),
        ),
    ];
    let deadline = std::time::Instant::now() + INITIAL_STATE_TIMEOUT;
    loop {
        // Liveness gate (r50): check BEFORE the sentinel-success test so a
        // closed transport can never report ready from stale cache words.
        if !session.is_alive() {
            anyhow::bail!(
                "dl32s: transport closed before initial mixer state arrived; reconnect once the mixer is reachable"
            );
        }
        if sentinels.iter().all(|(_, addr)| session.cached(*addr).is_some()) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let missing: Vec<String> = sentinels
                .iter()
                .filter(|(_, addr)| session.cached(*addr).is_none())
                .map(|(name, addr)| format!("{name} ({addr})"))
                .collect();
            anyhow::bail!(
                "dl32s: awaiting initial mixer state: no bulk push for {} after {:?}; reconnect once the mixer is broadcasting",
                missing.join(", "),
                INITIAL_STATE_TIMEOUT
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Canonical id -> documented modern-DL32S `(min, max)`, served through
/// [`MixerDriver::param_range_overrides`](openfader_core::MixerDriver::param_range_overrides).
/// Provenance per entry (Master Fader 5.1 manual unless noted):
/// - `gain 0..60`: p90 mic/balanced slider (CH7 MicPre native 0 observed).
/// - `trim -40..+20`: p91.
/// - `hpf_freq 20..700`: p103 Modern EQ; CH7 endpoints verified against
///   Master Fader 5.2 with independent readback and restore 2026-10-25.
/// - `gate_att 0.1..300`, `gate_hold 0..5000`, `gate_rel 50..3000` ms,
///   `gate_range 0..60` dB: native CH7 endpoints independently read and
///   restored 2026-10-25. Vintage timing selections share these values.
/// - `gate_mode 0..1`: CH7 native Modern/Vintage selector verified with
///   independent readback and restore 2026-10-25 (canonical has five modes).
/// - `eqN_q 0.5..16`: p103 Modern EQ (all four bands).
/// - `comp_thr -80..0`: p112 Modern compressor.
/// - `comp_makeup 0..20`: p112 Modern compressor gain.
///
/// EQ gains need no entry (canonical already documents ±15 per p103);
/// `gate_thr` needs none (canonical already -80..0 per p112).
static DL32S_RANGE_OVERRIDES: &[(&str, f32, f32)] = &[
    ("gain", 0.0, 60.0),
    ("trim", -40.0, 20.0),
    ("hpf_freq", 20.0, 700.0),
    ("gate_mode", 0.0, 1.0),
    ("gate_att", 0.1, 300.0),
    ("gate_hold", 0.0, 5000.0),
    ("gate_rel", 50.0, 3000.0),
    ("gate_range", 0.0, 60.0),
    ("eq1_q", 0.5, 16.0),
    ("eq2_q", 0.5, 16.0),
    ("eq3_q", 0.5, 16.0),
    ("eq4_q", 0.5, 16.0),
    ("comp_thr", -80.0, 0.0),
    ("comp_makeup", 0.0, 20.0),
];

// Native CH7 selections, independently read and restored 2026-10-25.
// Unmatched timings retain their exact value; clients must not snap them
// to a named selection merely by displaying this metadata.
static DL32S_PARAM_PRESENTATIONS: &[openfader_core::params::ParamPresentation] = &[
    openfader_core::params::ParamPresentation {
        id: "gate_mode", when: None, hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 0.0, label: "Modern" },
            openfader_core::params::ParamChoice { value: 1.0, label: "Vintage" },
        ],
    },
    openfader_core::params::ParamPresentation {
        id: "gate_mode", when: Some(("gate_mode", 0.0)), hidden: false,
        curve: Some("gate_attenuation"), choices: &[
            openfader_core::params::ParamChoice { value: 0.0, label: "Modern" },
            openfader_core::params::ParamChoice { value: 1.0, label: "Vintage" },
        ],
    },
    openfader_core::params::ParamPresentation {
        id: "gate_att", when: Some(("gate_mode", 1.0)), hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 0.1, label: "Fast" },
            openfader_core::params::ParamChoice { value: 50.0, label: "Medium" },
            openfader_core::params::ParamChoice { value: 300.0, label: "Slow" },
        ],
    },
    openfader_core::params::ParamPresentation {
        id: "gate_rel", when: Some(("gate_mode", 1.0)), hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 50.0, label: "Fast" },
            openfader_core::params::ParamChoice { value: 500.0, label: "Medium" },
            openfader_core::params::ParamChoice { value: 3000.0, label: "Slow" },
        ],
    },
    openfader_core::params::ParamPresentation {
        id: "gate_range", when: Some(("gate_mode", 1.0)), hidden: true, choices: &[], curve: None,
    },
];

/// Live DL32S-class mixer on TCP 50001.
pub struct Dl32sDriver {
    session: Mutex<Option<MackieSession>>,
    addr: Mutex<Option<String>>,
    update_tx: broadcast::Sender<ParamUpdate>,
    generation: Arc<AtomicU64>,
    meters: Arc<std::sync::Mutex<MeterState>>,
    spectrum: Arc<std::sync::Mutex<SpectrumState>>,
    /// Read-only native snapshot view (see [`NativeCache`]): seeded from
    /// the session's push-only verified stores on connect (covering pushes
    /// that arrived before this connection's ingest subscription), then
    /// updated from genuine `SnapshotRecall`/`SnapshotRename` pushes by
    /// the ingest task. Own saves/deletes never touch it. Reset on
    /// disconnect so a dead session reads unavailable, never the last desk.
    native: Arc<std::sync::Mutex<NativeCache>>,
}

impl Default for Dl32sDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Dl32sDriver {
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(256);
        Self {
            session: Mutex::new(None),
            addr: Mutex::new(None),
            update_tx,
            generation: Arc::new(AtomicU64::new(0)),
            meters: Arc::new(std::sync::Mutex::new(MeterState::default())),
            spectrum: Arc::new(std::sync::Mutex::new(SpectrumState::default())),
            native: Arc::new(std::sync::Mutex::new(NativeCache::default())),
        }
    }

    fn channel(strip: StripId) -> anyhow::Result<u32> {
        let ch = strip.0 as u32 + 1;
        if !(1..=INPUT_COUNT).contains(&ch) {
            anyhow::bail!("channel {} out of range 1..={INPUT_COUNT}", strip.0);
        }
        Ok(ch)
    }

    fn check_filename(filename: &str) -> anyhow::Result<()> {
        if filename.is_empty() || filename.len() > 128 {
            anyhow::bail!("dl32s: filename must be 1..=128 chars");
        }
        if filename.contains(['/', '\\', '\0']) {
            anyhow::bail!("dl32s: filename {filename:?} must not contain path separators");
        }
        Ok(())
    }

    async fn session(&self) -> anyhow::Result<MutexGuard<'_, Option<MackieSession>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        // Liveness gate (r50): a stored session whose transport tasks have
        // finished is a closed TCP connection, not a live desk. Report it as
        // a transport closure (wording avoids the engine's drop-class
        // substrings) so callers reconnect instead of serving stale cache.
        if !guard.as_ref().expect("checked above").is_alive() {
            anyhow::bail!("dl32s: transport closed: connection lost; reconnect once the mixer is reachable");
        }
        Ok(guard)
    }

    /// Write one table field, encoding the value per its wire type.
    /// Value-shape mistakes are reported before the connection is touched,
    /// so mapping bugs surface even offline.
    async fn set_field(&self, strip: StripId, key: &str, value: ParamValue) -> anyhow::Result<()> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let addr = table.address(Self::channel(strip)?, key)?;
        enum Out {
            Int(u32),
            Float(f32),
        }
        let out = match (field.encoding, value) {
            (addrs::Encoding::Bool, ParamValue::Bool(b)) => Out::Int(u32::from(b)),
            (addrs::Encoding::Enum | addrs::Encoding::Int, ParamValue::Float(v)) => {
                if !v.is_finite() {
                    anyhow::bail!("{key} must be finite, got {v}");
                }
                Out::Int(v as u32)
            }
            (enc, ParamValue::Float(v))
                if enc == addrs::Encoding::Db || enc == addrs::Encoding::Float =>
            {
                if !v.is_finite() {
                    anyhow::bail!("{key} must be finite, got {v}");
                }
                if let Some((lo, hi)) = field.limits {
                    if !(lo..=hi).contains(&v) {
                        anyhow::bail!("{key} must be within {lo}..{hi}, got {v}");
                    }
                }
                Out::Float(v)
            }
            (enc, got) => anyhow::bail!("{key} has {enc:?} encoding, got {got:?}"),
        };
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        match out {
            Out::Int(v) => session.write_int(addr, v, VALUE_TYPE_PARAM).await,
            Out::Float(v) => session.write_float(addr, v).await,
        }
    }

    /// Read one table field back from the desk-proven push cache.
    ///
    /// Uses [`cached_verified`](openfader_transport::mackie::MackieSession::cached_verified):
    /// own-write-pending words are withheld (no desk proof), so a post-write
    /// read reports "no desk-confirmed value yet" instead of echoing our own
    /// optimism back as confirmation. The error text stays out of the
    /// engine's drop-class substrings (`not supported`/`timed out`) so the
    /// poller keeps polling instead of evicting the param.
    async fn get_field(&self, strip: StripId, key: &str) -> anyhow::Result<ParamValue> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let addr = table.address(Self::channel(strip)?, key)?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached_verified(addr)
            .ok_or_else(|| anyhow::anyhow!("{key}: no desk-confirmed value yet"))?;
        if field.encoding.is_float() {
            Ok(ParamValue::Float(f32::from_bits(raw)))
        } else if field.encoding == addrs::Encoding::Bool {
            Ok(ParamValue::Bool(raw != 0))
        } else {
            Ok(ParamValue::Float(raw as f32))
        }
    }

    /// Desk-proven fader read (verification path): withholds own-write-pending
    /// words. Display/health paths use the inclusive
    /// [`MixerDriver::get_fader`] instead (continuity, no proof claimed).
    async fn verified_fader(&self, addr: u32) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached_verified(addr)
            .ok_or_else(|| anyhow::anyhow!("fader: no desk-confirmed value yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    /// Desk-proven mute read (verification path — see [`verified_fader`](Self::verified_fader)).
    async fn verified_mute(&self, addr: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_verified(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("mute: no desk-confirmed value yet"))
    }

    /// Desk-proven name read (verification path — see [`verified_fader`](Self::verified_fader)).
    async fn verified_name(&self, ch: u32) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name_verified(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no desk-confirmed value yet"))
    }

    /// Desk-proven raw u32 read (verification path for assign membership).
    /// Display paths use [`request_value_u32`](Self::request_value_u32).
    async fn verified_value_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        self.request_value_verified_u32(address).await
    }

    /// Desk-proven input Solo; pending own writes are withheld from polling.
    pub async fn get_solo_verified(&self, strip: StripId) -> anyhow::Result<bool> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .cached_type2_verified(address)
            .map(|raw| raw != 0)
            .ok_or_else(|| anyhow::anyhow!("solo: no desk-confirmed value yet"))
    }

    /// Mute/unmute a mute group master (fixed protocol address, no table needed).
    pub async fn set_mute_group(&self, group_1_based: u32, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_mute_group(group_1_based, muted)
            .await
    }

    /// Mute-group master state for DISPLAY continuity (attach/snapshot/health):
    /// inclusive read — own-write-pending words included, no proof claimed.
    /// Verification paths must use [`get_mute_group_verified`](Self::get_mute_group_verified).
    pub async fn get_mute_group(&self, group_1_based: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_mute_group(group_1_based)
            .ok_or_else(|| anyhow::anyhow!("mute group {group_1_based}: no value pushed yet"))
    }

    /// Desk-proven mute-group master (withholds own-write-pending values).
    /// Verification/poll/readback paths must use this, never [`get_mute_group`](Self::get_mute_group).
    pub async fn get_mute_group_verified(&self, group_1_based: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_mute_group_verified(group_1_based)
            .ok_or_else(|| anyhow::anyhow!("mute group {group_1_based}: no desk-confirmed value yet"))
    }

    /// Desk-proven mute-group assign (withholds own-write-pending words).
    /// Verification paths must use this, never the inclusive trait getter.
    pub async fn get_mute_group_assign_verified(&self, ch: StripId, group_0_based: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group_0_based)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
        let v = self.verified_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("mute-group assign: no desk-confirmed value yet"))?;
        Ok(v != 0)
    }

    /// Recall a mixer show snapshot the way the mixer app does (op 1 + ack).
    pub async fn recall_show(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .recall_show_snapshot(snapshot_1_based)
            .await
    }

    /// Save current state into a snapshot slot under `name` (op 5).
    pub async fn save_show(&self, snapshot_1_based: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .save_show_snapshot(snapshot_1_based, name)
            .await
    }

    // ---- aux sends (per input strip) ----

    pub async fn set_aux_mute(&self, channel: u32, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_mute"))?;
        let _ = aux; // validation via address table
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_aux_fader_db(&self, channel: u32, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_level"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    pub async fn set_aux_pan(&self, channel: u32, aux: u32, pan: f32) -> anyhow::Result<()> {
        let pan = pan.clamp(-1.0, 1.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_pan"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, pan).await
    }

    // ---- subgroups ----

    pub async fn set_subgroup_mute(&self, group: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_subgroups().address(group, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_subgroup_fader_db(&self, group: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_subgroups().address(group, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- main ----

    pub async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_main().address(1, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_main_fader_db(&self, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_main().address(1, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- aux masters ----

    pub async fn set_auxmaster_mute(&self, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_auxmasters().address(aux, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_auxmaster_fader_db(&self, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_auxmasters().address(aux, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- rename (channel names) ----

    pub async fn set_channel_name(&self, slot: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_channel_name(slot, name).await
    }

    // ---- mute group (already) + snapshot delete/reboot/etc via session ----

    pub async fn delete_snapshot(&self, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").delete_snapshot(slot).await
    }

    pub async fn reboot(&self, mode: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").reboot(mode).await
    }

    pub async fn disconnect_notice(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").disconnect_notice().await
    }

    // ---- file transfer ----

    pub async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").download_file(filename, REQUEST_TIMEOUT).await
    }

    pub async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").upload_file(filename, data, file_type, REQUEST_TIMEOUT).await
    }

    pub async fn set_file_crc(&self, address: u32, crc: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_file_crc(address, crc).await
    }

    pub async fn process_files(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").process_files().await
    }

    pub async fn refresh_snapshot_names(&self) -> anyhow::Result<std::collections::HashMap<u32, String>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let _ = session.download_file("Show.zip", REQUEST_TIMEOUT).await;
        Ok(session.snapshot_names())
    }

    // ---- device admin ----

    pub async fn get_device_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_device_name(REQUEST_TIMEOUT).await
    }
    pub async fn set_device_name(&self, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_device_name(name, REQUEST_TIMEOUT).await
    }
    pub async fn get_subdevice_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_subdevice_name(REQUEST_TIMEOUT).await
    }
    pub async fn get_ip_address(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_ip_address(REQUEST_TIMEOUT).await
    }
    pub async fn get_lock_status(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_lock_status(REQUEST_TIMEOUT).await
    }
    pub async fn get_static_ip_config(&self) -> anyhow::Result<(bool, String, String, String)> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_static_ip_config(REQUEST_TIMEOUT).await
    }
    pub async fn set_static_ip_config(&self, is_static: bool, address: &str, mask: &str, gateway: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_static_ip_config(is_static, address, mask, gateway, REQUEST_TIMEOUT).await
    }
    pub async fn get_max_message_words(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_max_message_words(REQUEST_TIMEOUT).await
    }
    pub async fn get_network_config(&self) -> anyhow::Result<Vec<u32>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_network_config(REQUEST_TIMEOUT).await
    }
    pub async fn site_survey(&self) -> anyhow::Result<Vec<[u32; 11]>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").site_survey(REQUEST_TIMEOUT).await
    }
    pub async fn get_transport_status(&self, kind: &str) -> anyhow::Result<openfader_transport::mackie::Message> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let subtype = match kind {
            "transport" => 8u32,
            "multitrack" => 9u32,
            "music" => 10u32,
            _ => anyhow::bail!("kind must be transport, multitrack or music"),
        };
        session.request_general_info(subtype, REQUEST_TIMEOUT).await
    }
    pub async fn get_show_info(&self) -> anyhow::Result<openfader_transport::mackie::Message> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_show_info(REQUEST_TIMEOUT).await
    }
    pub async fn show_ok(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").show_ok().await
    }
    pub async fn get_show_crc(&self, pid: u32) -> anyhow::Result<openfader_transport::mackie::Message> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_show_crc(pid, REQUEST_TIMEOUT).await
    }
    pub async fn set_show_parameter(&self, pid: u32, value: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_show_parameter(pid, value).await
    }
    pub async fn set_show_name(&self, pid: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_show_name(pid, name).await
    }
    pub async fn enable_meter_reports(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").enable_meter_reports().await
    }
    pub async fn recall_snapshot_addr(&self, address: u32, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").recall_snapshot(address, slot).await
    }
    pub async fn set_values(&self, values: &std::collections::HashMap<u32, u32>, value_type: u8) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_values(values, value_type).await
    }
    pub async fn raw_set_value_int(&self, address: u32, value: i64) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(address, value as u32, VALUE_TYPE_PARAM).await
    }
    pub async fn raw_set_value_float(&self, address: u32, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(address, value).await
    }
    pub async fn request_value_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        Ok(session.cached(address))
    }

    /// Desk-proven variant of [`request_value_u32`](Self::request_value_u32):
    /// withholds own-write-pending words (no desk proof).
    pub async fn request_value_verified_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        Ok(session.cached_verified(address))
    }
}

impl Dl32sDriver {
    fn dl_check_bus(bus: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_auxmasters().count;
        if (bus as u32) >= count {
            anyhow::bail!("dl32s: bus {bus} out of range 0..{count} (0-based)");
        }
        Ok(bus as u32 + 1)
    }

    fn dl_check_subgroup(group: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_subgroups().count;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: subgroup {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }

    fn dl_check_mute_group(group: u8) -> anyhow::Result<u32> {
        let count = MUTE_GROUP_COUNT;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: mute group {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }
}

impl MixerDriver for Dl32sDriver {
    fn name(&self) -> &'static str {
        "dl32s"
    }

    /// DL32S strip map: 32 input channels only (honest — output-strip
    /// leaves are served through the bus/subgroup/mute-group/main
    /// methods below, not as pollable strips, so no extra topology is
    /// advertised).
    fn topology(&self) -> Vec<openfader_core::StripFamily> {
        vec![openfader_core::StripFamily {
            key: "ch",
            kind: openfader_core::StripKind::In,
            label: "Ch 1-32",
            first: 0,
            count: INPUT_COUNT as usize,
            name_prefix: "Ch",
            color_default: 2,
        }]
    }

    fn strip_count(&self) -> usize {
        INPUT_COUNT as usize
    }

    /// Wave-1+2 verified surface: every id names a confirmed DL32S leaf
    /// (see [`DL32S_CAPABILITIES`]). The poller reads exactly this set.
    fn capabilities(&self) -> &'static [&'static str] {
        DL32S_CAPABILITIES
    }

    /// Provider rows for confirmed leaves with no canonical equivalent:
    /// `eq_mode` (write-proven 0/1, modern/vintage per Mackie convention).
    /// Canonical ids win on collision, so a future canonical `eq_mode`
    /// upgrades this row seamlessly.
    fn param_defs(&self) -> &'static [openfader_core::params::ParamDef] {
        static DEFS: std::sync::OnceLock<[openfader_core::params::ParamDef; 4]> =
            std::sync::OnceLock::new();
        DEFS.get_or_init(|| {
            let source = |id, max, label| openfader_core::params::ParamDef {
                id,
                kind: openfader_core::params::ParamKind::Enum,
                scope: openfader_core::params::ParamScope::Channel,
                scopes: openfader_core::params::SCOPES_INPUT,
                min: 0.0,
                max,
                unit: "",
                default: openfader_core::params::ParamDefault::Float(0.0),
                label,
            };
            [
                openfader_core::params::ParamDef {
                    id: "eq_mode",
                    kind: openfader_core::params::ParamKind::Enum,
                    scope: openfader_core::params::ParamScope::Channel,
                    scopes: openfader_core::params::SCOPES_EQ,
                    min: 0.0,
                    max: 1.0,
                    unit: "",
                    default: openfader_core::params::ParamDefault::Float(0.0),
                    label: "EQ modern/vintage",
                },
                // The source IDs are opaque native integers; the app only reads
                // these two rows, preserving the desk's actual patch labels/IDs.
                source("source_a", u32::MAX as f32, "Input A source ID"),
                source("source_b", u32::MAX as f32, "Input B source ID"),
                source("source_select", 1.0, "Input A/B"),
            ]
        })
    }

    /// Documented modern-DL32S engineering-unit ranges (Master Fader 5.1
    /// manual; the desk runs native Master Fader 5.2 / mixer firmware 1.6).
    /// Applied over the canonical (X-Air-shaped) catalogue in this
    /// provider's registry only — sim and xair keep canonical ranges.
    /// Every entry below has known units and a direct wire encoding (dB
    /// leaves ride `Encoding::Db`, Hz/unitless leaves ride
    /// `Encoding::Float`); kinds, scopes, labels and defaults are
    /// untouched. Except for the live-verified HPF endpoints and Gate controls
    /// domain noted in `DL32S_RANGE_OVERRIDES`, bounds are documentation-
    /// based (hardware endpoint writes pending); the desk is believed to
    /// clamp outside its maxima.
    /// Deliberately absent (canonical kept, with reason):
    /// - Line-input `-20..40` effective attenuation (p90): no
    ///   input-socket detection exists, so the single mic/balanced `0..60`
    ///   slider is served (CH7 MicPre native 0 observed, consistent); the
    ///   existing wire bound is retained without assuming how the physical
    ///   offset is represented.
    /// - Compressor timing leaves (`comp_att/hold/rel`): wire floats with
    ///   no confirmed endpoint mapping — canonical domains kept.
    /// - `comp_ratio` `1..INF` (p112): no known INF wire encoding — no
    ///   sentinel invented; finite canonical `1.1..=100` kept (the desk
    ///   clamps above its max; direct floats 1.68..20.2 observed).
    /// - Other mode leaves (`comp_mode/comp_knee/eq_mode`):
    ///   modern/vintage and soft/hard polarities assumed per Mackie
    ///   convention, labels unverified — no mode-aware switching fabricated.
    ///   Gate Modern=0/Vintage=1 is live-verified; its range is overridden.
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        DL32S_RANGE_OVERRIDES
    }

    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        DL32S_PARAM_PRESENTATIONS
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        addrs::model_status("dl32s")?;
        let session = MackieSession::connect(addr).await?;
        // Readiness gate: the wire init handshake is best-effort and
        // returns once the TCP exchange completes, but the live value
        // space only lands in the push cache as the mixer broadcasts it
        // (bulk chunks arrive seconds later on a slow path). Exposing the
        // session before that serves fabricated snapshot defaults as real
        // state, so wait for the sentinel words before publishing. On
        // timeout the session is closed (transport tasks abort on close /
        // drop) and no ingest task is spawned, so nothing leaks.
        if let Err(e) = wait_for_initial_state(&session).await {
            session.close();
            return Err(e);
        }
        // Desk-broadcast ingest: decode every pushed frame into the stable
        // update channel (one task per connection; it exits when the next
        // connect bumps the generation or the session drops). Mirrors the
        // X-Air session->driver forward path. Type-1 meter blocks ride a
        // separate branch into the input-meter cache (first 32 words only);
        // `decode_push` keeps dropping them as params.
        // DL32S-only 91-entry meter layout (per-client override through the
        // transport layout hook): RTA bins ride indices 60..90 while the
        // first 32 input words stay put. Best-effort — a refused layout just
        // leaves RTA unavailable while input meters keep flowing.
        if let Err(e) = session
            .request_meter_layout(&openfader_transport::mackie::dl32s_meter_addresses())
            .await
        {
            eprintln!("dl32s: RTA meter layout override failed (input meters continue, RTA unavailable): {e:#}");
        }
        // Liveness recheck (r50): the layout override above awaited two
        // correlated round trips. Recheck BEFORE publishing the session or
        // spawning ingest so a transport that died mid-await never becomes
        // the stored session. Cleanup mirrors the readiness-gate failure:
        // close the local session (aborts transport tasks on close/drop),
        // spawn nothing, assign nothing.
        if !session.is_alive() {
            session.close();
            anyhow::bail!(
                "dl32s: transport closed before initial mixer state arrived; reconnect once the mixer is reachable"
            );
        }
        // Retire the previous session's ingest/cache only after all
        // fallible readiness checks succeed. A failed replacement must
        // leave the retained session's generation and telemetry intact.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut m) = self.meters.lock() {
            m.generation = generation;
            m.updated_at = None;
            m.levels.clear();
        }
        // Initial shared-source read: the rig-confirmed initial bulk carries
        // the type-2 addr-22 selector (5000 = Return 1–2). Treated as a
        // source change (fresh settle window) so early frames are never
        // misattributed.
        // Subscribe before sampling the cache so a selector update cannot
        // land between the initial read and the live subscription.
        let mut rx = session.subscribe_pushes();
        let initial_selector = session.cached_type2(RTA_SELECTOR_ADDR);
        if let Ok(mut s) = self.spectrum.lock() {
            s.generation = generation;
            s.selector = initial_selector;
            s.epoch = generation << 32;
            s.switched_at = Some(std::time::Instant::now());
            s.updated_at = None;
            s.bins_db.clear();
        }
        let tx = self.update_tx.clone();
        let gen = self.generation.clone();
        let meters = self.meters.clone();
        let spectrum = self.spectrum.clone();
        // Seed the read-only native snapshot view from the session's
        // push-only verified stores (covers snapshot pushes that arrived
        // before this subscription; the broadcast channel buffers anything
        // newer until the task below drains it). Verified stores only —
        // own-save optimism never seeds this view.
        let native = self.native.clone();
        if let Ok(mut n) = native.lock() {
            let mut slots: Vec<openfader_core::NativeSnapshotSlot> = session
                .snapshot_names_pushed()
                .into_iter()
                .map(|(slot, name)| openfader_core::NativeSnapshotSlot { slot, name })
                .collect();
            slots.sort_by_key(|s| s.slot);
            n.generation = generation;
            n.view.slots = slots;
            n.view.current = session.current_snapshot_pushed();
            n.view.received = !n.view.slots.is_empty() || n.view.current.is_some();
            n.view.connected = true;
        }
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(push) => {
                        if gen.load(Ordering::Acquire) != generation {
                            break;
                        }
                        // Passive shared-source tracking (no writes): a
                        // type-2 push covering addr 22 reselects the source —
                        // clear the cache, bump the epoch, restart the
                        // settle window.
                        if let Some(sel) = rta_selector_from_push(&push) {
                            if let Ok(mut s) = spectrum.lock() {
                                if s.generation == generation && s.selector != Some(sel) {
                                    s.selector = Some(sel);
                                    s.epoch += 1;
                                    s.bins_db.clear();
                                    s.updated_at = None;
                                    s.switched_at = Some(std::time::Instant::now());
                                }
                            }
                        }
                        if let Some(levels) = decode_input_meters(&push) {
                            if let Ok(mut m) = meters.lock() {
                                m.generation = generation;
                                m.updated_at = Some(std::time::Instant::now());
                                m.levels = levels;
                            }
                            // The 91-word frame carries both planes: meters
                            // above, RTA bins at 60..90. A 60-word frame
                            // yields meters only (exact-shape gate inside).
                            if let Some(bins) = decode_rta_bins(&push) {
                                if let Ok(mut s) = spectrum.lock() {
                                    if s.generation == generation {
                                        s.bins_db = bins;
                                        s.updated_at = Some(std::time::Instant::now());
                                    }
                                }
                            }
                            continue;
                        }
                        // Read-only native snapshot tracking: genuine desk
                        // recall/rename pushes update the view (received
                        // marks PARTIAL observed metadata — even a bare
                        // recall with no names proves only the last
                        // reported recall, never a full inventory). No
                        // param fanout: `decode_push` already maps these to
                        // nothing, and this view is sampled by the engine
                        // ticker, never fanned out as strip updates. The
                        // inside-lock generation check closes the
                        // seed race: a stale task that passed the outer
                        // check above must not write into the newer
                        // session's view.
                        match &push {
                            openfader_transport::mackie::PushEvent::SnapshotRecall(n) => {
                                if let Ok(mut v) = native.lock() {
                                    if v.generation != generation {
                                        break;
                                    }
                                    if v.view.connected {
                                        v.view.current = Some(*n);
                                        v.view.received = true;
                                    }
                                }
                            }
                            openfader_transport::mackie::PushEvent::SnapshotRename { slot, name } => {
                                if let Ok(mut v) = native.lock() {
                                    if v.generation != generation {
                                        break;
                                    }
                                    if v.view.connected {
                                        match v.view.slots.iter_mut().find(|s| s.slot == *slot) {
                                            Some(row) => row.name = name.clone(),
                                            None => v.view.slots.push(openfader_core::NativeSnapshotSlot {
                                                slot: *slot,
                                                name: name.clone(),
                                            }),
                                        }
                                        v.view.slots.sort_by_key(|s| s.slot);
                                        v.view.received = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                        for update in decode_push(push) {
                            let _ = tx.send(update);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        *self.session.lock().await = Some(session);
        *self.addr.lock().await = Some(addr.to_string());
        Ok(())
    }

    /// Live desk-originated strip updates (another client's moves arrive
    /// without a poll sweep). Values are engineering units, same as
    /// `get_param`; the push plane equals the caps plane.
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<ParamUpdate>> {
        Some(self.update_tx.subscribe())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let level = validate_fader(value)?;
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "fader")?;
        let db = addrs::DL32S_FADER_SCALE.forward(level);
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    /// DISPLAY continuity read (attach/snapshot/health/state): inclusive —
    /// own writes included, no proof claimed. Verification uses `get_param`
    /// (desk-proven) instead.
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .write_int(address, u32::from(soloed), VALUE_TYPE_MUTE_GROUP).await
    }

    /// Display continuity only; polling uses `get_solo_verified`.
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .cached_type2(address)
            .map(|raw| raw != 0)
            .ok_or_else(|| anyhow::anyhow!("solo: no value pushed yet"))
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let ch = Self::channel(strip)?;
        self.set_channel_name(ch, name).await
    }
    /// DISPLAY continuity read (inclusive).
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = Self::channel(strip)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no value pushed yet"))
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.set_main_fader_db(addrs::DL32S_FADER_SCALE.forward(value)).await
    }
    /// DISPLAY continuity read (inclusive).
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        let addr = addrs::dl32s_main().address(1, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("main fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    /// Main LR master mute: the inherent `set_main_mute` leaf exists; the
    /// read rides the pushed-value cache like every other DL32S mute.
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        self.set_main_mute(muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        let addr = addrs::dl32s_main().address(1, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("main mute: no value pushed yet"))
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("mute: no value pushed yet"))
    }

    // ---- R2 mix-bus masters (aux masters 1..8, 0-based bus) ----
    fn bus_count(&self) -> u8 {
        addrs::dl32s_auxmasters().count as u8
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let aux = Self::dl_check_bus(bus)?;
        self.set_auxmaster_fader_db(aux, addrs::DL32S_FADER_SCALE.forward(validate_fader(value)?)).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let aux = Self::dl_check_bus(bus)?;
        let addr = addrs::dl32s_auxmasters().address(aux, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("bus fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        let aux = Self::dl_check_bus(bus)?;
        self.set_auxmaster_mute(aux, muted).await
    }

    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        let aux = Self::dl_check_bus(bus)?;
        let addr = addrs::dl32s_auxmasters().address(aux, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("bus mute: no value pushed yet"))
    }

    // ---- R2 subgroup masters (1..6, 0-based group) + membership ----
    fn subgroup_count(&self) -> u8 {
        addrs::dl32s_subgroups().count as u8
    }

    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        self.set_subgroup_fader_db(g, addrs::DL32S_FADER_SCALE.forward(validate_fader(value)?)).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
        let g = Self::dl_check_subgroup(group)?;
        let addr = addrs::dl32s_subgroups().address(g, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("subgroup fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        self.set_subgroup_mute(g, muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_subgroup(group)?;
        let addr = addrs::dl32s_subgroups().address(g, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("subgroup mute: no value pushed yet"))
    }

    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("sub{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_subgroup(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("sub{g}"))?;
        let v = self.request_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("subgroup assign: no value pushed yet"))?;
        Ok(v != 0)
    }

    // ---- R2 mute-group masters + membership ----
    fn mute_group_count(&self) -> u8 {
        MUTE_GROUP_COUNT as u8
    }

    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_mute_group(group)?;
        self.set_mute_group(g, muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        self.get_mute_group(g).await
    }

    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_mute_group(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
        let v = self.request_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("mute-group assign: no value pushed yet"))?;
        Ok(v != 0)
    }

    // ---- R2 file transfer (Show.zip up/download) ----
    fn file_transfer(&self) -> bool {
        true
    }

    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        Self::check_filename(filename)?;
        self.download_file(filename).await
    }

    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        Self::check_filename(filename)?;
        self.upload_file(filename, data, file_type).await
    }

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match (param, value) {
            ("source_select", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: source_select must be 1 (A) or 0 (B), got {v}");
                }
                self.set_field(strip, "source_select", ParamValue::Float(v))
                    .await
            }
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("name", _) => anyhow::bail!("dl32s: set_name requires text"),
            // Canonical pan is -100..+100; the wire leaf is -1.0..+1.0.
            ("pan", ParamValue::Float(v)) => {
                self.set_field(strip, "pan", ParamValue::Float(v / 100.0)).await
            }
            // Binary-confirmed modes (live survey 2026-10-20 saw only 0/1
            // across 32 channels): modern=0/vintage=1 per Mackie convention
            // (polarity assumed — the desk accepts both, labels may swap).
            // Anything else is rejected before it reaches the wire.
            ("comp_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: comp_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "comp_mode", ParamValue::Float(v)).await
            }
            ("comp_knee", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: comp_knee must be 0 (soft) or 1 (hard), got {v}");
                }
                self.set_field(strip, "comp_knee", ParamValue::Float(v)).await
            }
            // Gate attack uses the canonical alias path below (gate_attack
            // in the native table); modes retain their explicit domain guards.
            ("gate_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: gate_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "gate_mode", ParamValue::Float(v)).await
            }
            ("eq_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: eq_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "eq_mode", ParamValue::Float(v)).await
            }
            ("eq1_type" | "eq2_type" | "eq3_type" | "eq4_type", ParamValue::Float(v)) => {
                if !(0.0..=5.0).contains(&v) {
                    anyhow::bail!("dl32s: {param} must be 0..=5, got {v}");
                }
                self.set_field(strip, param, ParamValue::Float(v)).await
            }
            (key, ParamValue::Float(v)) => {
                if let Some(alias) = alias_target(key) {
                    return self.set_field(strip, alias, ParamValue::Float(v)).await;
                }
                // Generic send slots ride the aux/FX leaves; send pans are
                // canonical -100..+100 vs wire -1.0..+1.0.
                if let Some(mapped) = send_table_key(key) {
                    let w = if mapped.ends_with("_pan") { v / 100.0 } else { v };
                    return self.set_field(strip, &mapped, ParamValue::Float(w)).await;
                }
                self.set_field(strip, key, ParamValue::Float(v)).await
            }
            (key, v) => {
                if let Some(alias) = alias_target(key) {
                    return self.set_field(strip, alias, v).await;
                }
                // Generic send slots ride the aux/FX leaves; anything else
                // falls through to the raw table key (experts only).
                let owned;
                let table_key = match send_table_key(key) {
                    Some(mapped) => {
                        owned = mapped;
                        &owned
                    }
                    None => key,
                };
                self.set_field(strip, table_key, v).await
            }
        }
    }

    /// Authoritative param read (verification/poll/refresh path): desk-proven
    /// only — own-write-pending words withheld. Deliberately NOT routed via
    /// the inclusive display getters above (continuity without proof); each
    /// arm reads the verified cache directly, including unit conversion.
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => {
                let ch = Self::channel(strip)?;
                let addr = dl32s_inputs().address(ch, "fader")?;
                self.verified_fader(addr).await.map(ParamValue::Float)
            }
            "mute" => {
                let ch = Self::channel(strip)?;
                let addr = dl32s_inputs().address(ch, "mute")?;
                self.verified_mute(addr).await.map(ParamValue::Bool)
            }
            "name" => {
                let ch = Self::channel(strip)?;
                self.verified_name(ch).await.map(ParamValue::Text)
            }
            // Wire pan -1.0..+1.0 back out to canonical -100..+100.
            "pan" => match self.get_field(strip, "pan").await? {
                ParamValue::Float(v) => Ok(ParamValue::Float(v * 100.0)),
                other => Ok(other),
            },
            key => {
                // Renames, then send slots, then the raw table key.
                if let Some(alias) = alias_target(key) {
                    return self.get_field(strip, alias).await;
                }
                if let Some(mapped) = send_table_key(key) {
                    let pv = self.get_field(strip, &mapped).await?;
                    if mapped.ends_with("_pan") {
                        return match pv {
                            ParamValue::Float(v) => Ok(ParamValue::Float(v * 100.0)),
                            other => Ok(other),
                        };
                    }
                    return Ok(pv);
                }
                self.get_field(strip, key).await
            }
        }
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let table = dl32s_inputs();
        let mut strips = Vec::with_capacity(INPUT_COUNT as usize);
        for i in 0..INPUT_COUNT {
            let ch = i + 1;
            let fader_addr = table.address(ch, "fader").expect("table static");
            let mute_addr = table.address(ch, "mute").expect("table static");
            let pan_addr = table.address(ch, "pan").expect("table static");
            // No fabricated defaults: a missing word means the bulk push
            // hasn't covered this strip yet, and reporting 0/unmuted as
            // real would paint the wrong desk state. Surface the read
            // error so callers hold their loading state instead.
            // Continuity, not proof: this reads the optimistic-inclusive
            // `cached` (own-write-pending words included) so a snapshot
            // right after local moves still reflects them. Provenance for
            // per-param confirmation lives one layer up (engine `authority`
            // on `param`/`params`); the typed `get_*` display getters above
            // are likewise inclusive, while `get_param` stays verified-only.
            let fader = session
                .cached(fader_addr)
                .map(|raw| addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
                .ok_or_else(|| anyhow::anyhow!("snapshot: ch{ch} fader: no value pushed yet"))?;
            let mute = session
                .cached(mute_addr)
                .map(|v| v != 0)
                .ok_or_else(|| anyhow::anyhow!("snapshot: ch{ch} mute: no value pushed yet"))?;
            let pan = session
                .cached(pan_addr)
                .map(|raw| f32::from_bits(raw) * 100.0)
                .ok_or_else(|| anyhow::anyhow!("snapshot: ch{ch} pan: no value pushed yet"))?;
            let name = session
                .name(ch)
                .unwrap_or_else(|| format!("CH{ch:02}"));
            strips.push(ChannelStrip {
                id: StripId(i as u16),
                name,
                fader,
                mute,
                pan,
            });
        }
        Ok(MixerSnapshot { strips })
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        for strip in &snapshot.strips {
            // Names are deliberately not written back: the rename encoding is
            // unconfirmed and a wrong count word blanks neighbour slots.
            self.set_fader(strip.id, strip.fader).await?;
            self.set_mute(strip.id, strip.mute).await?;
        }
        Ok(())
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        let addr = self.addr.lock().await.clone().unwrap_or_default();
        Ok(MixerInfo {
            name: String::new(),
            model: "DL32S".into(),
            // GENERAL_INFO model/firmware strings were never decoded from the
            // wire; report the driver identity rather than guessing.
            firmware: "unparsed".into(),
            addr,
        })
    }

    /// Only the verified pre-fader input tap: the first 32 meter words are
    /// hardware-pinned input dB (see [`decode_input_meters`]). No `post`
    /// (never `in` x fader — unconfirmed), no GR, no RTA.
    fn meter_taps(&self) -> &'static [&'static str] {
        &["in"]
    }

    /// Read-only native snapshot view: the push-observed model above
    /// (`Some` whenever this driver is selected — supported but possibly
    /// unconnected/loading — so the engine can render honest status
    /// instead of guessing). A mismatched private generation withholds
    /// the view (unavailable default) so a newer session never reads the
    /// older session's slots. No write path exists behind this hook.
    fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        let current = self.generation.load(Ordering::Acquire);
        self.native.lock().ok().map(|v| {
            if v.generation != current {
                return openfader_core::NativeSnapshots::default();
            }
            v.view.clone()
        })
    }

    /// Non-blocking input-meter sample: the cached linear levels (one per
    /// input strip), or empty when no fresh push backs this generation —
    /// empty means unavailable, never a fabricated silence row. The engine
    /// skips both the `meters` and the `in` tap frames on empty, so a dead
    /// stream goes quiet instead of frozen. No polling here: pushes arrive
    /// on the existing broadcast feed, and the shell's 80 ms ticker stays
    /// the only meter clock.
    fn poll_meters(&self) -> MeterFrame {
        let current = self.generation.load(Ordering::Acquire);
        let fresh = self.meters.lock().ok().and_then(|m| {
            if m.generation != current || m.levels.len() != INPUT_COUNT as usize {
                return None;
            }
            let at = m.updated_at?;
            if at.elapsed() > METER_STALE_AFTER {
                return None;
            }
            Some(m.levels.clone())
        });
        match fresh {
            Some(levels) => MeterFrame { levels },
            None => MeterFrame { levels: Vec::new() },
        }
    }

    /// Passive shared RTA (Return 1–2): the desk's MF-selected source,
    /// followed read-only — no acquisition, no restore writes. Returns the
    /// global (`strip: None`) frame for the confirmed selector 5000
    /// (`DL32S Return 1–2 (shared)` / `shared`); the verified 4006 selector
    /// attributes to [`StripId`]`(6)` (`DL32S CH7 (shared)` / `shared`).
    /// Unknown selectors (and the unsettled/stale windows) report empty —
    /// unavailable, never fabricated. Source changes clear the cache, bump
    /// the epoch (monotonic across source/generation changes) and withhold
    /// frames for [`RTA_SETTLE`] so in-flight old-source words are never
    /// misattributed. Stale (>1 s, same bound as input meters) reads
    /// empty. Band edges are nominal (see [`rta_band_edges_hz`]). No
    /// Return-strip id or param is invented here — return processing stays
    /// a later slice.
    fn poll_spectrum(&self) -> Vec<AudioSpectrum> {
        let current = self.generation.load(Ordering::Acquire);
        let (selector, epoch, bins_db) = match self.spectrum.lock() {
            Ok(s) => {
                if s.generation != current {
                    return Vec::new();
                }
                match s.selector {
                    Some(v) if v == RTA_SOURCE_RETURN_12 || v == RTA_SOURCE_CH7 => {}
                    _ => return Vec::new(),
                }
                if s.switched_at.is_some_and(|t| t.elapsed() < RTA_SETTLE) {
                    return Vec::new();
                }
                let at = match s.updated_at {
                    Some(t) => t,
                    None => return Vec::new(),
                };
                if at.elapsed() > METER_STALE_AFTER {
                    return Vec::new();
                }
                if s.bins_db.len() != RTA_BIN_COUNT {
                    return Vec::new();
                }
                (s.selector, s.epoch, s.bins_db.clone())
            }
            Err(_) => return Vec::new(),
        };
        let (strip, source, position) = match selector {
            Some(v) if v == RTA_SOURCE_RETURN_12 => (
                None,
                "DL32S Return 1–2 (shared)".to_string(),
                "shared".to_string(),
            ),
            Some(v) if v == RTA_SOURCE_CH7 => (
                Some(StripId(6)),
                "DL32S CH7 (shared)".to_string(),
                "shared".to_string(),
            ),
            _ => return Vec::new(),
        };
        vec![AudioSpectrum {
            strip,
            bins_db,
            band_edges_hz: rta_band_edges_hz(),
            source,
            position,
            epoch,
        }]
    }

    /// Passive shared RTA source: release (`None`) is a no-op Ok — there is
    /// nothing owned to restore. A named acquire errors explicitly (no
    /// default success pretence): the source is MF-selected and this stage
    /// performs no selector writes. The frontend disables automatic DL32S
    /// target requests.
    async fn set_spectrum_target(
        &self,
        _owner: &str,
        strip: Option<StripId>,
    ) -> anyhow::Result<()> {
        if strip.is_none() {
            return Ok(());
        }
        anyhow::bail!(
            "dl32s: RTA source is MF-selected (shared Return 1–2); automatic acquisition is disabled"
        )
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        if let Some(session) = self.session.lock().await.take() {
            session.close();
        }
        *self.addr.lock().await = None;
        if let Ok(mut m) = self.meters.lock() {
            m.updated_at = None;
            m.levels.clear();
        }
        if let Ok(mut s) = self.spectrum.lock() {
            s.updated_at = None;
            s.bins_db.clear();
        }
        // The last desk must not survive disconnect: the private
        // generation tag and the view reset together (the engine emits
        // `unavailable`, never stale slots; the getter would withhold a
        // mismatched generation either way).
        if let Ok(mut n) = self.native.lock() {
            *n = NativeCache::default();
        }
        Ok(())
    }
    // Buses / main / meter subscribe keep the core defaults (unsupported):
    // output-strip fields are structurally mapped but deliberately unlabelled
    // per SPEC.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn not_connected_errors() {
        let d = Dl32sDriver::new();
        assert_eq!(d.name(), "dl32s");
        assert!(d.get_fader(StripId(0)).await.is_err());
        assert!(d.get_mute(StripId(0)).await.is_err());
        assert!(d.get_name(StripId(0)).await.is_err());
        assert!(d.get_mute_group(1).await.is_err());
        assert!(d.set_fader(StripId(0), 0.5).await.is_err());
    }

    #[test]
    fn strip_index_is_zero_based() {
        assert_eq!(Dl32sDriver::channel(StripId(0)).unwrap(), 1);
        assert_eq!(Dl32sDriver::channel(StripId(31)).unwrap(), 32);
        assert!(Dl32sDriver::channel(StripId(32)).is_err());
    }

    #[tokio::test]
    async fn unknown_field_errors_without_network() {
        let d = Dl32sDriver::new();
        // Field resolution happens before the connection check, so mapping
        // mistakes surface even offline.
        let err = d
            .set_param(StripId(0), "nope", ParamValue::Float(1.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown"), "{err}");
        let err = d.get_param(StripId(0), "nope").await.unwrap_err().to_string();
        assert!(err.contains("unknown"), "{err}");
    }

    #[tokio::test]
    async fn wrong_value_kind_errors() {
        let d = Dl32sDriver::new();
        let err = d
            .set_field(StripId(0), "gain", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Db"), "{err}");
    }

    #[test]
    fn capabilities_are_wave1_verified_set() {
        let caps = Dl32sDriver::new().capabilities();
        for id in [
            "fader", "mute", "name", "pan", "gain", "phantom", "trim", "polarity",
            "hpf_on", "hpf_freq", "lr_assign", "link", "color",
            "gate_on", "gate_thr", "gate_att", "gate_mode", "gate_rel",
            "gate_range", "gate_hold",
            "comp_on", "comp_mode", "comp_thr", "comp_ratio", "comp_knee",
            "comp_att", "comp_rel", "comp_makeup", "eq_on", "eq_mode",
            "eq1_gain", "eq1_freq", "eq1_q", "eq1_type",
            "eq2_gain", "eq2_freq", "eq2_q", "eq2_type",
            "eq3_gain", "eq3_freq", "eq3_q", "eq3_type",
            "eq4_gain", "eq4_freq", "eq4_q", "eq4_type",
            "aux1_level", "aux1_mute", "aux8_level", "aux8_mute",
            "fx1_level", "fx1_mute", "fx4_level", "fx4_mute",
            "send1_level", "send1_pan", "send8_level", "send8_pan",
            "send9_level", "send12_level",
        ] {
            assert!(caps.contains(&id), "caps miss {id}");
        }
        for id in [
            "icon", // live domain is ~1000+, unmapped
            "comp_auto",
            "comp_det",
            "comp_env",
            "comp_mix", // no leaf
            "gate_key_src",
            "gate_filter_on",
            "comp_key_src", // no leaf
            "eq_bands",
            "hpf_slope",
            "aux1_pan",
            "send1_tap",
            "send1_grpon",
            "send1_on",
            "send9_pan",
            "send13_level",
            "send13_pan",
            "solo",
        ] {
            assert!(!caps.contains(&id), "caps must not advertise {id}");
        }
        // Every advertised id resolves in the canonical catalogue or the
        // driver's provider rows, so the engine renders a params row for it.
        let defs = Dl32sDriver::new().param_defs();
        for id in caps {
            let known = openfader_core::params::find(id).is_some()
                || defs.iter().any(|d| d.id == *id);
            assert!(known, "caps id {id} resolves nowhere");
        }
    }

    #[test]
    fn send_slot_targets() {
        assert_eq!(send_table_key("send1_level").as_deref(), Some("aux1_level"));
        assert_eq!(send_table_key("send1_pan").as_deref(), Some("aux1_pan"));
        assert_eq!(send_table_key("send8_level").as_deref(), Some("aux8_level"));
        assert_eq!(send_table_key("send8_pan").as_deref(), Some("aux8_pan"));
        assert_eq!(send_table_key("send9_level").as_deref(), Some("fx1_level"));
        assert_eq!(send_table_key("send9_pan"), None);
        assert_eq!(send_table_key("send12_level").as_deref(), Some("fx4_level"));
        assert_eq!(send_table_key("send13_level"), None);
        assert_eq!(send_table_key("send0_level"), None);
        assert_eq!(send_table_key("send1_tap"), None);
        assert_eq!(send_table_key("send1_grpon"), None);
        assert_eq!(send_table_key("send1_on"), None);
        assert_eq!(send_table_key("aux1_level"), None);
        assert_eq!(send_table_key("pan"), None);
    }

    #[test]
    fn send_slot_reverse_targets() {
        assert_eq!(send_slot_for_table_key("aux1_level"), Some((1, "level")));
        assert_eq!(send_slot_for_table_key("aux1_pan"), Some((1, "pan")));
        assert_eq!(send_slot_for_table_key("aux8_level"), Some((8, "level")));
        assert_eq!(send_slot_for_table_key("aux8_pan"), Some((8, "pan")));
        assert_eq!(send_slot_for_table_key("fx1_level"), Some((9, "level")));
        assert_eq!(send_slot_for_table_key("fx4_level"), Some((12, "level")));
        assert_eq!(send_slot_for_table_key("fx1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux9_level"), None);
        assert_eq!(send_slot_for_table_key("fx5_level"), None);
        assert_eq!(send_slot_for_table_key("pan"), None);
    }

    #[tokio::test]
    async fn unknown_send_slots_error_offline() {
        let d = Dl32sDriver::new();
        // Mapping/slot resolution happens before the connection check, so
        // these surface even offline.
        for id in ["send13_level", "send1_tap", "send9_pan", "nope"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(1.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
            let err = d
                .get_param(StripId(0), id)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
        }
    }

    #[tokio::test]
    async fn wrong_value_kind_errors_offline() {
        let d = Dl32sDriver::new();
        // Canonical pan takes a float; a bool must fail with an encoding
        // error, not a connection error.
        let err = d
            .set_param(StripId(0), "pan", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Float"), "{err}");
    }

    #[tokio::test]
    async fn out_of_domain_modes_rejected_offline() {
        let d = Dl32sDriver::new();
        // Binary-confirmed modes: only 0/1 reach the wire.
        for id in ["gate_mode", "comp_mode", "comp_knee", "eq_mode"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(2.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("must be 0"), "{id}: {err}");
        }
        // EQ types: 0..=5 accepted (raw 2/3/4 meanings verified, 0/1/5 relay exact).
        let err = d
            .set_param(StripId(0), "eq2_type", ParamValue::Float(6.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("0..=5"), "{err}");
    }

    #[test]
    fn decode_push_value_mirrors_get_param() {
        // ch1 fader (addr 9) -15 dB -> canonical level 0.5.
        let got = decode_push_value(9, (-15.0f32).to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, StripId(0));
        assert_eq!(got[0].1, "fader");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
        // ch1 mute (addr 8) = 1.
        let got = decode_push_value(8, 1);
        assert_eq!(
            got,
            vec![(StripId(0), "mute".to_string(), ParamValue::Bool(true))]
        );
        // ch1 pan (addr 10) 0.5 wire -> +50 canonical.
        let got = decode_push_value(10, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Renames ride canonical ids.
        // ch11 (strip 10) main_assign = 1+10*106+10 = 1071.
        let got = decode_push_value(1071, 1);
        assert_eq!(
            got,
            vec![(StripId(10), "lr_assign".to_string(), ParamValue::Bool(true))]
        );
        let got = decode_push_value(1 + 3 * 106 + 27, (-20.0f32).to_bits());
        assert_eq!(got[0].1, "comp_thr");
        // Aux send levels ride twice (native + generic slot).
        let got = decode_push_value(51, (-10.0f32).to_bits());
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|(_, p, _)| p == "aux1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send1_level"));
        let got = decode_push_value(75, (-10.0f32).to_bits());
        assert!(got.iter().any(|(_, p, _)| p == "fx1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send9_level"));
        // Aux pan mirrors to the generic send slot with x100 units.
        let got = decode_push_value(53, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "send1_pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Write-proven leaves decode straight through.
        let got = decode_push_value(21, 50.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "gate_att");
        let got = decode_push_value(47, 2.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "eq1_type");
        // Unadvertised + out-of-range leaves decode to nothing.
        assert!(decode_push_value(1 + 4, 1005).is_empty()); // icon
        assert!(decode_push_value(4041, 1).is_empty()); // subgroup master: no strip
        assert!(decode_push_value(4347, 0).is_empty()); // main: no strip
        assert!(decode_push_value(99999, 0).is_empty()); // unknown space
    }

    #[test]
    fn decode_push_names_and_snapshot() {
        use openfader_transport::mackie::PushEvent;
        let got = decode_push(PushEvent::Names {
            pairs: vec![(1, "Vox".to_string()), (33, "Aux".to_string())],
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].strip, StripId(0));
        assert_eq!(got[0].param, "name");
        assert_eq!(got[0].value, ParamValue::Text("Vox".to_string()));
        // Snapshot relays decode to nothing (session cache records them;
        // the poller converges the wholesale change).
        assert!(decode_push(PushEvent::SnapshotRecall(7)).is_empty());
        assert!(decode_push(PushEvent::SnapshotRename {
            slot: 2,
            name: "Main".to_string()
        })
        .is_empty());
        // Non-param value types (meters, mute-group masters) are skipped.
        let got = decode_push(PushEvent::Values {
            value_type: 1,
            start: 114,
            raws: vec![1],
        });
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn broadcast_push_arrives_as_param_update_loopback() {
        use openfader_transport::mackie::{
            decode_one, word_swap, Command, Message, MessageType, RawValue,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn read_msg(
            stream: &mut tokio::net::TcpStream,
        ) -> anyhow::Result<Message> {
            let mut prefix = [0u8; 8];
            stream.read_exact(&mut prefix).await?;
            let chunks = u16::from_be_bytes([prefix[2], prefix[3]]) as usize;
            let mut rest = vec![0u8; chunks * 4 + if chunks == 0 { 0 } else { 4 }];
            if !rest.is_empty() {
                stream.read_exact(&mut rest).await?;
            }
            let mut buf = prefix.to_vec();
            buf.extend_from_slice(&rest);
            let (msg, _) =
                decode_one(&buf)?.ok_or_else(|| anyhow::anyhow!("truncated"))?;
            Ok(msg)
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fake = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut seq: u8 = 40;
            let mut cic = 0;
            loop {
                let req = match read_msg(&mut stream).await {
                    Ok(m) => m,
                    Err(_) => break,
                };
                let resp = Message::new(req.seq, MessageType::Response, req.command, vec![])
                    .unwrap();
                if stream.write_all(&resp.encode()).await.is_err() {
                    break;
                }
                if req.command == Command::ChannelInfoControl {
                    cic += 1;
                }
                if cic < 2 {
                    continue;
                }
                // Repeating (not one-shot) keeps this race-free — pushes
                // sent before the test subscribes are missed by broadcast
                // semantics, so a later repetition must follow.
                // Another client moved ch1 fader to -15 dB (addr 9).
                let body = openfader_transport::mackie::build_single_set(
                    9,
                    RawValue::Float(-15.0),
                    5,
                );
                seq = seq.wrapping_add(1);
                let push = Message::new(seq, MessageType::Broadcast, Command::ChannelValues, body)
                    .unwrap();
                if stream.write_all(&push.encode()).await.is_err() {
                    break;
                }
                let mut padded = b"Vox\x00".to_vec();
                while !padded.len().is_multiple_of(4) {
                    padded.push(0);
                }
                let mut nbody = vec![];
                nbody.extend_from_slice(&1u32.to_be_bytes());
                nbody.extend_from_slice(&0x00010000u32.to_be_bytes());
                nbody.extend_from_slice(&word_swap(&padded));
                seq = seq.wrapping_add(1);
                let names = Message::new(seq, MessageType::Request, Command::ChannelNames, nbody)
                    .unwrap();
                if stream.write_all(&names.encode()).await.is_err() {
                    break;
                }
                // Readiness sentinels: the driver's connect gate waits for
                // the bulk-state words a real desk's whole-space push
                // delivers (CH1 mute at 8, CH32 mute/fader at 3294/3295,
                // main fader at 4347) before reporting ready.
                for (addr, raw) in [
                    (8, RawValue::Int(0)),
                    (3294, RawValue::Int(0)),
                    (3295, RawValue::Float(-120.0)),
                    (4347, RawValue::Float(-15.0)),
                ] {
                    let body = openfader_transport::mackie::build_single_set(addr, raw, 5);
                    seq = seq.wrapping_add(1);
                    let push =
                        Message::new(seq, MessageType::Broadcast, Command::ChannelValues, body)
                            .unwrap();
                    if stream.write_all(&push.encode()).await.is_err() {
                        break;
                    }
                }
                // Pause before the next relay round (post-init the client
                // only sends no-reply keepalives, so nothing needs an
                // answer here; the write error above still exits promptly
                // on a dropped client).
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });

        let d = Dl32sDriver::new();
        d.connect(&addr.to_string()).await.unwrap();
        let mut rx = d.subscribe_updates().expect("driver has update channel");
        let mut got_fader = false;
        let mut got_name = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !(got_fader && got_name) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "push updates missing: fader={got_fader} name={got_name}"
            );
            let u = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .expect("update timeout")
                .expect("update channel closed");
            if u.strip == StripId(0) && u.param == "fader" {
                assert!(matches!(u.value, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
                got_fader = true;
            }
            if u.strip == StripId(0) && u.param == "name" {
                assert_eq!(u.value, ParamValue::Text("Vox".to_string()));
                got_name = true;
            }
        }
        assert!(matches!(
            d.get_param(StripId(0), "fader").await.unwrap(),
            ParamValue::Float(v) if (v - 0.5).abs() < 1e-6
        ));
        assert_eq!(
            d.get_param(StripId(0), "name").await.unwrap(),
            ParamValue::Text("Vox".to_string())
        );
        fake.abort();
    }
}
