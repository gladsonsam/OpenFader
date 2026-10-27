//! Shared query/push codecs for the advertised input, dynamics and send leaves.
use super::*;

pub(super) fn decode(p: &str, msg: &OscMessage) -> anyhow::Result<ParamValue> {
    if matches!(p, "gate_on" | "comp_on") {
        return osc::is_on(msg).map(ParamValue::Bool);
    }
    if p.starts_with("gate_") {
        return match p {
            "gate_thr" => gate::wire_to_lin(
                osc::fader_value(msg)?,
                gate::GATE_THR_MIN_DB,
                gate::GATE_THR_MAX_DB,
                "thr",
            ),
            "gate_range" => gate::wire_to_lin(
                osc::fader_value(msg)?,
                gate::GATE_RANGE_MIN_DB,
                gate::GATE_RANGE_MAX_DB,
                "range",
            ),
            "gate_att" => gate::wire_to_lin(
                osc::fader_value(msg)?,
                gate::GATE_ATT_MIN_MS,
                gate::GATE_ATT_MAX_MS,
                "attack",
            ),
            "gate_hold" => gate::wire_to_log(
                osc::fader_value(msg)?,
                gate::GATE_HOLD_MIN_MS,
                gate::GATE_HOLD_MAX_MS,
                "hold",
            ),
            "gate_rel" => gate::wire_to_log(
                osc::fader_value(msg)?,
                gate::GATE_REL_MIN_MS,
                gate::GATE_REL_MAX_MS,
                "release",
            ),
            "gate_mode" => {
                gate::int_leaf_from_msg(msg, gate::GATE_MODE_MIN, gate::GATE_MODE_MAX, "mode")
                    .map(|i| i as f32)
            }
            "gate_key_src" => gate::int_leaf_from_msg(
                msg,
                gate::GATE_KEY_SRC_MIN,
                gate::GATE_KEY_SRC_MAX,
                "keysrc",
            )
            .map(|i| i as f32),
            "gate_filter_on" => return osc::is_on(msg).map(ParamValue::Bool),
            "gate_filter_type" => gate::int_leaf_from_msg(
                msg,
                gate::GATE_FILTER_TYPE_MIN,
                gate::GATE_FILTER_TYPE_MAX,
                "filter type",
            )
            .map(|i| i as f32),
            "gate_filter_freq" => gate::wire_to_filter_hz(osc::fader_value(msg)?),
            _ => anyhow::bail!("{}: unknown param {p:?}", "xair"),
        }
        .map(ParamValue::Float);
    }
    if p.starts_with("comp_") {
        return match p {
            "comp_thr" => comp::wire_to_lin(
                osc::fader_value(msg)?,
                comp::COMP_THR_MIN_DB,
                comp::COMP_THR_MAX_DB,
                "thr",
            ),
            "comp_ratio" => comp::wire_to_ratio(msg),
            "comp_knee" => comp::wire_to_lin(
                osc::fader_value(msg)?,
                comp::COMP_KNEE_MIN,
                comp::COMP_KNEE_MAX,
                "knee",
            ),
            "comp_makeup" => comp::wire_to_lin(
                osc::fader_value(msg)?,
                comp::COMP_MAKEUP_MIN_DB,
                comp::COMP_MAKEUP_MAX_DB,
                "makeup",
            ),
            "comp_att" => comp::wire_to_lin(
                osc::fader_value(msg)?,
                comp::COMP_ATT_MIN_MS,
                comp::COMP_ATT_MAX_MS,
                "attack",
            ),
            "comp_hold" => comp::wire_to_log(
                osc::fader_value(msg)?,
                comp::COMP_HOLD_MIN_MS,
                comp::COMP_HOLD_MAX_MS,
                "hold",
            ),
            "comp_rel" => comp::wire_to_log(
                osc::fader_value(msg)?,
                comp::COMP_REL_MIN_MS,
                comp::COMP_REL_MAX_MS,
                "release",
            ),
            "comp_mode" => {
                comp::int_leaf_from_msg(msg, comp::COMP_MODE_MIN, comp::COMP_MODE_MAX, "mode")
                    .map(|i| i as f32)
            }
            "comp_det" => {
                comp::int_leaf_from_msg(msg, comp::COMP_DET_MIN, comp::COMP_DET_MAX, "det")
                    .map(|i| i as f32)
            }
            "comp_env" => {
                comp::int_leaf_from_msg(msg, comp::COMP_ENV_MIN, comp::COMP_ENV_MAX, "env")
                    .map(|i| i as f32)
            }
            "comp_mix" => comp::wire_to_lin(
                osc::fader_value(msg)?,
                comp::COMP_MIX_MIN,
                comp::COMP_MIX_MAX,
                "mix",
            ),
            "comp_auto" => return osc::is_on(msg).map(ParamValue::Bool),
            "comp_key_src" => comp::int_leaf_from_msg(msg, 0, 22, "keysrc").map(|i| i as f32),
            "comp_filter_on" => return osc::is_on(msg).map(ParamValue::Bool),
            "comp_filter_type" => {
                comp::int_leaf_from_msg(msg, 0, 8, "filter type").map(|i| i as f32)
            }
            "comp_filter_freq" => gate::wire_to_filter_hz(osc::fader_value(msg)?),
            _ => anyhow::bail!("{}: unknown param {p:?}", "xair"),
        }
        .map(ParamValue::Float);
    }
    match p {
        "gate_on" | "comp_on" => osc::is_on(msg).map(ParamValue::Bool),
        "phantom" | "polarity" | "rtnsw" => osc::is_on(msg).map(ParamValue::Bool),
        "trim" => input::rtntrim_wire_to_db(osc::fader_value(msg)?).map(ParamValue::Float),
        "usbreturn" => input::usbreturn_from_msg(msg).map(|i| ParamValue::Float(i as f32)),
        "automix_weight" => automix::wire_to_weight(osc::fader_value(msg)?).map(ParamValue::Float),
        p if p.starts_with("send") => {
            let (_, leaf) = generic_send(p)?;
            match leaf {
                "level" => mix::wire_to_send_db(mix::send_level_value(msg)?).map(ParamValue::Float),
                "tap" => mix::tap_from_msg(msg).map(|i| ParamValue::Float(i as f32)),
                "pan" => mix::wire_to_pan(osc::fader_value(msg)?).map(ParamValue::Float),
                "grpon" => osc::is_on(msg).map(ParamValue::Bool),
                _ => anyhow::bail!("unsupported send leaf"),
            }
        }
        _ => anyhow::bail!("unknown shared decoder {p}"),
    }
}

pub(super) fn address(strip: u16, p: &str) -> Option<String> {
    let (kind, root) = strip_root(strip).ok()?;
    if p != "rtnsw" {
        check_scope(p, kind).ok()?;
    }
    if p == "rtnsw" && kind != openfader_core::StripKind::In {
        return None;
    }
    let suffix = match p {
        "usbreturn" => "config/rtnsrc",
        "trim" => "preamp/rtntrim",
        "polarity" => "preamp/invert",
        "rtnsw" => "preamp/rtnsw",
        "automix_weight" => "automix/weight",
        "gate_on" => "gate/on",
        "comp_on" => "dyn/on",
        "gate_thr" => return Some(gate::gate_thr(&root)),
        "gate_range" => return Some(gate::gate_range(&root)),
        "gate_att" => return Some(gate::gate_attack(&root)),
        "gate_hold" => return Some(gate::gate_hold(&root)),
        "gate_rel" => return Some(gate::gate_release(&root)),
        "gate_mode" => return Some(gate::gate_mode(&root)),
        "gate_key_src" => return Some(gate::gate_key_src(&root)),
        "gate_filter_on" => return Some(gate::gate_filter_on(&root)),
        "gate_filter_type" => return Some(gate::gate_filter_type(&root)),
        "gate_filter_freq" => return Some(gate::gate_filter_freq(&root)),
        "comp_thr" => return Some(comp::comp_thr(&root)),
        "comp_ratio" => return Some(comp::comp_ratio(&root)),
        "comp_knee" => return Some(comp::comp_knee(&root)),
        "comp_makeup" => return Some(comp::comp_makeup(&root)),
        "comp_att" => return Some(comp::comp_attack(&root)),
        "comp_hold" => return Some(comp::comp_hold(&root)),
        "comp_rel" => return Some(comp::comp_release(&root)),
        "comp_mode" => return Some(comp::comp_mode(&root)),
        "comp_det" => return Some(comp::comp_det(&root)),
        "comp_env" => return Some(comp::comp_env(&root)),
        "comp_mix" => return Some(comp::comp_mix(&root)),
        "comp_auto" => return Some(comp::comp_auto(&root)),
        "comp_key_src" => return Some(comp::comp_key_src(&root)),
        "comp_filter_on" => return Some(comp::comp_filter_on(&root)),
        "comp_filter_type" => return Some(comp::comp_filter_type(&root)),
        "comp_filter_freq" => return Some(comp::comp_filter_freq(&root)),
        p if p.starts_with("send") => {
            let (slot, leaf) = generic_send(p).ok()?;
            if !(1..=10).contains(&slot)
                || (leaf == "pan" && ![1, 3, 5].contains(&slot))
                || leaf == "on"
            {
                return None;
            }
            return Some(format!("{root}/mix/{slot:02}/{leaf}"));
        }
        _ => return None,
    };
    Some(format!("{root}/{suffix}"))
}

/// Constant-time routing to shared codecs. Build only driver-supported scopes.
pub(super) fn inbound(msg: &OscMessage) -> Vec<(u16, &'static str, ParamValue)> {
    type Routes = std::collections::HashMap<String, Vec<(u16, &'static str)>>;
    static ROUTES: std::sync::OnceLock<Routes> = std::sync::OnceLock::new();
    let routes = ROUTES.get_or_init(|| {
        let mut routes: Routes = Default::default();
        for strip in 0..XAIR_STRIPS as u16 {
            for &p in XAIR_CAPABILITIES.iter().chain(std::iter::once(&"rtnsw")) {
                if let Some(addr) = address(strip, p) {
                    routes.entry(addr).or_default().push((strip, p));
                }
            }
        }
        routes
    });
    routes
        .get(&msg.addr)
        .into_iter()
        .flatten()
        .filter_map(|&(strip, p)| decode(p, msg).ok().map(|v| (strip, p, v)))
        .collect()
}

/// Typed OSC containers, in captured firmware field order. Unknown fields
/// (e.g. config source and EQ GEQ mode) are intentionally not assigned IDs.
pub(super) fn expand(msg: &OscMessage) -> Vec<OscMessage> {
    let addr = &msg.addr;
    let Some((root, block)) = addr.rsplit_once('/') else {
        return Vec::new();
    };
    let leaves: &[&str] = match block {
        "config" if root.starts_with("/ch/") => &["name", "color", "", "rtnsrc"],
        "config" => &["name", "color"],
        "preamp" if root.starts_with("/ch/") => &["rtntrim", "rtnsw", "invert", "hpon", "hpf"],
        "gate" => &[
            "on", "mode", "thr", "range", "attack", "hold", "release", "keysrc",
        ],
        "dyn" => &[
            "on", "mode", "det", "env", "thr", "ratio", "knee", "mgain", "attack", "hold",
            "release", "mix", "auto", "keysrc",
        ],
        "filter" if root.ends_with("/dyn") || root.ends_with("/gate") => &["on", "type", "f"],
        "eq" => &["on"],
        "mix" if root == "/lr" => &["on", "fader"],
        "mix" => &["on", "fader", "lr", "pan"],
        "insert" if !root.starts_with("/fx/") => &["on", "sel"],
        "grp" => &["dca", "mute"],
        n if n.parse::<u8>().is_ok() && root.ends_with("/eq") => &["type", "f", "g", "q"],
        n if n.parse::<u8>().is_ok() && root.ends_with("/mix") => &["level", "grpon", "tap", "pan"],
        n if n.parse::<u8>().is_ok() && root == "/headamp" => &["gain", "phantom"],
        n if n.parse::<u8>().is_ok() && root == "/dca" => &["on", "fader"],
        _ => &[],
    };
    leaves
        .iter()
        .zip(&msg.args)
        .filter(|(leaf, _)| !leaf.is_empty())
        .map(|(leaf, value)| OscMessage {
            addr: format!("{addr}/{leaf}"),
            args: vec![value.clone()],
        })
        .collect()
}
