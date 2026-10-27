//! X-Air channel pan + sends (`/ch/NN/mix/pan`, `/ch/NN/mix/MM/level`).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`,
//!   extracted from Behringer-provided firmware):
//!   `mix/ pan [0.0,1.0] lin(101) // -100..100` (bal on lr) and
//!   `mix/ 01..n/ level [0.0,1.0] level(161) // -oo..10` with `10` on ch —
//!   ten send slots per channel. The send block lists `level`, `grpon`,
//!   `tap` and (odd slots only) `pan`: there is NO send-mute leaf, so
//!   `auxN_mute`/`fxN_mute` stay unsupported on X-Air (this agrees with the
//!   owner-rig finding that `/ch/02/mix/01/on` is silent, baseline T7).
//! - xair-api-python `shared.py` (MIT, onyx-and-iris, dev branch, fetched
//!   2026-10-18): `Send` at `{root}/mix/{MM}` (1-based, zero-padded
//!   two-digit) with `level` mapped through the X-Air fader law
//!   (`db_from`/`db_to`), built for `range(num_bus + num_fx)` sends — on
//!   the XR18 (`kinds.py`: `num_bus = 6`, `num_fx = 4`) that is slots
//!   `01..=10`, buses first, then FX. So `aux1..6` → `01..06` and
//!   `fx1..4` → `07..10`; `aux7/8` have no wire slot and stay unsupported.
//! - QLC+ forum X-Air thread (community, cross-check): "The Xair has 10
//!   mix busses available, 1-6 are busses …, mix 7-10 are the mono sends
//!   for the fx buses" — same 6+4 split as xair-api.
//! - protocol reference X32 OSC protocol documentation (`/tmp/x32.pdf`):
//!   backs only the `/ch/NN/mix/pan` node (`linf [-100, 100]`) and the
//!   send `level` fader law (`level [0.0..1.0 (+10dB)]`) — the X32 leaf
//!   names differ elsewhere, so slot shapes come from the X-Air sources.
//!
//! Owner-rig verified 2026-10-18 (XR18 FW 1.20): bus-send slots read tap
//! `2` (POSTEQ), FX-send slots `4` (POST); the six-point tap enum above is
//! the desk truth. Live-silent (no desk leaf, out of caps with the driver
//! arm retained for loopback only): per-send on/off (`/ch/NN/mix/MM/on`,
//! baseline T7).

use openfader_transport::osc;
use rosc::OscType;

/// Pan bounds in the relay's engineering units (firmware map `-100..100`).
pub const PAN_MIN: f32 = -100.0;
pub const PAN_MAX: f32 = 100.0;

/// Bus sends per channel (firmware `01..n` with `10` on ch; xair-api
/// `num_bus = 6` on XR18). `aux1..6` map to wire slots `01..06`.
pub const AUX_SENDS: u8 = 6;
/// FX sends per channel (xair-api `num_fx = 4` on XR18). `fx1..4` map to
/// wire slots `07..=10`.
pub const FX_SENDS: u8 = 4;

/// Send-level bounds in dB: the X-Air `level(161)` law bottoms at the fader
/// floor (`-90`, read as `-inf`) and tops at `+10`.
pub const SEND_MIN_DB: f32 = -90.0;
/// Send-level upper bound in dB (see [`SEND_MIN_DB`]).
pub const SEND_MAX_DB: f32 = 10.0;

/// ```
/// # use openfader_proto_xair::mix::mix_pan;
/// assert_eq!(mix_pan(1), "/ch/01/mix/pan");
/// assert_eq!(mix_pan(16), "/ch/16/mix/pan");
/// ```
pub fn mix_pan(ch: u16) -> String {
    format!("/ch/{ch:02}/mix/pan")
}

/// Slot `1..=6` are bus sends, `7..=10` are FX sends.
///
/// ```
/// # use openfader_proto_xair::mix::send_level;
/// assert_eq!(send_level(1, 1).unwrap(), "/ch/01/mix/01/level");
/// assert_eq!(send_level(16, 10).unwrap(), "/ch/16/mix/10/level");
/// ```
pub fn send_level(ch: u16, send: u8) -> anyhow::Result<String> {
    if !(1..=AUX_SENDS + FX_SENDS).contains(&send) {
        anyhow::bail!("send slot {send} out of range 1..={}", AUX_SENDS + FX_SENDS);
    }
    Ok(format!("/ch/{ch:02}/mix/{send:02}/level"))
}

pub fn aux_slot(n: u8) -> anyhow::Result<u8> {
    if !(1..=AUX_SENDS).contains(&n) {
        anyhow::bail!("aux send {n} out of range 1..={AUX_SENDS} (only 6 bus sends on X-Air)");
    }
    Ok(n)
}

pub fn fx_slot(n: u8) -> anyhow::Result<u8> {
    if !(1..=FX_SENDS).contains(&n) {
        anyhow::bail!("fx send {n} out of range 1..={FX_SENDS}");
    }
    Ok(AUX_SENDS + n)
}

/// Firmware `lin(101)`: engineering `-100..=+100` to wire `0..1`.
pub fn pan_to_wire(pan: f32) -> anyhow::Result<f32> {
    if !pan.is_finite() {
        anyhow::bail!("pan must be finite, got {pan}");
    }
    if !(PAN_MIN..=PAN_MAX).contains(&pan) {
        anyhow::bail!("pan {pan} out of range {PAN_MIN}..={PAN_MAX}");
    }
    Ok((pan - PAN_MIN) / (PAN_MAX - PAN_MIN))
}

/// Firmware `lin(101)` (decode direction).
pub fn wire_to_pan(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("pan wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("pan wire value {v} outside 0..=1");
    }
    Ok(PAN_MIN + (PAN_MAX - PAN_MIN) * v)
}

/// X-Air fader law (joints at `-60/-30/-10/0`, floor `-90` = `-inf`).
pub fn send_db_to_wire(db: f32) -> anyhow::Result<f32> {
    if !db.is_finite() {
        anyhow::bail!("send level must be finite, got {db}");
    }
    if !(SEND_MIN_DB..=SEND_MAX_DB).contains(&db) {
        anyhow::bail!("send level {db} dB out of range {SEND_MIN_DB}..={SEND_MAX_DB}");
    }
    Ok(if db >= 10.0 {
        1.0
    } else if db >= -10.0 {
        (db + 30.0) / 40.0
    } else if db >= -30.0 {
        (db + 50.0) / 80.0
    } else if db >= -60.0 {
        (db + 70.0) / 160.0
    } else {
        (db + 90.0) / 480.0
    })
}

/// X-Air fader law (decode direction); wire `0` reads `-90`, the `-inf` floor.
pub fn wire_to_send_db(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("send level wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("send level wire value {v} outside 0..=1");
    }
    Ok(if v >= 1.0 {
        10.0
    } else if v >= 0.5 {
        40.0 * v - 30.0
    } else if v >= 0.25 {
        80.0 * v - 50.0
    } else if v >= 0.0625 {
        160.0 * v - 70.0
    } else if v > 0.0 {
        480.0 * v - 90.0
    } else {
        -90.0
    })
}

pub fn send_level_value(msg: &rosc::OscMessage) -> anyhow::Result<f32> {
    osc::fader_value(msg)
}

/// OSC address of one channel send tap (`mix/MM/tap`, int `0..=5`).
///
/// Doc sources: the XR18 firmware OSC map send block lists
/// `tap [0,5] // IN, PREEQ, POSTEQ, PRE, POST, GRP` alongside `level`.
/// Owner-rig verified 2026-10-18 (XR18 FW 1.20): bus-send slots read back
/// `2` (POSTEQ), FX-send slots `4` (POST) — a `0..=1` pre/post codec is
/// wrong and rejects live values, so the full six-point enum is encoded.
///
/// ```
/// # use openfader_proto_xair::mix::send_tap;
/// assert_eq!(send_tap("/ch/01", 1).unwrap(), "/ch/01/mix/01/tap");
/// assert_eq!(send_tap("/ch/16", 10).unwrap(), "/ch/16/mix/10/tap");
/// ```
pub fn send_tap(root: &str, send: u8) -> anyhow::Result<String> {
    if !(1..=AUX_SENDS + FX_SENDS).contains(&send) {
        anyhow::bail!("send slot {send} out of range 1..={}", AUX_SENDS + FX_SENDS);
    }
    Ok(format!("{root}/mix/{send:02}/tap"))
}

/// OSC address of one channel send pan (`mix/MM/pan`, float `0..1`).
///
/// Only bus-pair slots 1, 3 and 5 have a pan leaf. Even bus slots and
/// FX-send slots are silent on XR18 FW 1.20 (rig-verified 2026-10-20).
///
/// ```
/// # use openfader_proto_xair::mix::send_pan;
/// assert_eq!(send_pan("/ch/01", 1).unwrap(), "/ch/01/mix/01/pan");
/// ```
pub fn send_pan(root: &str, send: u8) -> anyhow::Result<String> {
    if !matches!(send, 1 | 3 | 5) {
        anyhow::bail!("send pan not supported for slot {send}; expected 1, 3 or 5");
    }
    Ok(format!("{root}/mix/{send:02}/pan"))
}

/// OSC address of one strip LR assign (`mix/lr`, int `1`/`0`).
///
/// Doc sources: the XR18 firmware OSC map send block lists `lr` alongside
/// `level`/`grpon`/`tap`/`pan`; the community X-Air OSC table gives int
/// `0..=1` (OFF/ON) for `/ch/01/mix/lr` and `/bus/1/mix/lr`. Owner-rig
/// verified 2026-10-19 (XR18 FW 1.20): `/ch/01/mix/lr` → 1, `/bus/1/mix/lr`
/// → 0, `/rtn/1/mix/lr` → 1. Main LR (`/lr/mix`) carries NO `lr` field —
/// never address this builder with the `/lr` root.
///
/// ```
/// # use openfader_proto_xair::mix::mix_lr;
/// assert_eq!(mix_lr("/ch/01"), "/ch/01/mix/lr");
/// assert_eq!(mix_lr("/bus/1"), "/bus/1/mix/lr");
/// ```
pub fn mix_lr(root: &str) -> String {
    format!("{root}/mix/lr")
}

/// OSC address of one channel send group-on (`mix/MM/grpon`, int `1`/`0`).
///
/// Doc sources: the XR18 firmware OSC map send block lists `grpon`
/// alongside `level`/`tap`/`pan`; the community X-Air OSC table gives int
/// `0..=1` (OFF/ON). Owner-rig verified 2026-10-19 (XR18 FW 1.20): all
/// `/ch/01/mix/MM/grpon` slots read `0`.
///
/// ```
/// # use openfader_proto_xair::mix::send_grpon;
/// assert_eq!(send_grpon("/ch/01", 1).unwrap(), "/ch/01/mix/01/grpon");
/// assert_eq!(send_grpon("/bus/1", 10).unwrap(), "/bus/1/mix/10/grpon");
/// ```
pub fn send_grpon(root: &str, send: u8) -> anyhow::Result<String> {
    if !(1..=AUX_SENDS + FX_SENDS).contains(&send) {
        anyhow::bail!("send slot {send} out of range 1..={}", AUX_SENDS + FX_SENDS);
    }
    Ok(format!("{root}/mix/{send:02}/grpon"))
}

/// OSC address of one channel send on/off (`mix/MM/on`, int `1`/`0`).
///
/// NO desk leaf: the firmware send block lists `level`, `grpon`, `tap` and
/// `pan` only, and `/ch/NN/mix/MM/on` is live-silent on XR18 FW 1.20
/// (baseline T7). Out of caps; the arm is retained (loopback-tested) so
/// direct misuse errors locally instead of timing out on hardware.
///
/// ```
/// # use openfader_proto_xair::mix::send_on;
/// assert_eq!(send_on("/ch/01", 1).unwrap(), "/ch/01/mix/01/on");
/// ```
pub fn send_on(root: &str, send: u8) -> anyhow::Result<String> {
    if !(1..=AUX_SENDS + FX_SENDS).contains(&send) {
        anyhow::bail!("send slot {send} out of range 1..={}", AUX_SENDS + FX_SENDS);
    }
    Ok(format!("{root}/mix/{send:02}/on"))
}

/// Send-tap point bounds (firmware map `tap [0,5]`):
/// `0` = IN, `1` = PREEQ, `2` = POSTEQ, `3` = PRE, `4` = POST, `5` = GRP.
/// Live defaults on the owner rig (XR18 FW 1.20): bus sends `2`, FX sends
/// `4`.
pub const TAP_MIN: i32 = 0;
pub const TAP_MAX: i32 = 5;

pub fn tap_to_arg(v: f32) -> anyhow::Result<OscType> {
    if !v.is_finite() || v.fract() != 0.0 {
        anyhow::bail!("send tap must be a whole number, got {v}");
    }
    let i = v as i32;
    if !(TAP_MIN..=TAP_MAX).contains(&i) {
        anyhow::bail!("send tap {i} out of range {TAP_MIN}..={TAP_MAX} (0 = IN … 5 = GRP)");
    }
    Ok(OscType::Int(i))
}

pub fn tap_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => {
            if !(TAP_MIN..=TAP_MAX).contains(i) {
                anyhow::bail!("{}: send tap {i} outside {TAP_MIN}..={TAP_MAX}", msg.addr);
            }
            Ok(*i)
        }
        other => anyhow::bail!("{}: send tap needs an int arg, got {other:?}", msg.addr),
    }
}

/// Sends 1..6 are the bus sends, 7..10 the FX sends (same slots as
/// `aux1..6` / `fx1..4`). Shared bool codec: [`crate::codec::bool_arg`]
/// encode, [`openfader_transport::osc::is_on`] decode.
pub fn generic_send_slot(n: u8) -> anyhow::Result<u8> {
    if !(1..=AUX_SENDS + FX_SENDS).contains(&n) {
        anyhow::bail!(
            "send {n} out of range 1..={} (X-Air has 6 bus + 4 FX sends)",
            AUX_SENDS + FX_SENDS
        );
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(mix_pan(1), "/ch/01/mix/pan");
        assert_eq!(mix_lr("/ch/01"), "/ch/01/mix/lr");
        assert_eq!(mix_lr("/bus/1"), "/bus/1/mix/lr");
        assert_eq!(mix_lr("/rtn/1"), "/rtn/1/mix/lr");
        assert_eq!(send_grpon("/ch/01", 1).unwrap(), "/ch/01/mix/01/grpon");
        assert_eq!(send_grpon("/ch/01", 10).unwrap(), "/ch/01/mix/10/grpon");
        assert!(send_grpon("/ch/01", 0).is_err());
        assert!(send_grpon("/ch/01", 11).is_err());
        assert_eq!(send_level(1, 1).unwrap(), "/ch/01/mix/01/level");
        assert_eq!(send_level(1, 6).unwrap(), "/ch/01/mix/06/level");
        assert_eq!(send_level(16, 7).unwrap(), "/ch/16/mix/07/level");
        assert_eq!(send_level(16, 10).unwrap(), "/ch/16/mix/10/level");
        assert!(send_level(1, 0).is_err());
        assert!(send_level(1, 11).is_err());
        assert_eq!(aux_slot(6).unwrap(), 6);
        assert!(aux_slot(7).is_err());
        assert_eq!(fx_slot(1).unwrap(), 7);
        assert_eq!(fx_slot(4).unwrap(), 10);
        assert!(fx_slot(5).is_err());
    }

    #[test]
    fn pan_range_matches_firmware_map() {
        assert_eq!((PAN_MIN, PAN_MAX), (-100.0, 100.0));
        assert_eq!(pan_to_wire(-100.0).unwrap(), 0.0);
        assert_eq!(pan_to_wire(100.0).unwrap(), 1.0);
        assert_eq!(pan_to_wire(0.0).unwrap(), 0.5);
        assert_eq!(wire_to_pan(0.0).unwrap(), -100.0);
        assert_eq!(wire_to_pan(1.0).unwrap(), 100.0);
        assert_eq!(wire_to_pan(0.5).unwrap(), 0.0);
        assert!(pan_to_wire(100.1).is_err());
        assert!(wire_to_pan(1.1).is_err());
    }

    #[test]
    fn send_law_matches_fader_joints() {
        // Joints shared with the X-Air fader law (SPEC §4): the law is
        // continuous, 0.75 wire = 0 dB, floor wire 0 = -90 dB (-inf).
        assert_eq!(send_db_to_wire(10.0).unwrap(), 1.0);
        assert_eq!(send_db_to_wire(-90.0).unwrap(), 0.0);
        assert_eq!(wire_to_send_db(1.0).unwrap(), 10.0);
        assert_eq!(wire_to_send_db(0.0).unwrap(), -90.0);
        assert!((wire_to_send_db(0.75).unwrap() - 0.0).abs() < 1e-5);
        assert!((wire_to_send_db(0.5).unwrap() + 10.0).abs() < 1e-5);
        assert!((wire_to_send_db(0.25).unwrap() + 30.0).abs() < 1e-5);
        assert!((wire_to_send_db(0.0625).unwrap() + 60.0).abs() < 1e-5);
        for db in [-90.0, -60.0, -30.0, -10.0, 0.0, 5.0, 10.0] {
            let back = wire_to_send_db(send_db_to_wire(db).unwrap()).unwrap();
            assert!((back - db).abs() < 1e-4, "round-trip {db} -> {back}");
        }
        assert!(send_db_to_wire(10.1).is_err());
        assert!(send_db_to_wire(-90.1).is_err());
        assert!(wire_to_send_db(1.1).is_err());
    }
}
