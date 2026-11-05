//! X32 param plane: strip root + leaf + engineering-unit codec.
//!
//! Every canonical id served here converts between engineering units
//! (the `ParamValue` contract) and the normalized wire float. Enum
//! leaves accept int or string replies; int leaves accept int.
use crate::{addrs, codec};
use openfader_core::ParamValue;
use openfader_core::{StripId, StripKind};
use rosc::{OscMessage, OscType};

#[derive(Clone, Debug)]
pub enum WireKind {
    /// Raw normalized float (faders: no engineering conversion).
    Raw01,
    /// Bool as int 0/1 (`{OFF, ON}`).
    Bool,
    /// Int leaf with inclusive bounds (value rides `Float(index)`).
    Int(i32, i32),
    /// String leaf with max bytes.
    Text(usize),
    /// Enum leaf: int index or name (value rides `Float(index)`).
    Enum(&'static [&'static str]),
    /// Linear float leaf `[min, max]`.
    Linf(f32, f32),
    /// Logarithmic float leaf `[min, max]`.
    Logf(f32, f32),
    /// EQ Q (reversed log `10.0..0.3`).
    Q,
    /// Headamp gain `[-12, +60]` in 0.5 dB steps.
    Gain,
    /// Dynamics ratio step table (engineering ratio value).
    Ratio,
    /// Bus-send / mono level law (`-90..+10 dB` 4-segment curve).
    LevelDb,
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub path: String,
    pub kind: WireKind,
}

fn float_arg(msg: &OscMessage) -> anyhow::Result<f32> {
    anyhow::ensure!(
        msg.args.len() == 1,
        "x32: expected single arg at {}, got {}",
        msg.addr,
        msg.args.len()
    );
    match msg.args.first() {
        Some(OscType::Float(f)) => Ok(*f),
        Some(OscType::Double(d)) => Ok(*d as f32),
        other => anyhow::bail!("x32: expected float at {}, got {other:?}", msg.addr),
    }
}

/// Provider-owned strict integer decoder: exactly one arg, `Int` or a
/// finite integral `Float` inside the exact `i32` domain. `f64`
/// comparisons avoid `i32::MAX` rounding (`f32` cannot represent it
/// exactly, so `2^31` as `f32` correctly rejects). NaN/inf, fractional
/// floats, strings, blobs and multi-arg replies all bail.
pub(crate) fn strict_int(msg: &OscMessage) -> anyhow::Result<i32> {
    anyhow::ensure!(
        msg.args.len() == 1,
        "x32: expected single int arg at {}, got {}",
        msg.addr,
        msg.args.len()
    );
    match msg.args.first() {
        Some(OscType::Int(i)) => Ok(*i),
        Some(OscType::Float(f)) => {
            anyhow::ensure!(f.is_finite(), "x32: non-finite int at {}", msg.addr);
            anyhow::ensure!(f.fract() == 0.0, "x32: non-integral float at {}", msg.addr);
            let d = *f as f64;
            anyhow::ensure!(
                d >= i32::MIN as f64 && d <= i32::MAX as f64,
                "x32: int out of i32 domain at {}",
                msg.addr
            );
            Ok(*f as i32)
        }
        other => anyhow::bail!("x32: expected int at {}, got {other:?}", msg.addr),
    }
}

/// Provider-owned strict bool decoder: exactly one arg, `Int`/`Float`
/// `0`/`1` only. `2`, NaN and fractional floats all bail.
pub(crate) fn strict_bool(msg: &OscMessage) -> anyhow::Result<bool> {
    match strict_int(msg)? {
        0 => Ok(false),
        1 => Ok(true),
        other => anyhow::bail!("x32: bool {other} out of 0/1 domain at {}", msg.addr),
    }
}

impl Mapping {
    pub fn decode(&self, msg: &OscMessage) -> anyhow::Result<ParamValue> {
        anyhow::ensure!(
            msg.addr == self.path,
            "x32: reply address {} != {}",
            msg.addr,
            self.path
        );
        anyhow::ensure!(
            msg.args.len() == 1,
            "x32: expected single arg at {}, got {}",
            msg.addr,
            msg.args.len()
        );
        Ok(match &self.kind {
            WireKind::Raw01 => {
                let v = float_arg(msg)?;
                anyhow::ensure!((0.0..=1.0).contains(&v), "x32: fader out of range");
                ParamValue::Float(v)
            }
            WireKind::Bool => ParamValue::Bool(strict_bool(msg)?),
            WireKind::Int(lo, hi) => {
                let v = strict_int(msg)?;
                anyhow::ensure!((lo..=hi).contains(&&v), "x32: int out of range");
                ParamValue::Float(v as f32)
            }
            WireKind::Text(max) => match msg.args.first() {
                Some(OscType::String(s)) => {
                    anyhow::ensure!(
                        s.len() <= *max,
                        "x32: text exceeds {max} bytes at {}",
                        msg.addr
                    );
                    anyhow::ensure!(!s.contains('\0'), "x32: text contains NUL at {}", msg.addr);
                    ParamValue::Text(s.clone())
                }
                other => anyhow::bail!("x32: expected string at {}, got {other:?}", msg.addr),
            },
            WireKind::Enum(choices) => match msg.args.first() {
                Some(OscType::Int(i)) => {
                    anyhow::ensure!(
                        (0..choices.len() as i32).contains(i),
                        "x32: enum index out of range"
                    );
                    ParamValue::Float(*i as f32)
                }
                Some(OscType::String(s)) => {
                    let want = s.trim().to_uppercase();
                    let index = choices
                        .iter()
                        .position(|c| c.to_uppercase() == want)
                        .ok_or_else(|| anyhow::anyhow!("x32: unknown enum {s:?}"))?;
                    ParamValue::Float(index as f32)
                }
                Some(OscType::Float(f)) => {
                    anyhow::ensure!(f.is_finite(), "x32: non-finite enum at {}", msg.addr);
                    anyhow::ensure!(
                        (0.0..choices.len() as f32).contains(f),
                        "x32: enum index out of range"
                    );
                    anyhow::ensure!(f.fract() == 0.0, "x32: non-integral enum at {}", msg.addr);
                    ParamValue::Float(*f)
                }
                other => anyhow::bail!("x32: expected enum at {}, got {other:?}", msg.addr),
            },
            WireKind::Linf(min, max) => {
                ParamValue::Float(codec::linf_from_wire(float_arg(msg)?, *min, *max)?)
            }
            WireKind::Logf(min, max) => {
                ParamValue::Float(codec::logf_from_wire(float_arg(msg)?, *min, *max)?)
            }
            WireKind::Q => ParamValue::Float(codec::q_from_wire(float_arg(msg)?)?),
            WireKind::Gain => ParamValue::Float(codec::gain_from_wire(float_arg(msg)?)?),
            WireKind::Ratio => {
                let i = strict_int(msg)?;
                ParamValue::Float(
                    *addrs::DYN_RATIOS
                        .get(i as usize)
                        .ok_or_else(|| anyhow::anyhow!("x32: ratio index {i} out of range"))?,
                )
            }
            WireKind::LevelDb => {
                let db = codec::fader_db(float_arg(msg)?)?;
                // Wire 0.0 is the desk -inf floor; the canonical floor is
                // -90.0 dB (the documented range bottom), matching the
                // encode side (`-90.0` → wire 0.0) so floor writes round-trip.
                ParamValue::Float(if db.is_finite() { db } else { -90.0 })
            }
        })
    }

    pub fn encode(&self, value: ParamValue) -> anyhow::Result<OscType> {
        Ok(match (&self.kind, value) {
            (WireKind::Raw01, ParamValue::Float(v))
                if v.is_finite() && (0.0..=1.0).contains(&v) =>
            {
                OscType::Float(codec::raw01_to_wire_stepped(v)?)
            }
            (WireKind::Bool, ParamValue::Bool(v)) => OscType::Int(i32::from(v)),
            (WireKind::Int(lo, hi), ParamValue::Float(v))
                if v.is_finite() && v.fract() == 0.0 && v >= *lo as f32 && v <= *hi as f32 =>
            {
                OscType::Int(v as i32)
            }
            (WireKind::Text(max), ParamValue::Text(v)) if v.len() <= *max && !v.contains('\0') => {
                OscType::String(v)
            }
            (WireKind::Enum(choices), ParamValue::Float(v))
                if v.is_finite()
                    && v.fract() == 0.0
                    && v >= 0.0
                    && (v as usize) < choices.len() =>
            {
                OscType::Int(v as i32)
            }
            (WireKind::Linf(min, max), ParamValue::Float(v)) => {
                let wire = match codec::linf_step(*min, *max) {
                    Some(step) => codec::linf_to_wire_stepped(v, *min, *max, step)?,
                    None => codec::linf_to_wire(v, *min, *max)?,
                };
                OscType::Float(wire)
            }
            (WireKind::Logf(min, max), ParamValue::Float(v)) => {
                let wire = match codec::logf_steps(*min, *max) {
                    Some(steps) => codec::logf_to_wire_stepped(v, *min, *max, steps)?,
                    None => codec::logf_to_wire(v, *min, *max)?,
                };
                OscType::Float(wire)
            }
            (WireKind::Q, ParamValue::Float(v)) => OscType::Float(codec::q_to_wire_stepped(v)?),
            (WireKind::Gain, ParamValue::Float(v)) => OscType::Float(codec::gain_to_wire(v)?),
            (WireKind::Ratio, ParamValue::Float(v)) => {
                let index = addrs::DYN_RATIOS
                    .iter()
                    .position(|r| *r == v)
                    .ok_or_else(|| anyhow::anyhow!("x32: ratio {v} is not a desk step"))?;
                OscType::Int(index as i32)
            }
            (WireKind::LevelDb, ParamValue::Float(v)) if v.is_finite() => {
                OscType::Float(codec::level_to_wire_stepped(v)?)
            }
            _ => anyhow::bail!("x32: invalid value for {}", self.path),
        })
    }
}

fn leaf(root: &str, suffix: &str, kind: WireKind) -> Mapping {
    Mapping {
        path: format!("{root}/{suffix}"),
        kind,
    }
}

/// Family predicates shared by the plane.
pub struct Fam {
    pub is_ch: bool,
    pub is_aux: bool,
    pub is_fx: bool,
    pub is_bus: bool,
    pub is_mtx: bool,
    pub is_main: bool,
    pub is_dca: bool,
    pub key: &'static str,
}

pub fn fam_of(strip: StripId) -> anyhow::Result<(String, Fam)> {
    let (root, kind) = addrs::root(strip)?;
    let i = strip.0 as usize;
    let key = addrs::X32_TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .map(|f| f.key)
        .unwrap_or("?");
    Ok((
        root,
        Fam {
            is_ch: kind == StripKind::In,
            is_aux: key == "auxin",
            is_fx: key == "fxrtn",
            is_bus: kind == StripKind::Bus,
            is_mtx: kind == StripKind::Mtx,
            is_main: kind == StripKind::Main,
            is_dca: kind == StripKind::Dca,
            key,
        },
    ))
}

/// Canonical param → wire mapping. `gain`/`phantom` are intentionally
/// absent: they resolve through the shared-headamp path (`preamp`).
pub fn mapping(strip: StripId, param: &str) -> anyhow::Result<Mapping> {
    let (r, f) = fam_of(strip)?;
    let mixable = !f.is_dca;
    let sends_src = f.is_ch || f.is_aux || f.is_fx;
    let out_src = f.is_bus || f.is_main;
    match param {
        "fader" => Ok(leaf(
            &r,
            if f.is_dca { "fader" } else { "mix/fader" },
            WireKind::Raw01,
        )),
        "mute" => Ok(leaf(
            &r,
            if f.is_dca { "on" } else { "mix/on" },
            WireKind::Bool,
        )),
        "name" => Ok(leaf(&r, "config/name", WireKind::Text(12))),
        "color" => Ok(leaf(&r, "config/color", WireKind::Enum(addrs::COLORS))),
        "icon" => {
            anyhow::ensure!(mixable, "x32: icon unavailable on DCA");
            Ok(leaf(&r, "config/icon", WireKind::Int(1, 74)))
        }
        "pan" => {
            anyhow::ensure!(
                f.is_ch || f.is_aux || f.is_fx || f.is_bus || (f.is_main && f.key == "main"),
                "x32: pan unavailable on this strip"
            );
            Ok(leaf(&r, "mix/pan", WireKind::Linf(-100.0, 100.0)))
        }
        "lr_assign" => {
            anyhow::ensure!(sends_src || f.is_bus, "x32: LR assign unavailable here");
            Ok(leaf(&r, "mix/st", WireKind::Bool))
        }
        "x32:mono_assign" => {
            anyhow::ensure!(sends_src || f.is_bus, "x32: mono assign unavailable here");
            Ok(leaf(&r, "mix/mono", WireKind::Bool))
        }
        "x32:mono_level" => {
            anyhow::ensure!(sends_src || f.is_bus, "x32: mono level unavailable here");
            Ok(leaf(&r, "mix/mlevel", WireKind::LevelDb))
        }
        "trim" => {
            anyhow::ensure!(f.is_ch || f.is_aux, "x32: trim is channel/aux only");
            Ok(leaf(&r, "preamp/trim", WireKind::Linf(-18.0, 18.0)))
        }
        "polarity" => {
            anyhow::ensure!(f.is_ch || f.is_aux, "x32: polarity is channel/aux only");
            Ok(leaf(&r, "preamp/invert", WireKind::Bool))
        }
        "delay_on" => {
            anyhow::ensure!(f.is_ch, "x32: delay is channel only");
            Ok(leaf(&r, "delay/on", WireKind::Bool))
        }
        "delay_ms" => {
            anyhow::ensure!(f.is_ch, "x32: delay is channel only");
            Ok(leaf(&r, "delay/time", WireKind::Linf(0.3, 500.0)))
        }
        "hpf_on" => {
            anyhow::ensure!(f.is_ch, "x32: HPF is channel only");
            Ok(leaf(&r, "preamp/hpon", WireKind::Bool))
        }
        "hpf_freq" => {
            anyhow::ensure!(f.is_ch, "x32: HPF is channel only");
            Ok(leaf(&r, "preamp/hpf", WireKind::Logf(20.0, 400.0)))
        }
        "hpf_slope" => {
            anyhow::ensure!(f.is_ch, "x32: HPF is channel only");
            Ok(leaf(
                &r,
                "preamp/hpslope",
                WireKind::Enum(addrs::HPF_SLOPES),
            ))
        }
        "gate_on" if f.is_ch => Ok(leaf(&r, "gate/on", WireKind::Bool)),
        "gate_mode" if f.is_ch => Ok(leaf(&r, "gate/mode", WireKind::Enum(addrs::GATE_MODES))),
        "gate_thr" if f.is_ch => Ok(leaf(&r, "gate/thr", WireKind::Linf(-80.0, 0.0))),
        "gate_range" if f.is_ch => Ok(leaf(&r, "gate/range", WireKind::Linf(3.0, 60.0))),
        "gate_att" if f.is_ch => Ok(leaf(&r, "gate/attack", WireKind::Linf(0.0, 120.0))),
        "gate_hold" if f.is_ch => Ok(leaf(&r, "gate/hold", WireKind::Logf(0.02, 2000.0))),
        "gate_rel" if f.is_ch => Ok(leaf(&r, "gate/release", WireKind::Logf(5.0, 4000.0))),
        "gate_key_src" if f.is_ch => Ok(leaf(&r, "gate/keysrc", WireKind::Int(0, 64))),
        "gate_filter_on" if f.is_ch => Ok(leaf(&r, "gate/filter/on", WireKind::Bool)),
        "gate_filter_type" if f.is_ch => Ok(leaf(
            &r,
            "gate/filter/type",
            WireKind::Enum(addrs::KEY_FILTER_TYPES),
        )),
        "gate_filter_freq" if f.is_ch => {
            Ok(leaf(&r, "gate/filter/f", WireKind::Logf(20.0, 20000.0)))
        }
        "comp_on" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/on", WireKind::Bool))
        }
        "comp_mode" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/mode", WireKind::Enum(addrs::DYN_MODES)))
        }
        "comp_det" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/det", WireKind::Enum(addrs::DYN_DETS)))
        }
        "comp_env" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/env", WireKind::Enum(addrs::DYN_ENVS)))
        }
        "comp_thr" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/thr", WireKind::Linf(-60.0, 0.0)))
        }
        "comp_ratio" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/ratio", WireKind::Ratio))
        }
        "comp_knee" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/knee", WireKind::Linf(0.0, 5.0)))
        }
        "comp_makeup" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/mgain", WireKind::Linf(0.0, 24.0)))
        }
        "comp_att" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/attack", WireKind::Linf(0.0, 120.0)))
        }
        "comp_hold" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/hold", WireKind::Logf(0.02, 2000.0)))
        }
        "comp_rel" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/release", WireKind::Logf(5.0, 4000.0)))
        }
        "comp_mix" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/mix", WireKind::Linf(0.0, 100.0)))
        }
        "comp_auto" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/auto", WireKind::Bool))
        }
        "comp_key_src" if f.is_ch || f.is_bus => Ok(leaf(&r, "dyn/keysrc", WireKind::Int(0, 64))),
        "comp_filter_on" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/filter/on", WireKind::Bool))
        }
        "comp_filter_type" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => Ok(leaf(
            &r,
            "dyn/filter/type",
            WireKind::Enum(addrs::KEY_FILTER_TYPES),
        )),
        "comp_filter_freq" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "dyn/filter/f", WireKind::Logf(20.0, 20000.0)))
        }
        "eq_on" if mixable && !f.is_dca => Ok(leaf(&r, "eq/on", WireKind::Bool)),
        "insert_on" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "insert/on", WireKind::Bool))
        }
        "insert_sel" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "insert/sel", WireKind::Int(0, 22)))
        }
        "x32:insert_pos" if f.is_ch || f.is_bus || f.is_mtx || f.is_main => {
            Ok(leaf(&r, "insert/pos", WireKind::Enum(&["PRE", "POST"])))
        }
        _ if param.starts_with("eq") && mixable && !f.is_dca => eq_mapping(&r, &f, param),
        _ if param.starts_with("send") => send_mapping(&r, &f, sends_src, out_src, param),
        _ if param.starts_with("dca") && param.ends_with("_assign") => {
            group_bit(&r, &f, param, "grp/dca", 8)
        }
        _ if param.starts_with("_mg") => group_bit(&r, &f, param, "grp/mute", 6),
        "automix_group" => {
            anyhow::ensure!(
                f.is_ch && strip.0 < 8,
                "x32: automix group is channels 1..8 only"
            );
            Ok(leaf(&r, "automix/group", WireKind::Int(0, 2)))
        }
        "automix_weight" => {
            anyhow::ensure!(
                f.is_ch && strip.0 < 8,
                "x32: automix weight is channels 1..8 only"
            );
            Ok(leaf(&r, "automix/weight", WireKind::Linf(-12.0, 12.0)))
        }
        _ => anyhow::bail!("x32: unsupported {param:?} on {r}"),
    }
}

/// EQ band mapping: ch/aux/fxrtn 4 bands, bus/mtx/main 6 bands. The type
/// table is per family, not per band count: buses use the 6-option table
/// (`0..5`) on all six bands; matrices and mains use the 14-option table
/// (`0..13`, Butterworth/Bessel/Linkwitz-Riley variants on top).
fn eq_mapping(r: &str, f: &Fam, param: &str) -> anyhow::Result<Mapping> {
    let bands = if f.is_bus || f.is_mtx || f.is_main {
        6
    } else {
        4
    };
    let (n, leaf_name) = param[2..]
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("x32: invalid EQ parameter"))?;
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("x32: invalid EQ parameter"))?;
    anyhow::ensure!((1..=bands).contains(&n), "x32: EQ band out of range");
    let (suffix, kind) = match leaf_name {
        "gain" => ("g", WireKind::Linf(-15.0, 15.0)),
        "freq" => ("f", WireKind::Logf(20.0, 20000.0)),
        "q" => ("q", WireKind::Q),
        "type" => (
            "type",
            WireKind::Enum(if f.is_mtx || f.is_main {
                addrs::EQ_TYPES_6BAND
            } else {
                addrs::EQ_TYPES_4BAND
            }),
        ),
        _ => anyhow::bail!("x32: unsupported EQ parameter"),
    };
    Ok(leaf(r, &format!("eq/{n}/{suffix}"), kind))
}

/// Send mapping: ch/aux/fxrtn `send1..16` → bus 1..16; bus/main
/// `send17..22` → matrix 1..6 (distinct canonical slots so one global
/// destination identity never means bus and matrix at once). Odd sends
/// carry pan + tap type; even sends are level + on only. Input-side taps
/// offer six options (with `GRP`); bus/main matrix taps offer five (no
/// `GRP`, per the documented bus/main option sets).
fn send_mapping(
    r: &str,
    _f: &Fam,
    sends_src: bool,
    out_src: bool,
    param: &str,
) -> anyhow::Result<Mapping> {
    let (n, field) = param[4..]
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("x32: invalid send parameter"))?;
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("x32: invalid send parameter"))?;
    let (wire, matrix) = if sends_src {
        anyhow::ensure!(
            (1..=16).contains(&n),
            "x32: matrix sends are unavailable on inputs (send17..22 are bus/main only)"
        );
        (n, false)
    } else if out_src {
        anyhow::ensure!(
            (17..=22).contains(&n),
            "x32: bus destinations are unavailable on bus/main (send17..22 feed the matrices)"
        );
        (n - 16, true)
    } else {
        anyhow::bail!("x32: sends unavailable on this strip")
    };
    let odd = wire % 2 == 1;
    let wire_n = format!("{wire:02}");
    match field {
        "level" => Ok(leaf(r, &format!("mix/{wire_n}/level"), WireKind::LevelDb)),
        "on" => Ok(leaf(r, &format!("mix/{wire_n}/on"), WireKind::Bool)),
        "pan" if odd => Ok(leaf(
            r,
            &format!("mix/{wire_n}/pan"),
            WireKind::Linf(-100.0, 100.0),
        )),
        "tap" if odd => Ok(leaf(
            r,
            &format!("mix/{wire_n}/type"),
            WireKind::Enum(if matrix {
                addrs::MATRIX_SEND_TYPES
            } else {
                addrs::SEND_TYPES
            }),
        )),
        _ => anyhow::bail!("x32: unsupported send parameter"),
    }
}

/// DCA (`grp/dca`, 8 bits) and mute-group (`grp/mute`, 6 bits) bitmask
/// members. DCA masters carry no `grp` leaf.
fn group_bit(r: &str, f: &Fam, param: &str, leaf_name: &str, count: u8) -> anyhow::Result<Mapping> {
    anyhow::ensure!(!f.is_dca, "x32: group assignment unavailable on DCA");
    let n: u8 = if leaf_name == "grp/dca" {
        param
            .strip_prefix("dca")
            .and_then(|s| s.strip_suffix("_assign"))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("x32: invalid DCA assignment"))?
    } else {
        param
            .strip_prefix("_mg")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("x32: invalid mute-group assignment"))?
    };
    anyhow::ensure!((1..=count).contains(&n), "x32: group out of range");
    Ok(Mapping {
        path: format!("{r}/{leaf_name}"),
        kind: WireKind::Int(0, if count == 8 { 255 } else { 63 }),
    })
}

/// `/-ha/[00..39]/index` for a channel/aux strip (0..31 ch, 32..39 aux).
pub fn ha_index_addr(strip: StripId) -> anyhow::Result<String> {
    let (r, f) = fam_of(strip)?;
    let _ = r;
    if f.is_ch {
        Ok(format!("/-ha/{:02}/index", strip.0))
    } else if f.is_aux {
        Ok(format!("/-ha/{:02}/index", 32 + (strip.0 - 32)))
    } else {
        anyhow::bail!("x32: headamp routing is channel/aux only")
    }
}
