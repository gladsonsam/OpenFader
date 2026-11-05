//! Engineering-unit conversions for X32 normalized wire floats.
//!
//! All float leaves travel as normalized `0.0..1.0` (Maillot v4.09-02
//! §"Float type"); the documented engineering ranges are linear
//! (`linf`), logarithmic (`logf`) or the 4-segment fader law (`level`).
//! Conversions here are exact inverses; desk-step quantization lives in
//! the `*_stepped` helpers below (documented quanta only, endpoints
//! exact) and in the wire-domain agreement check used at readback.
//!
//! Documented quantum sources (same doc, parameter tables + appendix):
//! pan [-100,100,2], trim [-18,18,.25], delay [0.3,500,.1],
//! gate thr [-80,0,.5], gate range [3,60,1] (58 values), gate attack
//! [0,120,1], gate/dyn hold [0.02,2000,101], gate/dyn release
//! [5,4000,101], dyn thr [-60,0,.5], knee [0,5,1], mgain [0,24,.5],
//! dyn attack [0,120,1], dyn mix [0,100,5], HPF [20,400,101],
//! filter/EQ frequency [20,20000,201], EQ gain [-15,15,.25],
//! Q reversed [10,0.3,72], automix weight [-12,12,.5], headamp gain
//! [-12,60,.5] (145 values), fader wire 1024 steps (appendix fader
//! floats), level wire 161 steps (appendix level table).
//! FX `/fx/N/par/MM` rows (pp88-118) list ranges with no step counts:
//! continuous, no desk quantum is assumed there.

/// Wire-domain agreement tolerance (f32 round-trip only, far below any
/// documented half-step; the smallest half-step owned here is the
/// 0.3..500 ms delay wire half-step ~1e-4).
pub const WIRE_EPS: f32 = 1e-5;
/// Fader wire quanta: appendix fader floats table, 1024 values.
pub const FADER_WIRE_STEPS: u32 = 1024;
/// Level wire quanta: appendix level table, 161 values.
pub const LEVEL_WIRE_STEPS: u32 = 161;
/// EQ Q quanta: appendix Q table, 72 values.
pub const Q_WIRE_STEPS: u32 = 72;

/// Linear map `[min, max] <-> [0, 1]`.
pub fn linf_to_wire(value: f32, min: f32, max: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        value.is_finite() && value >= min && value <= max,
        "x32: {value} out of range {min}..{max}"
    );
    Ok((value - min) / (max - min))
}

pub fn linf_from_wire(wire: f32, min: f32, max: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        wire.is_finite() && (0.0..=1.0).contains(&wire),
        "x32: wire value {wire} out of range"
    );
    Ok(min + wire * (max - min))
}

/// Logarithmic map `[min, max] <-> [0, 1]` (`logf`).
pub fn logf_to_wire(value: f32, min: f32, max: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        value.is_finite() && value >= min.min(max) && value <= min.max(max),
        "x32: {value} out of range {min}..{max}"
    );
    Ok((value / min).ln() / (max / min).ln())
}

pub fn logf_from_wire(wire: f32, min: f32, max: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        wire.is_finite() && (0.0..=1.0).contains(&wire),
        "x32: wire value {wire} out of range"
    );
    Ok(min * (max / min).powf(wire))
}

/// Reversed-log map for EQ Q (`10.0 -> 0.0`, `0.3 -> 1.0`, 72 steps).
pub fn q_to_wire(value: f32) -> anyhow::Result<f32> {
    logf_to_wire(value, 10.0, 0.3)
}

pub fn q_from_wire(wire: f32) -> anyhow::Result<f32> {
    logf_from_wire(wire, 10.0, 0.3)
}

/// X32 4-segment fader law, appendix p131 (`0.0..1.0 <-> -oo..+10 dB`).
/// `0.0` reads back as silence; dB values below -90 dB clamp to 0.
pub fn fader_db(wire: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        wire.is_finite() && (0.0..=1.0).contains(&wire),
        "x32: fader wire value {wire} out of range"
    );
    Ok(if wire >= 0.5 {
        wire * 40.0 - 30.0
    } else if wire >= 0.25 {
        wire * 80.0 - 50.0
    } else if wire >= 0.0625 {
        wire * 160.0 - 70.0
    } else if wire > 0.0 {
        wire * 480.0 - 90.0
    } else {
        f32::NEG_INFINITY
    })
}

pub fn db_to_fader(db: f32) -> anyhow::Result<f32> {
    if !db.is_finite() || db <= -90.0 {
        return Ok(0.0);
    }
    anyhow::ensure!(db <= 10.0, "x32: fader dB {db} out of range");
    Ok(if db < -60.0 {
        (db + 90.0) / 480.0
    } else if db < -30.0 {
        (db + 70.0) / 160.0
    } else if db < -10.0 {
        (db + 50.0) / 80.0
    } else {
        (db + 30.0) / 40.0
    })
}

/// Headamp gain `[-12, +60] dB in 0.5 dB steps <-> [0, 1]`.
/// Rounds to the desk step on write; reads return the exact desk value.
pub fn gain_to_wire(db: f32) -> anyhow::Result<f32> {
    let stepped = (db * 2.0).round() / 2.0;
    anyhow::ensure!(
        db.is_finite() && (-12.0..=60.0).contains(&db),
        "x32: headamp gain {db} out of range -12..60 dB"
    );
    Ok((stepped + 12.0) / 72.0)
}

pub fn gain_from_wire(wire: f32) -> anyhow::Result<f32> {
    linf_from_wire(wire, -12.0, 60.0)
}

/// Documented linear step for a `linf` engineering range, if the
/// parameter tables give one. Matched on the exact documented
/// `(min, max)` pair; unknown ranges yield `None` (never guessed).
pub fn linf_step(min: f32, max: f32) -> Option<f32> {
    if min == -100.0 && max == 100.0 {
        Some(2.0) // pan (ch/aux/fx/bus, sends)
    } else if min == -18.0 && max == 18.0 {
        Some(0.25) // trim
    } else if min == 0.3 && max == 500.0 {
        Some(0.1) // delay time
    } else if min == -80.0 && max == 0.0 {
        Some(0.5) // gate threshold
    } else if (min == 3.0 && max == 60.0) || (min == 0.0 && max == 120.0) {
        Some(1.0) // gate range (58 values), gate/dyn attack
    } else if min == -60.0 && max == 0.0 {
        Some(0.5) // dyn threshold
    } else if min == 0.0 && max == 5.0 {
        Some(1.0) // dyn knee
    } else if min == 0.0 && max == 24.0 {
        Some(0.5) // dyn makeup gain
    } else if min == 0.0 && max == 100.0 {
        Some(5.0) // dyn mix
    } else if min == -12.0 && max == 12.0 {
        Some(0.5) // automix weight
    } else if min == -15.0 && max == 15.0 {
        Some(0.25) // EQ gain
    } else {
        None
    }
}

/// Documented wire-step count for a `logf` engineering range, if the
/// parameter tables give one. Unknown ranges yield `None`.
pub fn logf_steps(min: f32, max: f32) -> Option<u32> {
    if (min == 20.0 && max == 400.0)
        || (min == 0.02 && max == 2000.0)
        || (min == 5.0 && max == 4000.0)
    {
        Some(101) // HPF, gate/dyn hold, gate/dyn release
    } else if min == 20.0 && max == 20000.0 {
        Some(201) // key filter / EQ frequency
    } else {
        None
    }
}

/// Round a normalized wire float to `steps` discrete desk values
/// (`steps >= 2`), snapping exact endpoints.
pub fn quantize_wire(wire: f32, steps: u32) -> f32 {
    debug_assert!(steps >= 2);
    if !wire.is_finite() {
        return wire;
    }
    if wire <= 0.0 {
        return 0.0;
    }
    if wire >= 1.0 {
        return 1.0;
    }
    let n = (steps - 1) as f32;
    ((wire * n).round() / n).clamp(0.0, 1.0)
}

/// Round an engineering value to the documented linear step,
/// snapping exact endpoints and clamping into range.
pub fn quantize_linf(value: f32, min: f32, max: f32, step: f32) -> f32 {
    if !value.is_finite() || step <= 0.0 {
        return value;
    }
    if value <= min {
        return min;
    }
    if value >= max {
        return max;
    }
    let q = ((value - min) / step).round() * step + min;
    // Snap values landing within float dust of an endpoint back to it.
    if (q - min).abs() <= step * 1e-4 {
        return min;
    }
    if (q - max).abs() <= step * 1e-4 {
        return max;
    }
    q.clamp(min, max)
}

/// `linf` engineering value rounded to its documented step, then mapped
/// to wire. Range-checked like [`linf_to_wire`].
pub fn linf_to_wire_stepped(value: f32, min: f32, max: f32, step: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        value.is_finite() && value >= min && value <= max,
        "x32: {value} out of range {min}..{max}"
    );
    let q = quantize_linf(value, min, max, step);
    Ok(((q - min) / (max - min)).clamp(0.0, 1.0))
}

/// `logf` engineering value mapped to wire, then rounded to `steps`
/// discrete desk values. Range-checked like [`logf_to_wire`].
pub fn logf_to_wire_stepped(value: f32, min: f32, max: f32, steps: u32) -> anyhow::Result<f32> {
    let wire = logf_to_wire(value, min, max)?;
    Ok(quantize_wire(wire, steps))
}

/// EQ Q engineering value rounded to the documented 72 reversed-log
/// desk values, mapped to wire. Endpoints exact.
pub fn q_to_wire_stepped(value: f32) -> anyhow::Result<f32> {
    let wire = q_to_wire(value)?;
    Ok(quantize_wire(wire, Q_WIRE_STEPS))
}

/// Send/mono level dB mapped through the preserved 4-segment law, then
/// rounded to the documented 161 wire values. `<= -90 dB` is the
/// exact wire-0 floor.
pub fn level_to_wire_stepped(db: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(db.is_finite(), "x32: send level {db} out of range");
    if db <= -90.0 {
        return Ok(0.0);
    }
    anyhow::ensure!(db <= 10.0, "x32: send level {db} out of range");
    Ok(quantize_wire(db_to_fader(db)?, LEVEL_WIRE_STEPS))
}

/// Fader wire value rounded to the documented 1024 desk values,
/// endpoints exact. The fader law itself is untouched.
pub fn raw01_to_wire_stepped(value: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "x32: fader out of range"
    );
    Ok(quantize_wire(value, FADER_WIRE_STEPS))
}

/// Number of wire values for a stepped `linf` range.
pub fn linf_wire_steps(min: f32, max: f32, step: f32) -> Option<u32> {
    if step <= 0.0 || max <= min {
        return None;
    }
    let n = ((max - min) / step).round() + 1.0;
    if n < 2.0 || n > 100_000.0 {
        return None;
    }
    Some(n as u32)
}

/// True when a normalized wire float sits on one of `steps` discrete
/// desk values within float roundoff.
pub fn wire_is_on_grid(wire: f32, steps: u32) -> bool {
    if steps < 2 || !wire.is_finite() {
        return false;
    }
    (wire - quantize_wire(wire, steps)).abs() <= WIRE_EPS
}

/// Wire-domain agreement for one stepped leaf: exact echo (emulator or
/// already-on-quantum write) within float roundoff, or a readback that
/// sits on the documented grid within half a wire step of the request
/// (desk rounded to the nearest quantum). Anything else is a refused
/// or misdirected write, never a confirm.
pub fn wire_agrees_stepped(written: f32, read: f32, steps: u32) -> bool {
    if steps < 2 || !written.is_finite() || !read.is_finite() {
        return false;
    }
    if (read - written).abs() <= WIRE_EPS {
        return true;
    }
    let half = 0.5 / (steps - 1) as f32;
    wire_is_on_grid(read, steps) && (read - written).abs() <= half + WIRE_EPS
}
