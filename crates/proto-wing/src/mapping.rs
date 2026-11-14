//! WING Remote Protocols 3.0.5-1, pp. 18–28, 41–58.
//! OSC numbers are engineering values, never the second (normalized) reply arg.
use openfader_core::{ParamValue, StripFamily, StripId, StripKind};
use rosc::{OscMessage, OscType};

pub const TOPOLOGY: &[StripFamily] = &[
    StripFamily {
        key: "ch",
        kind: StripKind::In,
        label: "Channels",
        first: 0,
        count: 40,
        name_prefix: "Ch",
        color_default: 2,
    },
    StripFamily {
        key: "aux",
        kind: StripKind::Ret,
        label: "Aux inputs",
        first: 40,
        count: 8,
        name_prefix: "Aux",
        color_default: 3,
    },
    StripFamily {
        key: "bus",
        kind: StripKind::Bus,
        label: "Buses",
        first: 48,
        count: 16,
        name_prefix: "Bus",
        color_default: 4,
    },
    StripFamily {
        key: "main",
        kind: StripKind::Main,
        label: "Mains",
        first: 64,
        count: 4,
        name_prefix: "Main",
        color_default: 5,
    },
    StripFamily {
        key: "mtx",
        kind: StripKind::Mtx,
        label: "Matrices",
        first: 68,
        count: 8,
        name_prefix: "Mtx",
        color_default: 6,
    },
    StripFamily {
        key: "dca",
        kind: StripKind::Dca,
        label: "DCAs",
        first: 76,
        count: 16,
        name_prefix: "DCA",
        color_default: 7,
    },
];
pub const STRIPS: usize = 92;
pub const OUTPUTS: &[StripKind] = &[StripKind::Bus, StripKind::Main, StripKind::Mtx];

pub fn root(strip: StripId) -> anyhow::Result<(String, StripKind)> {
    let i = strip.0 as usize;
    let f = TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .ok_or_else(|| anyhow::anyhow!("WING: strip out of range"))?;
    Ok((format!("/{}/{}", f.key, i - f.first + 1), f.kind))
}

// This is OpenFader's lossless normalized representation, NOT WING's raw OSC
// position. All wire writes use dB. Dart wingLaw must use these same endpoints.
pub fn fader_db(v: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        v.is_finite() && (0.0..=1.0).contains(&v),
        "invalid WING fader"
    );
    Ok(-144.0 + v * 154.0)
}
pub fn fader_unit(db: f32) -> anyhow::Result<f32> {
    anyhow::ensure!(
        db.is_finite() && (-144.0..=10.0).contains(&db),
        "invalid WING level {db}"
    );
    Ok((db + 144.0) / 154.0)
}

#[derive(Clone, Debug)]
pub enum WireKind {
    Float(f32, f32),
    Bool,
    Int(i32, i32),
    Text(usize),
    Enum(&'static [&'static str]),
    Ratio,
    Choices(&'static [(&'static str, f32)]),
    Fixed(f32),
    Fader,
}
#[derive(Clone, Debug)]
pub struct Mapping {
    pub path: String,
    pub kind: WireKind,
    pub model: Option<(&'static str, &'static str)>,
}
impl Mapping {
    pub fn decode(&self, msg: &OscMessage) -> anyhow::Result<ParamValue> {
        // WING single-tag events use one arg; queries use sff/sfi. Reject
        // malformed tuples rather than silently interpreting normalized data.
        let arg = match msg.args.as_slice() {
            [one] => one,
            [OscType::String(_), OscType::Float(raw), value] if raw.is_finite() => value,
            _ => anyhow::bail!("WING: malformed reply at {}", self.path),
        };
        // Some WING integer query replies encode the final field as a
        // zero-based step, while others carry the engineering integer.
        // The documented normalized position and parameter domain identify
        // the value independently. Do not apply this to one-argument events.
        if let (
            WireKind::Int(lo, hi),
            [OscType::String(display), OscType::Float(raw), OscType::Int(wire)],
        ) = (&self.kind, msg.args.as_slice())
        {
            anyhow::ensure!(
                raw.is_finite() && (0.0..=1.0).contains(raw),
                "WING: invalid integer position at {}: {:?}",
                self.path,
                msg.args
            );
            let position = f64::from(*lo) + f64::from(*raw) * (f64::from(*hi) - f64::from(*lo));
            let rounded = position.round();
            anyhow::ensure!(
                (position - rounded).abs() <= 0.001
                    && rounded >= f64::from(*lo)
                    && rounded <= f64::from(*hi),
                "WING: inconsistent integer position at {}: {:?}",
                self.path,
                msg.args
            );
            let value = rounded as i32;
            anyhow::ensure!(
                *wire == value || i64::from(*wire) == i64::from(value) - i64::from(*lo),
                "WING: inconsistent integer reply at {}: {:?}",
                self.path,
                msg.args
            );
            if let Ok(shown) = display.trim().parse::<i32>() {
                anyhow::ensure!(
                    shown == value,
                    "WING: inconsistent integer display at {}: {:?}",
                    self.path,
                    msg.args
                );
            }
            return Ok(ParamValue::Float(value as f32));
        }
        Ok(match (&self.kind, arg) {
            (WireKind::Fixed(v), OscType::String(s)) if s == "STD" => ParamValue::Float(*v),
            (WireKind::Choices(choices), OscType::String(s)) => ParamValue::Float(
                choices
                    .iter()
                    .find(|(name, _)| *name == s)
                    .ok_or_else(|| anyhow::anyhow!("unknown WING choice"))?
                    .1,
            ),
            (WireKind::Bool, OscType::Int(v)) if (0..=1).contains(v) => ParamValue::Bool(*v != 0),
            (WireKind::Int(lo, hi), OscType::Int(v)) if v >= lo && v <= hi => {
                ParamValue::Float(*v as f32)
            }
            (WireKind::Float(lo, hi), OscType::Float(v))
                if v.is_finite() && *v >= *lo - 0.01 && *v <= *hi + 0.01 =>
            {
                ParamValue::Float(*v)
            }
            (WireKind::Ratio, OscType::String(v)) => {
                let v: f32 = v.parse()?;
                anyhow::ensure!(RATIOS.contains(&v), "unsupported ratio");
                ParamValue::Float(v)
            }
            (WireKind::Ratio, OscType::Float(v)) if RATIOS.contains(v) => ParamValue::Float(*v),
            (WireKind::Fader, OscType::Float(v)) => ParamValue::Float(fader_unit(*v)?),
            (WireKind::Text(_), OscType::String(v)) => ParamValue::Text(v.clone()),
            (WireKind::Enum(choices), OscType::String(v)) => ParamValue::Float(
                choices
                    .iter()
                    .position(|s| *s == v)
                    .ok_or_else(|| anyhow::anyhow!("WING: unknown enum {v:?}"))?
                    as f32,
            ),
            _ => anyhow::bail!(
                "WING: invalid value at {} for {:?}: {:?}",
                self.path,
                self.kind,
                msg.args
            ),
        })
    }
    pub fn encode(&self, value: ParamValue) -> anyhow::Result<OscType> {
        Ok(match (&self.kind, value) {
            (WireKind::Choices(choices), ParamValue::Float(v)) => OscType::String(
                choices
                    .iter()
                    .find(|(_, n)| *n == v)
                    .ok_or_else(|| anyhow::anyhow!("invalid WING choice"))?
                    .0
                    .into(),
            ),
            (WireKind::Bool, ParamValue::Bool(v)) => OscType::Int(i32::from(v)),
            (WireKind::Float(lo, hi), ParamValue::Float(v))
                if v.is_finite() && v >= *lo && v <= *hi =>
            {
                OscType::Float(v)
            }
            (WireKind::Ratio, ParamValue::Float(v)) if RATIOS.contains(&v) => {
                OscType::String(v.to_string())
            }
            (WireKind::Fader, ParamValue::Float(v)) => OscType::Float(fader_db(v)?),
            (WireKind::Int(lo, hi), ParamValue::Float(v))
                if v.is_finite() && v.fract() == 0.0 && v >= *lo as f32 && v <= *hi as f32 =>
            {
                OscType::Int(v as i32)
            }
            (WireKind::Enum(choices), ParamValue::Float(v))
                if v.is_finite()
                    && v.fract() == 0.0
                    && v >= 0.0
                    && (v as usize) < choices.len() =>
            {
                OscType::String(choices[v as usize].to_string())
            }
            (WireKind::Text(max), ParamValue::Text(v)) if v.len() <= *max && !v.contains('\0') => {
                OscType::String(v)
            }
            _ => anyhow::bail!("WING: invalid value for {}", self.path),
        })
    }
}

pub fn mapping(strip: StripId, param: &str) -> anyhow::Result<Mapping> {
    let (r, kind) = root(strip)?;
    let input = matches!(kind, StripKind::In | StripKind::Ret);
    let dsp = kind != StripKind::Dca;
    let comp = dsp && kind != StripKind::Ret;
    if kind == StripKind::In {
        if let Some(mapping) = crate::filters::mapping(&r, param) {
            return Ok(mapping);
        }
    }
    let mut model = None;
    let (suffix, wire) = match param {
        "fader" => ("fdr".to_string(), WireKind::Fader),
        "mute" => ("mute".into(), WireKind::Bool),
        "name" => (
            "name".into(),
            WireKind::Text(if kind == StripKind::Dca { 8 } else { 16 }),
        ),
        "color" => ("col".into(), WireKind::Int(1, 12)),
        "solo" => ("$solo".into(), WireKind::Bool),
        "pan" if dsp => ("pan".into(), WireKind::Float(-100., 100.)),
        "trim" if input => ("in/set/trim".into(), WireKind::Float(-18., 18.)),
        "wing:input_mode" if input => ("in/set/$mode".into(), WireKind::Enum(crate::source::MODES)),
        "wing:source_auto" if input => ("in/set/srcauto".into(), WireKind::Bool),
        "wing:input_balance" if input => ("in/set/bal".into(), WireKind::Float(-9., 9.)),
        "wing:width" if dsp => ("wid".into(), WireKind::Float(-150., 150.)),
        "polarity" if input => ("in/set/inv".into(), WireKind::Bool),
        "source_select" if input => ("in/set/altsrc".into(), WireKind::Int(0, 1)),
        "lr_assign" if input || kind == StripKind::Bus => ("main/1/on".into(), WireKind::Bool),
        "wing:send_tap" if kind == StripKind::In => (
            "ptap".into(),
            WireKind::Enum(&["IN", "FILT", "3", "4", "5", "PFL", "AFL", "POST"]),
        ),
        "hpf_on" if kind == StripKind::In => ("flt/lc".into(), WireKind::Bool),
        "hpf_freq" if kind == StripKind::In => ("flt/lcf".into(), WireKind::Float(20., 2000.)),
        "gate_mode" if kind == StripKind::In => ("gate/mdl".into(), WireKind::Enum(GATE_MODELS)),
        "comp_mode" if comp => ("dyn/mdl".into(), WireKind::Enum(COMP_MODELS)),
        "eq_mode" if dsp => ("eq/mdl".into(), WireKind::Enum(EQ_MODELS)),
        "eq_on" if dsp => ("eq/on".into(), WireKind::Bool),
        "gate_on" if kind == StripKind::In => ("gate/on".into(), WireKind::Bool),
        "comp_on" if dsp => ("dyn/on".into(), WireKind::Bool),
        "comp_mix" if comp => ("dyn/mix".into(), WireKind::Float(0., 100.)),
        "delay_on" if dsp => {
            return Ok(Mapping {
                path: crate::delay::path(strip, "on")?,
                kind: WireKind::Bool,
                model: None,
            })
        }
        "wing:delay_unit" if dsp => {
            return Ok(Mapping {
                path: crate::delay::path(strip, "mode")?,
                kind: WireKind::Enum(crate::delay::UNITS),
                model: None,
            })
        }
        "comp_makeup" if comp => ("dyn/gain".into(), WireKind::Float(-6., 12.)),
        _ if param.starts_with("send") => {
            let (n, leaf) = param[4..]
                .split_once('_')
                .ok_or_else(|| anyhow::anyhow!("invalid send"))?;
            let n: u16 = n.parse()?;
            anyhow::ensure!((1..=28).contains(&n), "send out of range");
            let source_allowed = if n <= 16 || n >= 25 {
                input || kind == StripKind::Bus
            } else {
                input || matches!(kind, StripKind::Bus | StripKind::Main)
            };
            anyhow::ensure!(
                source_allowed,
                "WING: send unavailable on this strip family"
            );
            let target = match n {
                1..=16 => format!("send/{n}"),
                17..=24 => format!("send/MX{}", n - 16),
                _ => format!("main/{}", n - 24),
            };
            let (leaf, wire) = match leaf {
                "level" => ("lvl", WireKind::Float(-144., 10.)),
                "on" => ("on", WireKind::Bool),
                "pan" if input && n <= 24 => ("pan", WireKind::Float(-100., 100.)),
                "tap" if input && n <= 24 => (
                    "mode",
                    WireKind::Choices(&[("PRE", 3.), ("POST", 4.), ("GRP", 5.)]),
                ),
                "pre" if !input || n >= 25 => ("pre", WireKind::Bool),
                _ => anyhow::bail!("unsupported WING send parameter"),
            };
            (format!("{target}/{leaf}"), wire)
        }
        _ if param.starts_with("eq") && dsp => {
            let (n, leaf) = param[2..]
                .split_once('_')
                .ok_or_else(|| anyhow::anyhow!("invalid EQ parameter"))?;
            let n: u8 = n.parse()?;
            let bands = if OUTPUTS.contains(&kind) { 8 } else { 6 };
            anyhow::ensure!((1..=bands).contains(&n), "EQ band out of range");
            let band = match n {
                1 => "l".to_string(),
                n if n == bands => "h".into(),
                n => (n - 1).to_string(),
            };
            let (leaf, wire) = match leaf {
                "gain" => ("g", WireKind::Float(-15., 15.)),
                "freq" if n == 1 => ("f", WireKind::Float(20., 2000.)),
                "freq" if n == bands => ("f", WireKind::Float(50., 20000.)),
                "freq" => ("f", WireKind::Float(20., 20000.)),
                "q" => ("q", WireKind::Float(0.44, 10.)),
                "type" => {
                    model = Some(("eq", "STD"));
                    if n == 1 || n == bands {
                        return Ok(Mapping {
                            path: format!("{r}/eq/{band}eq"),
                            kind: WireKind::Choices(if n == 1 {
                                &[("PEQ", 2.), ("SHV", 1.)]
                            } else {
                                &[("PEQ", 2.), ("SHV", 4.)]
                            }),
                            model,
                        });
                    }
                    return Ok(Mapping {
                        path: format!("{r}/eq/mdl"),
                        kind: WireKind::Fixed(2.),
                        model,
                    });
                }
                _ => anyhow::bail!("unsupported WING EQ parameter"),
            };
            model = Some(("eq", "STD"));
            (format!("eq/{band}{leaf}"), wire)
        }
        _ if param.starts_with("gate_") && kind == StripKind::In => {
            let (leaf, wire) = match &param[5..] {
                "thr" => ("thr", WireKind::Float(-80., 0.)),
                "range" => ("range", WireKind::Float(3., 60.)),
                "att" => ("att", WireKind::Float(0., 120.)),
                "hold" => ("hld", WireKind::Float(0., 200.)),
                "rel" => ("rel", WireKind::Float(4., 4000.)),
                _ => anyhow::bail!("unsupported WING gate parameter"),
            };
            model = Some(("gate", "GATE"));
            (format!("gate/{leaf}"), wire)
        }
        _ if param.starts_with("comp_") && comp => {
            let (leaf, wire) = match &param[5..] {
                "thr" => ("thr", WireKind::Float(-60., 0.)),
                "ratio" => ("ratio", WireKind::Ratio),
                "det" => ("det", WireKind::Enum(&["PEAK", "RMS"])),
                "env" => ("env", WireKind::Enum(&["LIN", "LOG"])),
                "att" => ("att", WireKind::Float(0., 120.)),
                "hold" => ("hld", WireKind::Float(1., 200.)),
                "rel" => ("rel", WireKind::Float(4., 4000.)),
                "knee" => ("knee", WireKind::Int(0, 5)),
                "auto" => ("auto", WireKind::Bool),
                _ => anyhow::bail!("unsupported WING compressor parameter"),
            };
            model = Some(("dyn", "COMP"));
            (format!("dyn/{leaf}"), wire)
        }
        _ => anyhow::bail!("WING: unsupported {param:?} on {r}"),
    };
    Ok(Mapping {
        path: format!("{r}/{suffix}"),
        kind: wire,
        model,
    })
}

pub const GATE_MODELS: &[&str] = &[
    "GATE", "DUCK", "E88", "9000G", "D241", "DS902", "WAVE", "DEQ", "WARM", "76LA", "LA", "RIDE",
    "PSE", "CMB",
];
pub const COMP_MODELS: &[&str] = &[
    "COMP", "EXP", "B160", "B560", "D241", "ECL33", "9000C", "SBUS", "RED3", "76LA", "LA", "F670",
    "BLISS", "NSTR", "WAVE", "RIDE", "2250", "L100", "CMB",
];
pub const EQ_MODELS: &[&str] = &[
    "STD", "SOUL", "E88", "E84", "F110", "PULSAR", "MACH4", "PIA",
];
pub const RATIOS: &[f32] = &[
    1.1, 1.2, 1.3, 1.5, 1.7, 2., 2.5, 3., 3.5, 4., 5., 6., 8., 10., 20., 50., 100.,
];
