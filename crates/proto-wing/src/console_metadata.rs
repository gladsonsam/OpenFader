//! Native WING value decoding and DD descriptor projection.
use crate::{
    mapping::{Mapping, WireKind},
    native::{Definition, Kind},
};
use anyhow::{ensure, Context};
use openfader_core::{
    console_controls::{ConsoleControl, ConsoleControlChoice, ConsoleControlKind as CK},
    ParamValue,
};
use rosc::{OscMessage, OscType};
use serde_json::{json, Value};

pub(crate) fn valid_path(path: &str) -> bool {
    path == "/"
        || (path.starts_with('/')
            && path.len() <= 512
            && !path.ends_with('/')
            && path[1..].split('/').all(|p| {
                !p.is_empty()
                    && p != "."
                    && p != ".."
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'$'))
            }))
}
pub(crate) fn child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}
pub(crate) fn model_path(path: &str) -> Option<String> {
    let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 4
        && matches!(parts[0], "ch" | "aux" | "bus" | "main" | "mtx")
        && matches!(parts[2], "eq" | "gate" | "dyn" | "flt")
        && parts[3] != "mdl"
    {
        Some(format!("/{}/{}/{}/mdl", parts[0], parts[1], parts[2]))
    } else if parts.len() >= 3 && parts[0] == "fx" && parts[2] != "mdl" {
        Some(format!("/fx/{}/mdl", parts[1]))
    } else {
        None
    }
}
fn endpoint_allowance(value: f32) -> f64 {
    2. * f64::from(f32::EPSILON) * f64::from(value).abs().max(1.)
}

pub(crate) fn value_of(def: &Definition, msg: &OscMessage) -> anyhow::Result<Value> {
    let arg = match msg.args.as_slice() {
        [one] => one,
        [OscType::String(_), OscType::Float(_), v] => v,
        _ => anyhow::bail!("invalid WING value reply"),
    };
    match &def.kind {
        Kind::Integer { min, max } => {
            let value = Mapping {
                path: msg.addr.clone(),
                kind: WireKind::Int(*min, *max),
                model: None,
            }
            .decode(msg)?;
            let ParamValue::Float(v) = value else {
                unreachable!()
            };
            if *min == 0 && *max == 1 {
                Ok(json!(v != 0.))
            } else {
                Ok(json!(v as i32))
            }
        }
        Kind::Linear { min, max, .. } | Kind::Log { min, max, .. } => match arg {
            OscType::Float(v)
                if v.is_finite()
                    && min.is_finite()
                    && max.is_finite()
                    && f64::from(*v)
                        >= f64::from(min.min(*max)) - endpoint_allowance(min.min(*max))
                    && f64::from(*v)
                        <= f64::from(min.max(*max)) + endpoint_allowance(min.max(*max)) =>
            {
                Ok(json!(v))
            }
            _ => anyhow::bail!("invalid WING numeric value"),
        },
        Kind::Fader => match arg {
            OscType::Float(v) if v.is_finite() && (-144.0..=10.0).contains(v) => Ok(json!(v)),
            _ => anyhow::bail!("invalid WING fader value"),
        },
        Kind::Enum(options) => match arg {
            OscType::String(s) if options.iter().any(|(v, _)| v == s) => Ok(json!(s)),
            _ => anyhow::bail!("invalid WING enum value"),
        },
        Kind::FloatEnum(options) => match arg {
            OscType::Float(v) if options.iter().any(|(n, _)| *n == *v) => Ok(json!(v)),
            OscType::String(s) => options
                .iter()
                .find(|(_, label)| label == s)
                .map(|(v, _)| json!(v))
                .context("invalid WING float enum"),
            _ => anyhow::bail!("invalid WING float enum"),
        },
        Kind::Text { max_bytes } => match arg {
            OscType::String(s) if s.len() <= *max_bytes as usize => Ok(json!(s)),
            _ => anyhow::bail!("invalid WING text"),
        },
        Kind::Node => anyhow::bail!("node has no value"),
    }
}
pub(crate) fn encode(def: &Definition, value: &Value) -> anyhow::Result<OscType> {
    Ok(match &def.kind {
        Kind::Integer { min, max } => {
            let n = if *min == 0 && *max == 1 {
                value
                    .as_bool()
                    .map(i64::from)
                    .or_else(|| value.as_i64())
                    .context("expected boolean")?
            } else {
                value.as_i64().context("expected integer")?
            };
            ensure!(
                n >= i64::from(*min) && n <= i64::from(*max),
                "integer outside live range"
            );
            OscType::Int(n as i32)
        }
        Kind::Linear { min, max, .. } | Kind::Log { min, max, .. } => {
            let n = value.as_f64().context("expected number")?;
            ensure!(
                n.is_finite() && n >= f64::from(min.min(*max)) && n <= f64::from(min.max(*max)),
                "number outside live range"
            );
            OscType::Float(n as f32)
        }
        Kind::Fader => {
            let n = value.as_f64().context("expected fader dB")?;
            ensure!(
                n.is_finite() && (-144.0..=10.0).contains(&n),
                "fader outside range"
            );
            OscType::Float(n as f32)
        }
        Kind::Enum(options) => {
            let s = value.as_str().context("expected enum")?;
            ensure!(options.iter().any(|(v, _)| v == s), "unknown enum");
            OscType::String(s.into())
        }
        Kind::FloatEnum(options) => {
            let n = value.as_f64().context("expected numeric choice")?;
            ensure!(
                options.iter().any(|(v, _)| *v == n as f32),
                "unknown numeric choice"
            );
            OscType::Float(n as f32)
        }
        Kind::Text { max_bytes } => {
            let s = value.as_str().context("expected text")?;
            ensure!(
                s.len() <= *max_bytes as usize && !s.contains('\0'),
                "text outside live limit"
            );
            OscType::String(s.into())
        }
        Kind::Node => anyhow::bail!("cannot write node"),
    })
}
/// Predict the console's discrete linear/log position from its live DD.
/// Verification accepts the requested float32 value or the predicted grid
/// position, with bounded float32 roundoff, never a half-step tolerance.
pub(crate) fn readback_agrees(
    def: &Definition,
    requested: &Value,
    observed: &Value,
) -> anyhow::Result<(bool, bool)> {
    match &def.kind {
        Kind::Linear { min, max, steps } | Kind::Log { min, max, steps } if *steps > 1 => {
            let requested = requested.as_f64().context("expected numeric value")? as f32;
            let observed = observed.as_f64().context("invalid numeric readback")?;
            let lo = f64::from(*min);
            let hi = f64::from(*max);
            let input = f64::from(requested);
            ensure!(
                lo.is_finite() && hi.is_finite() && lo != hi,
                "invalid live numeric grid"
            );
            let logarithmic = matches!(def.kind, Kind::Log { .. });
            let position = if logarithmic {
                ensure!(
                    lo > 0. && hi > 0. && input > 0.,
                    "invalid live logarithmic grid"
                );
                (input / lo).ln() / (hi / lo).ln()
            } else {
                (input - lo) / (hi - lo)
            };
            let index = (position.clamp(0., 1.) * f64::from(*steps - 1)).round();
            let unit = index / f64::from(*steps - 1);
            let predicted = if logarithmic {
                lo * (hi / lo).powf(unit)
            } else {
                lo + (hi - lo) * unit
            };
            let raw_agrees = (observed - input).abs() <= endpoint_allowance(requested);
            let grid_agrees = (observed - predicted).abs() <= endpoint_allowance(predicted as f32);
            Ok((raw_agrees || grid_agrees, grid_agrees && !raw_agrees))
        }
        Kind::FloatEnum(_) => Ok((
            requested
                .as_f64()
                .zip(observed.as_f64())
                .is_some_and(|(a, b)| (a as f32) == (b as f32)),
            false,
        )),
        Kind::Fader | Kind::Linear { .. } | Kind::Log { .. } => Ok((
            requested
                .as_f64()
                .zip(observed.as_f64())
                .is_some_and(|(a, b)| (a - b).abs() <= 0.0001),
            false,
        )),
        _ => Ok((requested == observed, false)),
    }
}

pub(crate) fn descriptor(path: String, def: &Definition) -> ConsoleControl {
    let mut c = ConsoleControl {
        path,
        label: if def.label.is_empty() {
            def.address_name().unwrap_or_default()
        } else {
            def.label.clone()
        },
        read_only: def.read_only,
        ..Default::default()
    };
    c.unit = match def.unit {
        1 => "dB",
        2 => "%",
        3 => "ms",
        4 => "Hz",
        5 => "m",
        6 => "s",
        7 => "oct",
        _ => "",
    }
    .into();
    match &def.kind {
        Kind::Node => {
            c.kind = CK::Branch;
            c.read_only = true;
        }
        Kind::Integer { min, max } => {
            c.kind = if *min == 0 && *max == 1 {
                CK::Boolean
            } else {
                CK::Integer
            };
            c.min = Some((*min).into());
            c.max = Some((*max).into());
            c.step = Some(1.);
        }
        Kind::Linear { min, max, steps } | Kind::Log { min, max, steps } => {
            c.kind = CK::Number;
            c.min = Some(f64::from(min.min(*max)));
            c.max = Some(f64::from(min.max(*max)));
            if *steps > 1 {
                c.step = Some(f64::from((max - min).abs()) / f64::from(*steps - 1));
            }
        }
        Kind::Fader => {
            c.kind = CK::Number;
            c.min = Some(-144.);
            c.max = Some(10.);
            c.unit = "dB".into();
        }
        Kind::Enum(options) => {
            c.kind = CK::Enumeration;
            c.choices = options
                .iter()
                .map(|(v, l)| ConsoleControlChoice {
                    label: l.clone(),
                    value: json!(v),
                })
                .collect();
        }
        Kind::FloatEnum(options) => {
            c.kind = CK::Enumeration;
            c.choices = options
                .iter()
                .map(|(v, l)| ConsoleControlChoice {
                    label: l.clone(),
                    value: json!(v),
                })
                .collect();
        }
        Kind::Text { .. } => c.kind = CK::Text,
    }
    if matches!(
        c.path.as_str(),
        "/$ctl/lib/$action" | "/play/$action" | "/rec/$action"
    ) {
        c.kind = CK::Action;
        c.choices.retain(|choice| match c.path.as_str() {
            "/$ctl/lib/$action" => choice.value == "GO",
            "/play/$action" => matches!(choice.value.as_str(), Some("PLAY" | "PAUSE" | "STOP")),
            "/rec/$action" => matches!(choice.value.as_str(), Some("REC" | "PAUSE" | "STOP")),
            _ => false,
        });
        c.read_only = def.read_only || c.choices.is_empty();
        if c.read_only {
            c.unavailable_reason = Some("Native action is not writable on this console.".into());
        }
        return c;
    }
    if c.path == "/$ctl/lib/$actionidx" {
        return c;
    }
    // $ commands can be actions despite their reported DD access flag. The
    // generic browser does not guess their side effects or acknowledge them.
    if c.path
        .rsplit('/')
        .next()
        .is_some_and(|s| s.starts_with('$'))
        && !matches!(def.kind, Kind::Node)
    {
        c.read_only = true;
        c.unavailable_reason =
            Some("This native action or derived value requires a dedicated operation.".into());
    }
    c
}
