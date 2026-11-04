//! Typed `/fx/N/par/MM` wire codec: normalized `0..1` engine values map
//! onto the documented wire type per parameter (float for `linf`/`logf`,
//! OSC int option index for `enum`), mirroring the validated X-Air pattern
//! (continuous params reply as floats, discrete replies as ints).

use super::catalogue::{param_info, FxParamKind};
use rosc::OscType;

fn check_value(value: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "x32: FX param out of range 0..1"
    );
    Ok(value)
}

/// Decode one `/fx/N/par/MM` reply into normalized `0..1` using the
/// parameter's own doc row (`None` = outside the effect's documented
/// count, rejected rather than surfaced as a fake value).
pub fn decode_fx_par(code: &str, par: u8, arg: &OscType) -> anyhow::Result<f32> {
    let info = param_info(code, par)
        .ok_or_else(|| anyhow::anyhow!("x32: {code} has no documented par{par:02}"))?;
    match arg {
        OscType::Int(i) if info.kind == FxParamKind::Enum => {
            decode_int(*i as i64, &info, code, par)
        }
        OscType::Long(l) if info.kind == FxParamKind::Enum => decode_int(*l, &info, code, par),
        OscType::Float(f) if info.kind != FxParamKind::Enum => {
            if !f.is_finite() {
                anyhow::bail!("x32: {code} par{par:02} absent (non-finite wire float {f})");
            }
            check_value(*f)
        }
        OscType::Double(d) if info.kind != FxParamKind::Enum => {
            if !d.is_finite() {
                anyhow::bail!("x32: {code} par{par:02} absent (non-finite wire double {d})");
            }
            check_value(*d as f32)
        }
        other => {
            anyhow::bail!("x32: {code} par{par:02} has the wrong documented wire type: {other:?}")
        }
    }
}

fn decode_int(
    i: i64,
    info: &super::catalogue::FxParamInfo,
    code: &str,
    par: u8,
) -> anyhow::Result<f32> {
    match info.options {
        Some(options) => {
            let n = options.len() as i64;
            if n < 2 {
                anyhow::bail!("x32: {code} par{par:02} enum has {n} options, cannot map int {i}");
            }
            if !(0..n).contains(&i) {
                anyhow::bail!("x32: {code} par{par:02} enum index {i} out of range 0..{n}");
            }
            Ok(i as f32 / (n - 1) as f32)
        }
        None => check_value(i as f32),
    }
}

/// Encode one normalized `0..1` value for the documented wire type of
/// `par` (int option index for enums, float otherwise). Out-of-count pars
/// and out-of-range values bail before any I/O.
pub fn encode_fx_par(code: &str, par: u8, value: f32) -> anyhow::Result<OscType> {
    let info = param_info(code, par)
        .ok_or_else(|| anyhow::anyhow!("x32: {code} has no documented par{par:02}"))?;
    let value = check_value(value)?;
    if info.kind == FxParamKind::Enum {
        let options = info.options.ok_or_else(|| {
            anyhow::anyhow!("x32: {code} par{par:02} enum has no documented options")
        })?;
        if options.len() < 2 {
            anyhow::bail!("x32: {code} par{par:02} enum has no documented options");
        }
        return Ok(OscType::Int(
            (value * (options.len() - 1) as f32).round() as i32
        ));
    }
    Ok(OscType::Float(value))
}
