//! X32 console control native and engineering value codecs.
use super::specs::{ConsoleControl, ConsoleControlKind, NumberScale};
use crate::{
    codec,
    mapping::{Mapping, WireKind},
};
use openfader_core::ParamValue;
pub(super) fn mapping(row: &ConsoleControl) -> Mapping {
    Mapping {
        path: row.path.into(),
        kind: match row.kind {
            ConsoleControlKind::Toggle => WireKind::Bool,
            ConsoleControlKind::Choice(options) => WireKind::Enum(options),
            ConsoleControlKind::IQEq(options) => WireKind::Enum(options),
            ConsoleControlKind::CardChoice { choices, .. } => WireKind::Enum(choices),
            ConsoleControlKind::IQSound { .. } => WireKind::Int(0, 7),
            ConsoleControlKind::Bitset(options) => WireKind::Int(
                0,
                options.iter().fold(0, |mask, (_, bit)| mask | *bit) as i32,
            ),
            ConsoleControlKind::ReadOnlyChoice(options) => WireKind::Enum(options),
            ConsoleControlKind::ReadOnlyText(max) => WireKind::Text(max),
            ConsoleControlKind::UserAssignment(max) => WireKind::Text(max),
            ConsoleControlKind::Text(max) => WireKind::Text(max),
            ConsoleControlKind::Number {
                min, max, scale, ..
            } => match scale {
                NumberScale::Linear { .. } => WireKind::Linf(min, max),
                NumberScale::Logarithmic { .. } => WireKind::Logf(min, max),
                NumberScale::Level => WireKind::LevelDb,
            },
        },
    }
}

pub(super) fn encode(
    row: &ConsoleControl,
    desired: &ParamValue,
) -> anyhow::Result<(rosc::OscType, ParamValue)> {
    let m = mapping(row);
    let arg = match (row.kind, desired) {
        (
            ConsoleControlKind::Number {
                min, max, scale, ..
            },
            ParamValue::Float(v),
        ) => {
            anyhow::ensure!(
                v.is_finite() && *v >= min && *v <= max,
                "x32: console control value out of range"
            );
            rosc::OscType::Float(match scale {
                NumberScale::Linear { step } => codec::linf_to_wire_stepped(*v, min, max, step)?,
                NumberScale::Logarithmic { steps } => {
                    codec::logf_to_wire_stepped(*v, min, max, steps)?
                }
                NumberScale::Level => codec::level_to_wire_stepped(*v)?,
            })
        }
        _ => m.encode(desired.clone())?,
    };
    let target = m.decode(&rosc::OscMessage {
        addr: m.path.clone(),
        args: vec![arg.clone()],
    })?;
    Ok((arg, target))
}

pub(super) fn agrees(
    row: &ConsoleControl,
    target: &ParamValue,
    applied: &ParamValue,
) -> anyhow::Result<bool> {
    Ok(match (row.kind, target, applied) {
        (
            ConsoleControlKind::Number {
                min, max, scale, ..
            },
            ParamValue::Float(want),
            ParamValue::Float(got),
        ) => {
            let norm = |v| match scale {
                NumberScale::Linear { .. } => codec::linf_to_wire(v, min, max),
                NumberScale::Logarithmic { .. } => codec::logf_to_wire(v, min, max),
                NumberScale::Level => codec::db_to_fader(v),
            };
            (norm(*want)? - norm(*got)?).abs() < 0.000025
        }
        _ => applied == target,
    })
}

pub(super) fn json_value(v: &ParamValue) -> serde_json::Value {
    match v {
        ParamValue::Bool(v) => serde_json::json!(v),
        ParamValue::Float(v) => serde_json::json!(v),
        ParamValue::Text(v) => serde_json::json!(v),
    }
}
pub(super) fn parse_value(
    row: &ConsoleControl,
    value: &serde_json::Value,
) -> anyhow::Result<ParamValue> {
    match row.kind {
        ConsoleControlKind::Toggle => Ok(ParamValue::Bool(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("expected boolean"))?,
        )),
        ConsoleControlKind::Bitset(options) => {
            let v = value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("expected unsigned bitmap"))?;
            let mask = options
                .iter()
                .fold(0_u64, |mask, (_, bit)| mask | u64::from(*bit));
            anyhow::ensure!(v & !mask == 0, "unsupported bitmap bits");
            if row.id == "dp48.scope" {
                anyhow::ensure!(
                    v & 12 == 0 || v & 2 != 0,
                    "channel level and pan require group assign"
                );
            }
            Ok(ParamValue::Float(v as f32))
        }
        ConsoleControlKind::ReadOnlyChoice(_) | ConsoleControlKind::ReadOnlyText(_) => {
            anyhow::bail!("console control is read-only")
        }
        ConsoleControlKind::UserAssignment(max) => {
            let text = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("expected assignment code"))?;
            let encoder = row.id.contains(".encoder.");
            anyhow::ensure!(
                text.len() <= max && super::user_assign::validate(encoder, text),
                "unsupported or invalid X32 assignment code"
            );
            Ok(ParamValue::Text(text.into()))
        }
        ConsoleControlKind::Text(max) => {
            let text = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("expected text"))?;
            anyhow::ensure!(
                text.len() <= max && !text.contains('\0'),
                "text exceeds native limit"
            );
            Ok(ParamValue::Text(text.into()))
        }
        ConsoleControlKind::IQSound { .. } => {
            let value = value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("expected sound profile index"))?;
            anyhow::ensure!(value <= 7, "invalid sound profile index");
            Ok(ParamValue::Float(value as f32))
        }
        ConsoleControlKind::Choice(options)
        | ConsoleControlKind::IQEq(options)
        | ConsoleControlKind::CardChoice {
            choices: options, ..
        } => {
            let v = value
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("expected choice index"))?;
            anyhow::ensure!(
                v.is_finite() && v.fract() == 0.0 && v >= 0.0 && v < options.len() as f64,
                "invalid choice index"
            );
            Ok(ParamValue::Float(v as f32))
        }
        ConsoleControlKind::Number { .. } => {
            let v = value
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("expected number"))?;
            anyhow::ensure!(
                v.is_finite() && v >= f32::MIN as f64 && v <= f32::MAX as f64,
                "non-finite or out-of-range number"
            );
            Ok(ParamValue::Float(v as f32))
        }
    }
}

pub(super) fn parse_observed_value(
    row: &ConsoleControl,
    value: &serde_json::Value,
) -> anyhow::Result<ParamValue> {
    if let ConsoleControlKind::UserAssignment(max) = row.kind {
        let text = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("expected observed assignment"))?;
        anyhow::ensure!(
            text.len() <= max && !text.contains('\0'),
            "invalid observed assignment"
        );
        return Ok(ParamValue::Text(text.into()));
    }
    parse_value(row, value)
}
