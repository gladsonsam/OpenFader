//! Strip addressing, parameter validation, and send identifiers.
use super::*;

/// Parse an `eq{b}_{leaf}` param id into its 1-based band and leaf.
/// Bands 1..=6 parse here; the per-kind band cap (4 on channels/returns,
/// 6 on buses/main) is enforced by [`check_eq_band`] after the kind is
/// known, so out-of-range bands on a desk family error instead of
/// hitting a silent address.
pub(super) fn eq_leaf(param: &str) -> anyhow::Result<(u8, &str)> {
    let rest = param
        .strip_prefix("eq")
        .ok_or_else(|| anyhow::anyhow!("xair: not an eq param {param:?}"))?;
    let (band, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let band: u8 = band
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    eq::check_band_wide(band)?;
    Ok((band, leaf))
}

/// Enforce the firmware band count for one strip kind: channels and aux
/// returns carry 4 bands, buses and main LR 6 (`1..n: 4 4 6 6`).
pub(super) fn check_eq_band(kind: openfader_core::StripKind, band: u8) -> anyhow::Result<()> {
    use openfader_core::StripKind as K;
    let max = match kind {
        K::In | K::Ret => 4,
        _ => 6,
    };
    if band < 1 || band > max {
        anyhow::bail!("xair: eq band {band} out of range 1..={max} for {kind:?} strips");
    }
    Ok(())
}

pub(super) fn aux_send(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("aux")
        .ok_or_else(|| anyhow::anyhow!("xair: not an aux param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "level" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::AUX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n)
}

/// Parse a generic `send{N}_{leaf}` param id into its send number (1..=10)
/// and leaf (`level|tap|pan|grpon|on`).
pub(super) fn generic_send(param: &str) -> anyhow::Result<(u8, &str)> {
    let rest = param
        .strip_prefix("send")
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::AUX_SENDS + mix::FX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    if !matches!(leaf, "level" | "tap" | "pan" | "grpon" | "on") {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok((n, leaf))
}

pub(super) fn dca_assign(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("dca")
        .ok_or_else(|| anyhow::anyhow!("xair: not a dca param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "assign" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=groups::DCA_GROUPS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n - 1)
}

/// Global strip index to (family kind, OSC root). Channels keep the
/// `/ch/NN` root (T3); FX returns live at `/rtn/N` (NOT `/fxrtn/N` — the
/// desk is silent there); the stereo aux input lives at `/rtn/aux` (NOT
/// `/aux/*` or `/auxin/*` — the desk is silent there, owner-rig verified
/// 2026-10-20); buses are single-digit `/bus/N` (T2); DCA strips
/// are `/dca/N` with fader/on directly underneath (no `/mix` segment).
/// Every root below is live-verified on the owner rig (FW 1.20).
pub(super) fn strip_root(strip: u16) -> anyhow::Result<(openfader_core::StripKind, String)> {
    use openfader_core::StripKind as K;
    let s = strip as usize;
    if s < 16 {
        return Ok((K::In, format!("/ch/{:02}", s + 1)));
    }
    if (16..20).contains(&s) {
        return Ok((K::Ret, format!("/rtn/{}", s - 16 + 1)));
    }
    if (20..26).contains(&s) {
        return Ok((K::Bus, format!("/bus/{}", s - 20 + 1)));
    }
    if (26..30).contains(&s) {
        return Ok((K::FxSnd, format!("/fxsend/{}", s - 26 + 1)));
    }
    if s == 30 {
        return Ok((K::Main, "/lr".to_string()));
    }
    if (31..35).contains(&s) {
        return Ok((K::Dca, format!("/dca/{}", s - 31 + 1)));
    }
    if s == 35 {
        return Ok((K::Ret, "/rtn/aux".to_string()));
    }
    anyhow::bail!("channel strip {strip} out of range 0..=35 (0-based)")
}

/// Scope-check one catalogue id against a strip kind: out-of-scope is a
/// NON-drop-class error (the engine pre-filters its poll set by scopes, so
/// this only fires on direct misuse and must never shrink `caps`).
pub(super) fn check_scope(param: &str, kind: openfader_core::StripKind) -> anyhow::Result<()> {
    let def = openfader_core::params::find(param)
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let fx_send_dca = kind == openfader_core::StripKind::FxSnd
        && matches!(
            param,
            "dca1_assign" | "dca2_assign" | "dca3_assign" | "dca4_assign"
        );
    if (!openfader_core::params::supports_kind(def, kind) && !fx_send_dca)
        || (param == "comp_key_src" && kind == openfader_core::StripKind::Main)
    {
        anyhow::bail!("xair: {param} out of scope for {kind:?} strips");
    }
    Ok(())
}

/// Mix fader address for one strip root (`/lr` keeps its dedicated leaf;
/// DCA strips address `/dca/N/fader` directly — there is no `/mix`
/// segment there and the desk stays silent on it).
pub(super) fn strip_mix_fader(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_FADER.to_string();
    }
    if root.starts_with("/dca/") {
        return format!("{root}/fader");
    }
    format!("{root}/mix/fader")
}

/// Mix on/mute address for one strip root (DCA: `/dca/N/on` directly).
pub(super) fn strip_mix_on(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_ON.to_string();
    }
    if root.starts_with("/dca/") {
        return format!("{root}/on");
    }
    format!("{root}/mix/on")
}

pub(super) fn strip_config_name(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_NAME.to_string();
    }
    format!("{root}/config/name")
}

/// Reverse of [`strip_root`]: an OSC root back to its global strip index.
/// Returns `None` for roots outside the XR18 map. The stereo aux input
/// (`/rtn/aux`) is strip 35; numeric `/rtn/N` covers the four FX returns.
pub(super) fn strip_of_root(root: &str) -> Option<u16> {
    if root == "/rtn/aux" {
        return Some(35);
    }
    let path = root.strip_prefix('/')?;
    let (fam, n) = path.split_once('/')?;
    let n: usize = n.parse().ok()?;
    match fam {
        "ch" if (1..=16).contains(&n) => Some((n - 1) as u16),
        "rtn" if (1..=4).contains(&n) => Some((15 + n) as u16),
        "bus" if (1..=6).contains(&n) => Some((19 + n) as u16),
        "fxsend" if (1..=4).contains(&n) => Some((25 + n) as u16),
        "dca" if (1..=4).contains(&n) => Some((30 + n) as u16),
        _ => None,
    }
}

pub(super) fn fx_send(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("fx")
        .ok_or_else(|| anyhow::anyhow!("xair: not an fx param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "level" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::FX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n)
}
