//! Decode desk-originated parameter updates.
use super::*;

pub(super) const LR_STRIP: u16 = 30;

/// Canonical catalogue id per send slot (bus slots 1..=6 ride `auxN_level`,
/// FX slots 7..=10 ride `fxN_level` — the same ids `get_param` decodes).
pub(super) const AUX_LEVEL_IDS: [&str; 6] = [
    "aux1_level",
    "aux2_level",
    "aux3_level",
    "aux4_level",
    "aux5_level",
    "aux6_level",
];
pub(super) const FX_LEVEL_IDS: [&str; 4] = ["fx1_level", "fx2_level", "fx3_level", "fx4_level"];
/// Canonical catalogue ids per DCA-assign bit (`dca1..4_assign`, the same
/// ids the `dcaN_assign` get_param arms decode — for `parse_inbound`).
pub(super) const DCA_ASSIGN_IDS: [&str; 4] =
    ["dca1_assign", "dca2_assign", "dca3_assign", "dca4_assign"];

/// Canonical catalogue ids per EQ band leaf (bands 1..=6 index 0..=5) —
/// the same ids `get_param`'s eq arms decode, for `parse_inbound`.
pub(super) const EQ_GAIN_IDS: [&str; 6] = [
    "eq1_gain", "eq2_gain", "eq3_gain", "eq4_gain", "eq5_gain", "eq6_gain",
];
pub(super) const EQ_FREQ_IDS: [&str; 6] = [
    "eq1_freq", "eq2_freq", "eq3_freq", "eq4_freq", "eq5_freq", "eq6_freq",
];
pub(super) const EQ_Q_IDS: [&str; 6] = ["eq1_q", "eq2_q", "eq3_q", "eq4_q", "eq5_q", "eq6_q"];
pub(super) const EQ_TYPE_IDS: [&str; 6] = [
    "eq1_type", "eq2_type", "eq3_type", "eq4_type", "eq5_type", "eq6_type",
];

/// Decode advertised XR18 controls in engineering units, including query
/// replies and captured typed containers. Dynamics/input/send leaves share
/// their query codecs. One send level fans out to canonical and legacy IDs.
/// Private group IDs are consumed by the engine's typed library fanout;
/// FX is stateful (its type selects each parameter codec) and is decoded by
/// the per-session receive actor. Unknown/unmapped paths remain absent.
pub fn parse_inbound(msg: &OscMessage) -> Vec<(u16, &'static str, ParamValue)> {
    if let Some((pair, linked)) = super::assignments::parse_channel_link(msg) {
        let left = (pair * 2) as u16;
        return vec![
            (left, "link", ParamValue::Bool(linked)),
            (left + 1, "link", ParamValue::Bool(linked)),
        ];
    }
    let shared = sync_decode::inbound(msg);
    if !shared.is_empty() {
        let mut shared = shared;
        if let Some((strip, id, value)) = shared.first().cloned() {
            if let Ok((slot, "level")) = generic_send(id) {
                let alias = if slot <= 6 {
                    AUX_LEVEL_IDS[slot as usize - 1]
                } else {
                    FX_LEVEL_IDS[slot as usize - 7]
                };
                // Keep the legacy update first for existing consumers.
                shared.insert(0, (strip, alias, value));
            }
        }
        return shared;
    }
    let expanded = sync_decode::expand(msg);
    if !expanded.is_empty() {
        return expanded.iter().flat_map(parse_inbound).collect();
    }
    let mut updates = parse_inbound_go(msg).unwrap_or_default();
    // One wire level has both canonical and compatibility IDs.
    if let Some((strip, id, value)) = updates.first().cloned() {
        let slot = AUX_LEVEL_IDS
            .iter()
            .position(|p| *p == id)
            .map(|i| i + 1)
            .or_else(|| FX_LEVEL_IDS.iter().position(|p| *p == id).map(|i| i + 7));
        if let Some(slot) = slot {
            const IDS: [&str; 10] = [
                "send1_level",
                "send2_level",
                "send3_level",
                "send4_level",
                "send5_level",
                "send6_level",
                "send7_level",
                "send8_level",
                "send9_level",
                "send10_level",
            ];
            updates.push((strip, IDS[slot - 1], value));
        }
    }
    updates
}

pub(super) fn parse_inbound_go(msg: &OscMessage) -> Option<Vec<(u16, &'static str, ParamValue)>> {
    let addr = msg.addr.strip_prefix('/')?;
    if let Some(group) = addr
        .strip_prefix("config/mute/")
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| (1..=4).contains(n))
    {
        return Some(vec![(
            group - 1,
            "_mute_group",
            ParamValue::Bool(osc::is_on(msg).ok()?),
        )]);
    }
    if let Some(root) = addr.strip_suffix("/grp/mute") {
        let strip = strip_of_root(&format!("/{root}"))?;
        if !matches!(
            strip_root(strip).ok()?.0,
            openfader_core::StripKind::In
                | openfader_core::StripKind::Ret
                | openfader_core::StripKind::Bus
                | openfader_core::StripKind::FxSnd
        ) {
            return None;
        }
        let mask = groups::mask_from_msg(msg).ok()?;
        const IDS: [&str; 4] = ["_mg1", "_mg2", "_mg3", "_mg4"];
        return Some(
            IDS.iter()
                .enumerate()
                .map(|(i, id)| (strip, *id, ParamValue::Bool(mask & (1 << i) != 0)))
                .collect(),
        );
    }
    if addr == "-stat/solosw" && msg.args.len() >= addrs::XR18_CHANNELS as usize {
        return Some(
            msg.args
                .iter()
                .take(16)
                .enumerate()
                .filter_map(|(i, arg)| match arg {
                    OscType::Int(v) => Some((i as u16, "solo", ParamValue::Bool(*v != 0))),
                    _ => None,
                })
                .collect(),
        );
    }

    // Solo switches (`/-stat/solosw/NN`, 1-based channel bank): the desk
    // broadcasts these on solo changes (surface, another OSC client, or our own
    // `set_solo` — all drive the same bank). Decoded to the `solo` poll
    // id so the engine can fan them out as `Solo` events; out-of-range
    // leaves stay silent (XR18: channels 1..=16 only).
    if let Some(rest) = addr.strip_prefix("-stat/solosw/") {
        let Ok(n): Result<u16, _> = rest.parse() else {
            return None;
        };
        if !(1..=addrs::XR18_CHANNELS).contains(&n) {
            return None;
        }
        let Ok(on) = osc::int_arg(msg) else {
            return None;
        };
        return Some(vec![(n - 1, "solo", ParamValue::Bool(on != 0))]);
    }
    // DCA direct forms first (`/dca/N/fader`, `/dca/N/on` — no `/mix`).
    if let Some(rest) = addr.strip_prefix("dca/") {
        let (n, leaf) = rest.split_once('/')?;
        let strip = strip_of_root(&format!("/dca/{n}"))?;
        if leaf == "config/name" {
            return Some(vec![(
                strip,
                "name",
                ParamValue::Text(osc::string_args(msg).into_iter().next()?),
            )]);
        }
        if leaf == "config/color" {
            let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
            return Some(vec![(strip, "color", ParamValue::Float(i as f32))]);
        }
        return match leaf {
            "fader" => Some(vec![(
                strip,
                "fader",
                ParamValue::Float(osc::fader_value(msg).ok()?),
            )]),
            "on" => Some(vec![(
                strip,
                "mute",
                ParamValue::Bool(osc::mute_is_muted(msg).ok()?),
            )]),
            _ => None,
        };
    }
    // Main LR (`/lr/mix/fader`, `/lr/mix/on`, `/lr/config/name`).
    if addr == "lr/mix/fader" {
        return Some(vec![(
            LR_STRIP,
            "fader",
            ParamValue::Float(osc::fader_value(msg).ok()?),
        )]);
    }
    if addr == "lr/mix/on" {
        return Some(vec![(
            LR_STRIP,
            "mute",
            ParamValue::Bool(osc::mute_is_muted(msg).ok()?),
        )]);
    }
    if addr == "lr/config/name" {
        return Some(vec![(
            LR_STRIP,
            "name",
            ParamValue::Text(osc::string_args(msg).into_iter().next()?),
        )]);
    }
    if addr == "lr/config/color" {
        let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
        return Some(vec![(LR_STRIP, "color", ParamValue::Float(i as f32))]);
    }
    // Family roots: `/ch/NN`, `/rtn/N`, `/bus/N`, `/fxsend/N`, with the
    // mix block (`…/mix/fader|on|pan`), a send slot (`…/mix/MM/level`) or a
    // config name/color (`…/config/name|color`).
    let (head, leaf) = addr.rsplit_once('/')?;
    // Send level (`/ch/NN/mix/MM/level`, slots 1..=10): channel roots only
    // (the `get_param` send arms are channel-scoped), decoded like the
    // `aux/fx/send…_level` arms (X-Air fader law, dB).
    if leaf == "level" {
        let (mix_root, slot) = head.rsplit_once('/')?;
        let (root, block) = mix_root.rsplit_once('/')?;
        if block != "mix" {
            return None;
        }
        let ch: u16 = root.strip_prefix("ch/")?.parse().ok()?;
        if !(1..=addrs::XR18_CHANNELS).contains(&ch) {
            return None;
        }
        let slot: usize = slot.parse().ok()?;
        let id = match slot {
            1..=6 => AUX_LEVEL_IDS[slot - 1],
            7..=10 => FX_LEVEL_IDS[slot - 7],
            _ => return None,
        };
        let db = mix::wire_to_send_db(mix::send_level_value(msg).ok()?).ok()?;
        return Some(vec![(ch - 1, id, ParamValue::Float(db))]);
    }
    // Input HPF leaves broadcast independently of EQ bypass. Decode with
    // the same input codecs as get_param; do not wait for a cached poll.
    if matches!(leaf, "hpon" | "hpf") {
        let (root, block) = head.rsplit_once('/')?;
        if block != "preamp" {
            return None;
        }
        let ch: u16 = root.strip_prefix("ch/")?.parse().ok()?;
        if !(1..=addrs::XR18_CHANNELS).contains(&ch) {
            return None;
        }
        let (id, value) = if leaf == "hpon" {
            ("hpf_on", ParamValue::Bool(osc::is_on(msg).ok()?))
        } else {
            (
                "hpf_freq",
                ParamValue::Float(input::hpf_wire_to_hz(osc::fader_value(msg).ok()?).ok()?),
            )
        };
        return Some(vec![(ch - 1, id, value)]);
    }
    // EQ on/off (`{root}/eq/on`): channels, FX returns, buses and main LR
    // — decoded like the `get_param` `eq_on` arm. FX-send/DCA roots carry
    // no EQ and stay silent.
    if let Some(eq_root) = addr.strip_suffix("/eq/on") {
        if eq_root != "lr"
            && !eq_root.starts_with("ch/")
            && !eq_root.starts_with("rtn/")
            && !eq_root.starts_with("bus/")
        {
            return None;
        }
        let strip = if eq_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{eq_root}"))?
        };
        return Some(vec![(
            strip,
            "eq_on",
            ParamValue::Bool(osc::is_on(msg).ok()?),
        )]);
    }
    // Insert bypass/select (`{root}/insert/on|sel`): channels, buses and
    // main LR only — returns, FX sends and masters carry no insert leaf
    // (the desk is silent there). Decoded like the `get_param`
    // `insert_on`/`insert_sel` arms (raw `0..=8` sel, no slot mapping).
    if let Some(ins_root) = addr.strip_suffix("/insert/on") {
        if ins_root != "lr" && !ins_root.starts_with("ch/") && !ins_root.starts_with("bus/") {
            return None;
        }
        let strip = if ins_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{ins_root}"))?
        };
        return Some(vec![(
            strip,
            "insert_on",
            ParamValue::Bool(osc::is_on(msg).ok()?),
        )]);
    }
    if let Some(ins_root) = addr.strip_suffix("/insert/sel") {
        if ins_root != "lr" && !ins_root.starts_with("ch/") && !ins_root.starts_with("bus/") {
            return None;
        }
        let strip = if ins_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{ins_root}"))?
        };
        let sel = insert::sel_from_msg(msg).ok()? as f32;
        return Some(vec![(strip, "insert_sel", ParamValue::Float(sel))]);
    }
    // EQ band leaves (`{root}/eq/{band}/{g|f|q|type}`): decoded like the
    // matching `get_param` eq arms (same codecs, engineering units).
    // Channels and FX returns carry 4 bands, buses and main LR 6
    // (mirroring `check_eq_band`); FX-send/DCA roots carry no EQ and stay
    // silent, as do out-of-range bands on a 4-band strip.
    if matches!(leaf, "g" | "f" | "q" | "type") {
        let (eq_parent, band_s) = head.rsplit_once('/')?;
        let eq_root = eq_parent.strip_suffix("/eq")?;
        let max_band = if eq_root == "lr" {
            6
        } else if eq_root.starts_with("ch/") || eq_root.starts_with("rtn/") {
            4
        } else if eq_root.starts_with("bus/") {
            6
        } else {
            return None;
        };
        let band: u8 = band_s.parse().ok()?;
        if !(1..=max_band).contains(&band) {
            return None;
        }
        let strip = if eq_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{eq_root}"))?
        };
        let idx = band as usize - 1;
        let id = match leaf {
            "g" => EQ_GAIN_IDS[idx],
            "f" => EQ_FREQ_IDS[idx],
            "q" => EQ_Q_IDS[idx],
            _ => EQ_TYPE_IDS[idx],
        };
        let value = match leaf {
            "g" => ParamValue::Float(eq::wire_to_gain_db(osc::fader_value(msg).ok()?).ok()?),
            "f" => ParamValue::Float(eq::wire_to_freq_hz(osc::fader_value(msg).ok()?).ok()?),
            "q" => ParamValue::Float(eq::wire_to_q(osc::fader_value(msg).ok()?).ok()?),
            _ => ParamValue::Float(eq::eqtype_from_msg(msg).ok()? as f32),
        };
        return Some(vec![(strip, id, value)]);
    }
    // Group assignment (`{root}/grp/dca`, int bitmask): one update per DCA
    // catalogue id (same decode as the `dcaN_assign` get_param arms).
    // `grp/mute` carries no catalogue ids (the engine tracks mute-group
    // membership through `MuteGroupAssign` events) and stays out — see the
    // fn docs. DCA/main roots carry no `grp` leaf on the desk; a message
    // from anywhere else stays silent.
    if let Some(grp_root) = addr.strip_suffix("/grp/dca") {
        let strip = strip_of_root(&format!("/{grp_root}"))?;
        let (kind, _) = strip_root(strip).ok()?;
        if !matches!(
            kind,
            openfader_core::StripKind::In
                | openfader_core::StripKind::Ret
                | openfader_core::StripKind::Bus
                | openfader_core::StripKind::FxSnd
        ) {
            return None;
        }
        let mask = groups::mask_from_msg(msg).ok()?;
        return Some(
            DCA_ASSIGN_IDS
                .iter()
                .enumerate()
                .map(|(i, id)| (strip, *id, ParamValue::Bool(mask & (1 << i) != 0)))
                .collect(),
        );
    }
    // Native leaf edits can be broadcast independently of the container.
    if let Some(am_root) = addr.strip_suffix("/automix/group") {
        let strip = strip_of_root(&format!("/{am_root}"))?;
        if strip_root(strip).ok()?.0 != openfader_core::StripKind::In {
            return None;
        }
        let [rosc::OscType::Int(group @ 0..=2)] = msg.args.as_slice() else {
            return None;
        };
        return Some(vec![
            (strip, "automix_on", ParamValue::Bool(*group != 0)),
            (strip, "automix_group", ParamValue::Float(*group as f32)),
        ]);
    }
    if let Some(am_root) = addr.strip_suffix("/automix/weight") {
        let strip = strip_of_root(&format!("/{am_root}"))?;
        if strip_root(strip).ok()?.0 != openfader_core::StripKind::In {
            return None;
        }
        let weight = automix::wire_to_weight(osc::fader_value(msg).ok()?).ok()?;
        return Some(vec![(strip, "automix_weight", ParamValue::Float(weight))]);
    }
    // Automix container (`{root}/automix`, `(int, float)` = enable +
    // weight): one update per leaf (same decode as the `automix_on` /
    // `automix_weight` get_param arms). Channel strips only — the desk is
    // silent on `automix` everywhere else.
    if let Some(am_root) = addr.strip_suffix("/automix") {
        let strip = strip_of_root(&format!("/{am_root}"))?;
        let (kind, _) = strip_root(strip).ok()?;
        if kind != openfader_core::StripKind::In {
            return None;
        }
        let (group, w) = automix::automix_group_from_msg(msg).ok()?;
        return Some(vec![
            (strip, "automix_on", ParamValue::Bool(group != 0)),
            (strip, "automix_weight", ParamValue::Float(w)),
            (strip, "automix_group", ParamValue::Float(group as f32)),
        ]);
    }
    let (root, leaf) = match leaf {
        "fader" | "on" | "pan" | "lr" => {
            let (root, block) = head.rsplit_once('/')?;
            if block != "mix" {
                return None;
            }
            (root, leaf)
        }
        "name" | "color" => {
            let (root, block) = head.rsplit_once('/')?;
            if block != "config" {
                return None;
            }
            (root, leaf)
        }
        _ => return None,
    };
    let strip = strip_of_root(&format!("/{root}"))?;
    match leaf {
        "fader" => Some(vec![(
            strip,
            "fader",
            ParamValue::Float(osc::fader_value(msg).ok()?),
        )]),
        "on" => Some(vec![(
            strip,
            "mute",
            ParamValue::Bool(osc::mute_is_muted(msg).ok()?),
        )]),
        "pan" => {
            let pan = mix::wire_to_pan(osc::fader_value(msg).ok()?).ok()?;
            Some(vec![(strip, "pan", ParamValue::Float(pan))])
        }
        // LR assign (`{root}/mix/lr`, int; same decode as `get_param`).
        // Main LR never reaches here (`/lr` has no `lr` field, and
        // `strip_of_root("/lr")` is `None` anyway).
        "lr" => Some(vec![(
            strip,
            "lr_assign",
            ParamValue::Bool(osc::is_on(msg).ok()?),
        )]),
        "name" => Some(vec![(
            strip,
            "name",
            ParamValue::Text(osc::string_args(msg).into_iter().next()?),
        )]),
        "color" => {
            let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
            Some(vec![(strip, "color", ParamValue::Float(i as f32))])
        }
        _ => None,
    }
}
