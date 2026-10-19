//! Canonical parameter catalogue (v2) for the param plane.
//!
//! Every driver-backed control the UI may show has one entry here: id, value
//! kind, strip scopes, engineering-unit range, unit label, default and
//! display label. Capabilities ([`MixerDriver::capabilities`](crate::MixerDriver::capabilities))
//! are subsets of these ids; the relay `caps` frame renders this table
//! filtered by the active driver, and `params` frames carry values in these
//! engineering units (dB, Hz, ratio-as-number) — never wire floats.
//!
//! ## Scopes (v2)
//!
//! Each entry carries `scopes: &[StripKind]` (which strip families it applies
//! to). The poller/snapshot iterate topology families and only query
//! (strip, id) pairs where the id's scopes contain the strip's kind, so e.g.
//! DCA strips (fader/mute/name/color only) never get EQ queries. The legacy
//! single `scope` field is kept for back-compat in the JSON `caps` frame
//! (`channel|bus|main`): `channel` when scopes contain `in`, else `bus` when
//! they contain `bus`, else `main`.
//!
//! ## Send naming migration
//!
//! The v1 `aux1..8_level/mute` + `fx1..4_level/mute` ids are kept verbatim so
//! the v3 UI keeps working. The v2 generic `send1..16_level/tap/pan/on` ids
//! are the forward path (over the topology bus+fxsnd families). On the X-Air
//! `send1..6` alias the six bus-send wire slots (same as `aux1..6`) and
//! `send7..10` alias the four FX-send slots (same as `fx1..4`); `send11..16`
//! have no X-Air wire slot and are sim-only. Sim stores both id sets
//! independently (both read/write); drivers document the alias.
//!
//! Ranges follow the real hardware (X-Air), NOT the old DL32S-flavoured UI
//! guesses in `apps/web/src/lib/sections.ts`. Known divergences from that
//! file (the UI v3 MS2 step follows this catalogue):
//!
//! * `gain`: `-12..=+60` dB here vs `-20..=60` there (X-Air headamp floor).
//! * `pan`: `-100..=+100` here vs `-1..=+1` there (X-Air `mix/pan`).
//! * `eqN_gain`: `-15..=+15` dB here vs `+-20` there (X-Air `eq/g`).
//! * `eqN_q`: `0.3..=10` here vs `0.1..=20` there (X-Air `eq/q`).
//! * `eq1_type`: exists here (X-Air bands 1..4 all carry `type 0..=5`);
//!   sections.ts omits band 1 (true only of the DL32S layout).
//! * `eqN_type`: `0..=5` here vs `0..=3` there (six X-Air filter types).
//! * `auxN_level` / `fxN_level` / `sendN_level`: `-90..=+10` dB here vs
//!   `-120..=+10` there (X-Air `level(161)` fader law; `-90` reads as `-inf`).
//! * `comp_ratio`: `1.1..=100` here (desk step-table ratio number, float).
//! * `color`: `0..=15` desk channel colour enum (name-bar colour).
//! * `icon`: `0..=31` desk channel icon enum.
//! * Gate/comp full sets: X-Air community-documented (firmware map + X32 OSC
//!   doc + xair-api), UNVERIFIED-ON-RIG — see `crates/proto-xair` docs.

use crate::{ParamValue, StripKind};

/// Value kind of one catalogue entry, as rendered in the relay `caps` frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Float,
    Bool,
    Enum,
    Text,
}

impl ParamKind {
    /// Wire name used in the `caps` frame (`float|bool|enum|text`).
    pub const fn as_str(self) -> &'static str {
        match self {
            ParamKind::Float => "float",
            ParamKind::Bool => "bool",
            ParamKind::Enum => "enum",
            ParamKind::Text => "text",
        }
    }
}

/// Legacy single-scope tag, kept for back-compat in the JSON `caps` frame
/// (`channel|bus|main`). New code uses [`ParamDef::scopes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamScope {
    Channel,
    Bus,
    Main,
}

impl ParamScope {
    /// Wire name used in the `caps` frame (`channel|bus|main`).
    pub const fn as_str(self) -> &'static str {
        match self {
            ParamScope::Channel => "channel",
            ParamScope::Bus => "bus",
            ParamScope::Main => "main",
        }
    }
}

/// Scope sets shared by catalogue rows.
pub const SCOPES_ALL: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::FxSnd,
    StripKind::Mtx,
    StripKind::Main,
    StripKind::Dca,
];
/// Input-channel-only ids (preamp, routing, sends source side).
pub const SCOPES_INPUT: &[StripKind] = &[StripKind::In];
/// Gate ids: the desk carries a gate block on channel strips only
/// (firmware availability column: `gate/` ch-only; buses/LR run dyn+EQ).
pub const SCOPES_GATE: &[StripKind] = &[StripKind::In];
/// Dynamics ids: channel strips, buses and main LR
/// (firmware availability: `dyn/` ch/bus/lr).
pub const SCOPES_DYN: &[StripKind] = &[StripKind::In, StripKind::Bus, StripKind::Main];
/// EQ ids: channels + aux returns carry 4 bands, buses + main LR 6
/// (firmware `1..n: 4 4 6 6` under ch/rtn/bus/lr). The band count is
/// enforced per kind by the X-Air driver, not by scopes.
pub const SCOPES_EQ: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Main,
];
/// EQ/dynamics ids: every mixable kind except DCA (DCA is fader/mute/name/color
/// only) and FX-send masters (fader/mute/name/color only on the desk).
/// Kept for ids whose firmware availability is the full mixable set
/// (`eq/on`, strip look); gate/comp/EQ bands use the narrower sets above.
pub const SCOPES_EQDYN: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Mtx,
    StripKind::Main,
];
/// Bus/main-only EQ bands 5..6 (channels + aux returns stop at 4).
pub const SCOPES_EQ56: &[StripKind] = &[StripKind::Bus, StripKind::Main];
/// Pan: everywhere a pan control exists (no DCA, no main).
pub const SCOPES_PAN: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Mtx,
];
/// Generic sends originate from input channels (sim seeds them there).
pub const SCOPES_SEND: &[StripKind] = &[StripKind::In];

/// Reset/unknown-look value of one catalogue entry. Const-friendly
/// (unlike [`ParamValue`], whose `Text` owns a `String`); convert with
/// [`ParamDefault::as_param_value`] when a live value is needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamDefault {
    Float(f32),
    Bool(bool),
    Text(&'static str),
}

impl ParamDefault {
    /// Materialize the default as a live [`ParamValue`].
    pub fn as_param_value(self) -> ParamValue {
        match self {
            ParamDefault::Float(v) => ParamValue::Float(v),
            ParamDefault::Bool(v) => ParamValue::Bool(v),
            ParamDefault::Text(v) => ParamValue::Text(v.to_string()),
        }
    }
}

/// One row of the canonical catalogue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamDef {
    /// Stable id (`gain`, `eq2_freq`, `aux1_level`, `send3_tap`, …).
    pub id: &'static str,
    pub kind: ParamKind,
    /// Legacy single scope, kept for back-compat JSON (`scope` in `caps`).
    pub scope: ParamScope,
    /// Which strip kinds this id applies to (v2; the poller filters on this).
    pub scopes: &'static [StripKind],
    /// Engineering-unit range (inclusive). `0..=0` for bool/text.
    pub min: f32,
    /// Engineering-unit range (inclusive). `0..=0` for bool/text.
    pub max: f32,
    /// Display unit (`dB`, `Hz`, …; `""` when unitless).
    pub unit: &'static str,
    /// Reset/unknown-look value. Enum defaults are whole-number floats.
    pub default: ParamDefault,
    /// Short display label.
    pub label: &'static str,
}

/// Whether catalogue entry `def` applies to strips of `kind`.
pub fn supports_kind(def: &ParamDef, kind: StripKind) -> bool {
    def.scopes.contains(&kind)
}

const fn f(
    id: &'static str,
    min: f32,
    max: f32,
    unit: &'static str,
    default: f32,
    label: &'static str,
    scopes: &'static [StripKind],
    scope: ParamScope,
) -> ParamDef {
    ParamDef {
        id,
        kind: ParamKind::Float,
        scope,
        scopes,
        min,
        max,
        unit,
        default: ParamDefault::Float(default),
        label,
    }
}

const fn b(
    id: &'static str,
    label: &'static str,
    scopes: &'static [StripKind],
    scope: ParamScope,
    default: bool,
) -> ParamDef {
    ParamDef {
        id,
        kind: ParamKind::Bool,
        scope,
        scopes,
        min: 0.0,
        max: 0.0,
        unit: "",
        default: ParamDefault::Bool(default),
        label,
    }
}

const fn e(
    id: &'static str,
    min: f32,
    max: f32,
    default: f32,
    label: &'static str,
    scopes: &'static [StripKind],
    scope: ParamScope,
) -> ParamDef {
    ParamDef {
        id,
        kind: ParamKind::Enum,
        scope,
        scopes,
        min,
        max,
        unit: "",
        default: ParamDefault::Float(default),
        label,
    }
}

const fn t(
    id: &'static str,
    label: &'static str,
    scopes: &'static [StripKind],
    scope: ParamScope,
) -> ParamDef {
    ParamDef {
        id,
        kind: ParamKind::Text,
        scope,
        scopes,
        min: 0.0,
        max: 0.0,
        unit: "",
        default: ParamDefault::Text(""),
        label,
    }
}

/// The canonical catalogue v2: fader/mute/name plus the full param plane
/// (input, HPF, gate, comp, 5-band EQ, legacy aux/fx sends, generic
/// `send1..16_*`, strip look, delay). Drivers advertise subsets via
/// [`crate::MixerDriver::capabilities`]; the relay `caps` frame renders this
/// table filtered by the active driver.
pub const CATALOGUE: &[ParamDef] = &[
    // ---- strip basics (all families) ----
    f("fader", 0.0, 1.0, "", 0.75, "fader", SCOPES_ALL, ParamScope::Channel),
    b("mute", "mute", SCOPES_ALL, ParamScope::Channel, false),
    t("name", "name", SCOPES_ALL, ParamScope::Channel),
    e("color", 0.0, 15.0, 2.0, "color", SCOPES_ALL, ParamScope::Channel),
    // ---- strip look / routing (no DCA) ----
    e("icon", 0.0, 31.0, 0.0, "icon", SCOPES_EQDYN, ParamScope::Channel),
    b("link", "link", SCOPES_INPUT, ParamScope::Channel, false),
    t("source", "source", SCOPES_INPUT, ParamScope::Channel),
    // ---- input / preamp ----
    f("gain", -12.0, 60.0, "dB", 0.0, "gain", SCOPES_INPUT, ParamScope::Channel),
    b("phantom", "+48V", SCOPES_INPUT, ParamScope::Channel, false),
    e("usbreturn", 0.0, 17.0, 0.0, "usb return", SCOPES_INPUT, ParamScope::Channel),
    f("trim", -18.0, 18.0, "dB", 0.0, "trim", SCOPES_INPUT, ParamScope::Channel),
    b("polarity", "phase", SCOPES_INPUT, ParamScope::Channel, false),
    b("delay_on", "delay on", SCOPES_INPUT, ParamScope::Channel, false),
    f("delay_ms", 0.0, 100.0, "ms", 0.0, "delay", SCOPES_INPUT, ParamScope::Channel),
    f("pan", -100.0, 100.0, "L/R", 0.0, "pan", SCOPES_PAN, ParamScope::Channel),
    b("hpf_on", "hpf on", SCOPES_INPUT, ParamScope::Channel, false),
    f("hpf_freq", 20.0, 400.0, "Hz", 80.0, "hpf", SCOPES_INPUT, ParamScope::Channel),
    e("hpf_slope", 0.0, 2.0, 0.0, "hpf slope", SCOPES_INPUT, ParamScope::Channel),
    // ---- gate (channel strips only; firmware availability is ch-only) ----
    b("gate_on", "gate on", SCOPES_GATE, ParamScope::Channel, false),
    f("gate_thr", -80.0, 0.0, "dB", -50.0, "gate thr", SCOPES_GATE, ParamScope::Channel),
    f("gate_range", 3.0, 60.0, "dB", 60.0, "gate range", SCOPES_GATE, ParamScope::Channel),
    f("gate_att", 0.0, 120.0, "ms", 1.0, "gate att", SCOPES_GATE, ParamScope::Channel),
    f("gate_hold", 0.02, 2000.0, "ms", 50.0, "gate hold", SCOPES_GATE, ParamScope::Channel),
    f("gate_rel", 5.0, 4000.0, "ms", 100.0, "gate rel", SCOPES_GATE, ParamScope::Channel),
    e("gate_mode", 0.0, 4.0, 1.0, "gate mode", SCOPES_GATE, ParamScope::Channel),
    e("gate_key_src", 0.0, 22.0, 0.0, "gate key", SCOPES_GATE, ParamScope::Channel),
    b("gate_filter_on", "gate flt on", SCOPES_GATE, ParamScope::Channel, false),
    e("gate_filter_type", 0.0, 8.0, 0.0, "gate flt type", SCOPES_GATE, ParamScope::Channel),
    f("gate_filter_freq", 20.0, 20000.0, "Hz", 1000.0, "gate flt f", SCOPES_GATE, ParamScope::Channel),
    // ---- compressor (channels, buses, main LR; firmware dyn/ ch/bus/lr) ----
    b("comp_on", "comp on", SCOPES_DYN, ParamScope::Channel, false),
    f("comp_thr", -60.0, 0.0, "dB", -20.0, "comp thr", SCOPES_DYN, ParamScope::Channel),
    f("comp_ratio", 1.1, 100.0, "", 2.0, "comp ratio", SCOPES_DYN, ParamScope::Channel),
    f("comp_knee", 0.0, 5.0, "", 3.0, "comp knee", SCOPES_DYN, ParamScope::Channel),
    f("comp_makeup", 0.0, 24.0, "dB", 0.0, "comp makeup", SCOPES_DYN, ParamScope::Channel),
    f("comp_att", 0.0, 120.0, "ms", 10.0, "comp att", SCOPES_DYN, ParamScope::Channel),
    f("comp_hold", 0.02, 2000.0, "ms", 20.0, "comp hold", SCOPES_DYN, ParamScope::Channel),
    f("comp_rel", 5.0, 4000.0, "ms", 100.0, "comp rel", SCOPES_DYN, ParamScope::Channel),
    e("comp_mode", 0.0, 1.0, 0.0, "comp mode", SCOPES_DYN, ParamScope::Channel),
    e("comp_det", 0.0, 1.0, 0.0, "comp det", SCOPES_DYN, ParamScope::Channel),
    e("comp_env", 0.0, 1.0, 0.0, "comp env", SCOPES_DYN, ParamScope::Channel),
    f("comp_mix", 0.0, 100.0, "%", 100.0, "comp mix", SCOPES_DYN, ParamScope::Channel),
    b("comp_auto", "comp auto", SCOPES_DYN, ParamScope::Channel, false),
    e("comp_key_src", 0.0, 22.0, 0.0, "comp key", SCOPES_DYN, ParamScope::Channel),
    b("comp_filter_on", "comp flt on", SCOPES_DYN, ParamScope::Channel, false),
    e("comp_filter_type", 0.0, 8.0, 0.0, "comp flt type", SCOPES_DYN, ParamScope::Channel),
    f("comp_filter_freq", 20.0, 20000.0, "Hz", 1000.0, "comp flt f", SCOPES_DYN, ParamScope::Channel),
    // ---- EQ (channels + returns: 4 bands; buses + main: 6 bands) ----
    b("eq_on", "eq on", SCOPES_EQ, ParamScope::Channel, false),
    f("eq1_gain", -15.0, 15.0, "dB", 0.0, "eq1 gain", SCOPES_EQ, ParamScope::Channel),
    f("eq1_freq", 20.0, 20000.0, "Hz", 100.0, "eq1 freq", SCOPES_EQ, ParamScope::Channel),
    f("eq1_q", 0.3, 10.0, "", 1.0, "eq1 Q", SCOPES_EQ, ParamScope::Channel),
    e("eq1_type", 0.0, 5.0, 0.0, "eq1 type", SCOPES_EQ, ParamScope::Channel),
    f("eq2_gain", -15.0, 15.0, "dB", 0.0, "eq2 gain", SCOPES_EQ, ParamScope::Channel),
    f("eq2_freq", 20.0, 20000.0, "Hz", 2000.0, "eq2 freq", SCOPES_EQ, ParamScope::Channel),
    f("eq2_q", 0.3, 10.0, "", 1.0, "eq2 Q", SCOPES_EQ, ParamScope::Channel),
    e("eq2_type", 0.0, 5.0, 0.0, "eq2 type", SCOPES_EQ, ParamScope::Channel),
    f("eq3_gain", -15.0, 15.0, "dB", 0.0, "eq3 gain", SCOPES_EQ, ParamScope::Channel),
    f("eq3_freq", 20.0, 20000.0, "Hz", 3000.0, "eq3 freq", SCOPES_EQ, ParamScope::Channel),
    f("eq3_q", 0.3, 10.0, "", 1.0, "eq3 Q", SCOPES_EQ, ParamScope::Channel),
    e("eq3_type", 0.0, 5.0, 0.0, "eq3 type", SCOPES_EQ, ParamScope::Channel),
    f("eq4_gain", -15.0, 15.0, "dB", 0.0, "eq4 gain", SCOPES_EQ, ParamScope::Channel),
    f("eq4_freq", 20.0, 20000.0, "Hz", 12000.0, "eq4 freq", SCOPES_EQ, ParamScope::Channel),
    f("eq4_q", 0.3, 10.0, "", 1.0, "eq4 Q", SCOPES_EQ, ParamScope::Channel),
    e("eq4_type", 0.0, 5.0, 0.0, "eq4 type", SCOPES_EQ, ParamScope::Channel),
    f("eq5_gain", -15.0, 15.0, "dB", 0.0, "eq5 gain", SCOPES_EQ56, ParamScope::Channel),
    f("eq5_freq", 20.0, 20000.0, "Hz", 16000.0, "eq5 freq", SCOPES_EQ56, ParamScope::Channel),
    f("eq5_q", 0.3, 10.0, "", 1.0, "eq5 Q", SCOPES_EQ56, ParamScope::Channel),
    e("eq5_type", 0.0, 5.0, 0.0, "eq5 type", SCOPES_EQ56, ParamScope::Channel),
    f("eq6_gain", -15.0, 15.0, "dB", 0.0, "eq6 gain", SCOPES_EQ56, ParamScope::Channel),
    f("eq6_freq", 20.0, 20000.0, "Hz", 16000.0, "eq6 freq", SCOPES_EQ56, ParamScope::Channel),
    f("eq6_q", 0.3, 10.0, "", 1.0, "eq6 Q", SCOPES_EQ56, ParamScope::Channel),
    e("eq6_type", 0.0, 5.0, 0.0, "eq6 type", SCOPES_EQ56, ParamScope::Channel),
    // ---- legacy sends (kept for the v3 UI) ----
    f("aux1_level", -90.0, 10.0, "dB", -90.0, "aux1", SCOPES_SEND, ParamScope::Channel),
    b("aux1_mute", "m1", SCOPES_SEND, ParamScope::Channel, false),
    f("aux2_level", -90.0, 10.0, "dB", -90.0, "aux2", SCOPES_SEND, ParamScope::Channel),
    b("aux2_mute", "m2", SCOPES_SEND, ParamScope::Channel, false),
    f("aux3_level", -90.0, 10.0, "dB", -90.0, "aux3", SCOPES_SEND, ParamScope::Channel),
    b("aux3_mute", "m3", SCOPES_SEND, ParamScope::Channel, false),
    f("aux4_level", -90.0, 10.0, "dB", -90.0, "aux4", SCOPES_SEND, ParamScope::Channel),
    b("aux4_mute", "m4", SCOPES_SEND, ParamScope::Channel, false),
    f("aux5_level", -90.0, 10.0, "dB", -90.0, "aux5", SCOPES_SEND, ParamScope::Channel),
    b("aux5_mute", "m5", SCOPES_SEND, ParamScope::Channel, false),
    f("aux6_level", -90.0, 10.0, "dB", -90.0, "aux6", SCOPES_SEND, ParamScope::Channel),
    b("aux6_mute", "m6", SCOPES_SEND, ParamScope::Channel, false),
    f("aux7_level", -90.0, 10.0, "dB", -90.0, "aux7", SCOPES_SEND, ParamScope::Channel),
    b("aux7_mute", "m7", SCOPES_SEND, ParamScope::Channel, false),
    f("aux8_level", -90.0, 10.0, "dB", -90.0, "aux8", SCOPES_SEND, ParamScope::Channel),
    b("aux8_mute", "m8", SCOPES_SEND, ParamScope::Channel, false),
    f("fx1_level", -90.0, 10.0, "dB", -90.0, "fx1", SCOPES_SEND, ParamScope::Channel),
    b("fx1_mute", "m1", SCOPES_SEND, ParamScope::Channel, false),
    f("fx2_level", -90.0, 10.0, "dB", -90.0, "fx2", SCOPES_SEND, ParamScope::Channel),
    b("fx2_mute", "m2", SCOPES_SEND, ParamScope::Channel, false),
    f("fx3_level", -90.0, 10.0, "dB", -90.0, "fx3", SCOPES_SEND, ParamScope::Channel),
    b("fx3_mute", "m3", SCOPES_SEND, ParamScope::Channel, false),
    f("fx4_level", -90.0, 10.0, "dB", -90.0, "fx4", SCOPES_SEND, ParamScope::Channel),
    b("fx4_mute", "m4", SCOPES_SEND, ParamScope::Channel, false),
    // ---- generic sends 1..16 (forward path) ----
    f("send1_level", -90.0, 10.0, "dB", -90.0, "send1", SCOPES_SEND, ParamScope::Channel),
    e("send1_tap", 0.0, 5.0, 1.0, "send1 tap", SCOPES_SEND, ParamScope::Channel),
    f("send1_pan", -100.0, 100.0, "L/R", 0.0, "send1 pan", SCOPES_SEND, ParamScope::Channel),
    b("send1_on", "send1 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send2_level", -90.0, 10.0, "dB", -90.0, "send2", SCOPES_SEND, ParamScope::Channel),
    e("send2_tap", 0.0, 5.0, 1.0, "send2 tap", SCOPES_SEND, ParamScope::Channel),
    f("send2_pan", -100.0, 100.0, "L/R", 0.0, "send2 pan", SCOPES_SEND, ParamScope::Channel),
    b("send2_on", "send2 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send3_level", -90.0, 10.0, "dB", -90.0, "send3", SCOPES_SEND, ParamScope::Channel),
    e("send3_tap", 0.0, 5.0, 1.0, "send3 tap", SCOPES_SEND, ParamScope::Channel),
    f("send3_pan", -100.0, 100.0, "L/R", 0.0, "send3 pan", SCOPES_SEND, ParamScope::Channel),
    b("send3_on", "send3 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send4_level", -90.0, 10.0, "dB", -90.0, "send4", SCOPES_SEND, ParamScope::Channel),
    e("send4_tap", 0.0, 5.0, 1.0, "send4 tap", SCOPES_SEND, ParamScope::Channel),
    f("send4_pan", -100.0, 100.0, "L/R", 0.0, "send4 pan", SCOPES_SEND, ParamScope::Channel),
    b("send4_on", "send4 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send5_level", -90.0, 10.0, "dB", -90.0, "send5", SCOPES_SEND, ParamScope::Channel),
    e("send5_tap", 0.0, 5.0, 1.0, "send5 tap", SCOPES_SEND, ParamScope::Channel),
    f("send5_pan", -100.0, 100.0, "L/R", 0.0, "send5 pan", SCOPES_SEND, ParamScope::Channel),
    b("send5_on", "send5 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send6_level", -90.0, 10.0, "dB", -90.0, "send6", SCOPES_SEND, ParamScope::Channel),
    e("send6_tap", 0.0, 5.0, 1.0, "send6 tap", SCOPES_SEND, ParamScope::Channel),
    f("send6_pan", -100.0, 100.0, "L/R", 0.0, "send6 pan", SCOPES_SEND, ParamScope::Channel),
    b("send6_on", "send6 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send7_level", -90.0, 10.0, "dB", -90.0, "send7", SCOPES_SEND, ParamScope::Channel),
    e("send7_tap", 0.0, 5.0, 1.0, "send7 tap", SCOPES_SEND, ParamScope::Channel),
    f("send7_pan", -100.0, 100.0, "L/R", 0.0, "send7 pan", SCOPES_SEND, ParamScope::Channel),
    b("send7_on", "send7 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send8_level", -90.0, 10.0, "dB", -90.0, "send8", SCOPES_SEND, ParamScope::Channel),
    e("send8_tap", 0.0, 5.0, 1.0, "send8 tap", SCOPES_SEND, ParamScope::Channel),
    f("send8_pan", -100.0, 100.0, "L/R", 0.0, "send8 pan", SCOPES_SEND, ParamScope::Channel),
    b("send8_on", "send8 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send9_level", -90.0, 10.0, "dB", -90.0, "send9", SCOPES_SEND, ParamScope::Channel),
    e("send9_tap", 0.0, 5.0, 1.0, "send9 tap", SCOPES_SEND, ParamScope::Channel),
    f("send9_pan", -100.0, 100.0, "L/R", 0.0, "send9 pan", SCOPES_SEND, ParamScope::Channel),
    b("send9_on", "send9 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send10_level", -90.0, 10.0, "dB", -90.0, "send10", SCOPES_SEND, ParamScope::Channel),
    e("send10_tap", 0.0, 5.0, 1.0, "send10 tap", SCOPES_SEND, ParamScope::Channel),
    f("send10_pan", -100.0, 100.0, "L/R", 0.0, "send10 pan", SCOPES_SEND, ParamScope::Channel),
    b("send10_on", "send10 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send11_level", -90.0, 10.0, "dB", -90.0, "send11", SCOPES_SEND, ParamScope::Channel),
    e("send11_tap", 0.0, 5.0, 1.0, "send11 tap", SCOPES_SEND, ParamScope::Channel),
    f("send11_pan", -100.0, 100.0, "L/R", 0.0, "send11 pan", SCOPES_SEND, ParamScope::Channel),
    b("send11_on", "send11 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send12_level", -90.0, 10.0, "dB", -90.0, "send12", SCOPES_SEND, ParamScope::Channel),
    e("send12_tap", 0.0, 5.0, 1.0, "send12 tap", SCOPES_SEND, ParamScope::Channel),
    f("send12_pan", -100.0, 100.0, "L/R", 0.0, "send12 pan", SCOPES_SEND, ParamScope::Channel),
    b("send12_on", "send12 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send13_level", -90.0, 10.0, "dB", -90.0, "send13", SCOPES_SEND, ParamScope::Channel),
    e("send13_tap", 0.0, 5.0, 1.0, "send13 tap", SCOPES_SEND, ParamScope::Channel),
    f("send13_pan", -100.0, 100.0, "L/R", 0.0, "send13 pan", SCOPES_SEND, ParamScope::Channel),
    b("send13_on", "send13 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send14_level", -90.0, 10.0, "dB", -90.0, "send14", SCOPES_SEND, ParamScope::Channel),
    e("send14_tap", 0.0, 5.0, 1.0, "send14 tap", SCOPES_SEND, ParamScope::Channel),
    f("send14_pan", -100.0, 100.0, "L/R", 0.0, "send14 pan", SCOPES_SEND, ParamScope::Channel),
    b("send14_on", "send14 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send15_level", -90.0, 10.0, "dB", -90.0, "send15", SCOPES_SEND, ParamScope::Channel),
    e("send15_tap", 0.0, 5.0, 1.0, "send15 tap", SCOPES_SEND, ParamScope::Channel),
    f("send15_pan", -100.0, 100.0, "L/R", 0.0, "send15 pan", SCOPES_SEND, ParamScope::Channel),
    b("send15_on", "send15 on", SCOPES_SEND, ParamScope::Channel, true),
    f("send16_level", -90.0, 10.0, "dB", -90.0, "send16", SCOPES_SEND, ParamScope::Channel),
    e("send16_tap", 0.0, 5.0, 1.0, "send16 tap", SCOPES_SEND, ParamScope::Channel),
    f("send16_pan", -100.0, 100.0, "L/R", 0.0, "send16 pan", SCOPES_SEND, ParamScope::Channel),
    b("send16_on", "send16 on", SCOPES_SEND, ParamScope::Channel, true),
];

/// Look up one catalogue entry by id (canonical table only; the engine
/// resolves through its per-driver [`ParamRegistry`] so provider
/// contributions merge in — see [`register_catalogue`]).
pub fn find(id: &str) -> Option<&'static ParamDef> {
    CATALOGUE.iter().find(|d| d.id == id)
}

/// Merged param-definition table: the canonical [`CATALOGUE`] plus one
/// provider's contributed rows (see
/// [`MixerDriver::param_defs`](crate::MixerDriver::param_defs)).
/// The engine builds one per registered driver at boot; every caps / poll
/// / snapshot / recall path resolves through it instead of the global
/// static, so a mixer with unique DSP arrives without editing core.
///
/// Merge rule: canonical rows are stable and always win — a provider row
/// whose id collides with a canonical id is ignored. Provider rows keep
/// their table order after the canonical rows, so `caps` output is
/// byte-identical while no provider contributes extra rows.
#[derive(Clone, Debug)]
pub struct ParamRegistry {
    defs: Vec<ParamDef>,
}

impl ParamRegistry {
    /// The canonical table alone ([`CATALOGUE`], same order).
    pub fn canonical() -> Self {
        Self {
            defs: CATALOGUE.to_vec(),
        }
    }

    /// Merge one provider-owned table (no-op for `&[]`). Canonical ids
    /// win on collision; new ids (conventionally `vendor:param`) append in
    /// table order.
    pub fn register(&mut self, extra: &[ParamDef]) {
        for def in extra {
            if !self.defs.iter().any(|d| d.id == def.id) {
                self.defs.push(*def);
            }
        }
    }

    /// Look up one entry by id (canonical + contributed).
    pub fn find(&self, id: &str) -> Option<&ParamDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// All entries in merge order (canonical first, then provider rows).
    pub fn iter(&self) -> impl Iterator<Item = &ParamDef> {
        self.defs.iter()
    }

    /// Entry count (canonical size while no provider contributes).
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether the registry holds no entries (never true for a merged
    /// registry — the canonical table is always present).
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Validate a value against one registry entry: right kind, finite, in
    /// range (enums additionally whole-numbered). Unknown ids fail.
    pub fn validate(&self, id: &str, value: &ParamValue) -> anyhow::Result<()> {
        let def = self
            .find(id)
            .ok_or_else(|| anyhow::anyhow!("unknown param {id:?}"))?;
        validate_def(def, id, value)
    }
}

/// Build a merged registry: canonical [`CATALOGUE`] plus one provider's
/// contributed table ([`crate::MixerDriver::param_defs`](crate::MixerDriver::param_defs)).
/// This is the engine-boot merge; canonical rows are stable and win on id
/// collision.
pub fn register_catalogue(provider: &[ParamDef]) -> ParamRegistry {
    let mut reg = ParamRegistry::canonical();
    reg.register(provider);
    reg
}

/// Validate a value against one catalogue entry: right kind, finite, in
/// range (enums additionally whole-numbered). Unknown ids fail.
pub fn validate(id: &str, value: &ParamValue) -> anyhow::Result<()> {
    let def = find(id).ok_or_else(|| anyhow::anyhow!("unknown param {id:?}"))?;
    validate_def(def, id, value)
}

fn validate_def(def: &ParamDef, id: &str, value: &ParamValue) -> anyhow::Result<()> {
    match (def.kind, value) {
        (ParamKind::Float, ParamValue::Float(v)) => {
            if !v.is_finite() {
                anyhow::bail!("{id} must be finite, got {v}");
            }
            if !(def.min..=def.max).contains(v) {
                anyhow::bail!("{id} {v} out of range {}..={}", def.min, def.max);
            }
            Ok(())
        }
        (ParamKind::Bool, ParamValue::Bool(_)) => Ok(()),
        (ParamKind::Text, ParamValue::Text(_)) => Ok(()),
        (ParamKind::Enum, ParamValue::Float(v)) => {
            if !v.is_finite() || v.fract() != 0.0 {
                anyhow::bail!("{id} must be a whole number, got {v}");
            }
            if !(def.min..=def.max).contains(v) {
                anyhow::bail!("{id} {v} out of range {}..={}", def.min, def.max);
            }
            Ok(())
        }
        (kind, got) => anyhow::bail!("{id} needs {kind:?}, got {got:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_unique_and_shaped() {
        let mut seen = HashSet::new();
        for d in CATALOGUE {
            assert!(!d.id.is_empty(), "empty param id");
            assert!(seen.insert(d.id), "duplicate param id {}", d.id);
            assert!(!d.label.is_empty(), "empty label for {}", d.id);
            // v2: every entry declares at least one strip kind; the legacy
            // single `scope` stays populated for back-compat JSON.
            assert!(!d.scopes.is_empty(), "empty scopes for {}", d.id);
            for k in d.scopes {
                let _ = format!("{k:?}");
            }
        }
        // v2 catalogue size: 54 v1 ids + 95 new (colour/icon/link/source,
        // delay x2, hpf_slope, gate x8, comp x12, eq5 x4, send16 x64) = 149,
        // then +11 firmware-truth ids (gate filter x3, comp key/filter x5,
        // eq6 x4) − 1 removed id (gate_key_filter) = 159.
        assert_eq!(CATALOGUE.len(), 159, "catalogue grew/shrank unexpectedly");
    }

    #[test]
    fn ranges_sane_and_default_in_range() {
        for d in CATALOGUE {
            match d.kind {
                ParamKind::Float | ParamKind::Enum => {
                    assert!(d.min < d.max, "empty range for {}", d.id);
                    assert!(d.min.is_finite() && d.max.is_finite(), "range for {}", d.id);
                }
                ParamKind::Bool | ParamKind::Text => {
                    assert_eq!((d.min, d.max), (0.0, 0.0), "bool/text range for {}", d.id);
                }
            }
            validate(d.id, &d.default.as_param_value())
                .unwrap_or_else(|e| panic!("default for {} invalid: {e}", d.id));
        }
    }

    #[test]
    fn scopes_cover_the_design_families() {
        use crate::StripKind as K;
        // Strip basics reach every family (DCA included).
        for id in ["fader", "mute", "name", "color"] {
            let d = find(id).unwrap();
            for k in [K::In, K::Ret, K::Bus, K::FxSnd, K::Mtx, K::Main, K::Dca] {
                assert!(supports_kind(d, k), "{id} misses {k:?}");
            }
        }
        // DCA carries fader/mute/name/color ONLY.
        for d in CATALOGUE {
            if supports_kind(d, K::Dca) {
                assert!(
                    ["fader", "mute", "name", "color"].contains(&d.id),
                    "DCA carries unexpected {}",
                    d.id
                );
            }
        }
        // Sends originate from inputs only.
        for d in CATALOGUE {
            if d.id.starts_with("send") || d.id.starts_with("aux") || d.id.starts_with("fx") {
                assert_eq!(d.scopes, SCOPES_SEND, "send scope for {}", d.id);
            }
        }
        // Legacy v1 ids are all still present (v3 UI back-compat).
        for id in [
            "aux1_level",
            "aux8_mute",
            "fx1_level",
            "fx4_mute",
            "usbreturn",
            "eq1_type",
        ] {
            assert!(find(id).is_some(), "v1 id {id} missing");
        }
    }

    #[test]
    fn kind_strings_match_ws_contract() {
        assert_eq!(ParamKind::Float.as_str(), "float");
        assert_eq!(ParamKind::Bool.as_str(), "bool");
        assert_eq!(ParamKind::Enum.as_str(), "enum");
        assert_eq!(ParamKind::Text.as_str(), "text");
        assert_eq!(ParamScope::Channel.as_str(), "channel");
        assert_eq!(ParamScope::Bus.as_str(), "bus");
        assert_eq!(ParamScope::Main.as_str(), "main");
    }

    #[test]
    fn validate_rejects_wrong_kind_and_range() {
        assert!(validate("gain", &ParamValue::Float(0.0)).is_ok());
        assert!(validate("gain", &ParamValue::Bool(true)).is_err());
        assert!(validate("gain", &ParamValue::Float(61.0)).is_err());
        assert!(validate("gain", &ParamValue::Float(f32::NAN)).is_err());
        assert!(validate("phantom", &ParamValue::Bool(true)).is_ok());
        assert!(validate("phantom", &ParamValue::Float(1.0)).is_err());
        assert!(validate("usbreturn", &ParamValue::Float(17.0)).is_ok());
        assert!(validate("usbreturn", &ParamValue::Float(3.5)).is_err());
        assert!(validate("usbreturn", &ParamValue::Float(18.0)).is_err());
        assert!(validate("name", &ParamValue::Text("Kick".into())).is_ok());
        assert!(validate("name", &ParamValue::Float(0.0)).is_err());
        assert!(validate("color", &ParamValue::Float(15.0)).is_ok());
        assert!(validate("color", &ParamValue::Float(16.0)).is_err());
        assert!(validate("send3_tap", &ParamValue::Float(1.0)).is_ok());
        assert!(validate("send3_tap", &ParamValue::Float(5.0)).is_ok());
        assert!(validate("send3_tap", &ParamValue::Float(6.0)).is_err());
        assert!(validate("comp_ratio", &ParamValue::Float(2.0)).is_ok());
        assert!(validate("comp_ratio", &ParamValue::Float(1.0)).is_err());
        assert!(validate("nope", &ParamValue::Float(0.0)).is_err());
    }

    // Provider-contributed DSP rows for registry tests (never merged into
    // CATALOGUE itself — the canonical table stays X-Air-shaped).
    const PROVIDER_TABLE: &[ParamDef] = &[
        ParamDef {
            id: "wing:stagebox_gain",
            kind: ParamKind::Float,
            scope: ParamScope::Channel,
            scopes: SCOPES_INPUT,
            min: -12.0,
            max: 60.0,
            unit: "dB",
            default: ParamDefault::Float(0.0),
            label: "stagebox gain",
        },
        // A colliding row: the merge must keep the canonical `gain`
        // (stable range/label) and ignore this one.
        ParamDef {
            id: "gain",
            kind: ParamKind::Float,
            scope: ParamScope::Channel,
            scopes: SCOPES_INPUT,
            min: 0.0,
            max: 1.0,
            unit: "",
            default: ParamDefault::Float(0.0),
            label: "shadowed",
        },
    ];

    #[test]
    fn registry_canonical_matches_static() {
        let reg = ParamRegistry::canonical();
        assert_eq!(reg.len(), CATALOGUE.len());
        for def in CATALOGUE {
            assert_eq!(reg.find(def.id).map(|d| d.id), Some(def.id));
        }
        // Free `find` and the registry agree on the canonical table.
        assert_eq!(find("fader").unwrap().id, reg.find("fader").unwrap().id);
    }

    #[test]
    fn registry_merges_provider_rows_canonical_wins() {
        let reg = register_catalogue(PROVIDER_TABLE);
        assert_eq!(reg.len(), CATALOGUE.len() + 1);
        // Namespaced provider id resolves with its own range.
        let extra = reg.find("wing:stagebox_gain").expect("provider row merges");
        assert_eq!((extra.min, extra.max), (-12.0, 60.0));
        assert!(reg.validate("wing:stagebox_gain", &ParamValue::Float(0.0)).is_ok());
        assert!(reg.validate("wing:stagebox_gain", &ParamValue::Float(61.0)).is_err());
        assert!(reg.validate("wing:stagebox_gain", &ParamValue::Bool(true)).is_err());
        // Canonical `gain` is untouched by the colliding provider row.
        let gain = reg.find("gain").unwrap();
        assert_eq!((gain.min, gain.max), (-12.0, 60.0));
        assert_eq!(gain.label, "gain");
        // Canonical fader/mute/name/pan stay stable through any merge.
        for id in ["fader", "mute", "name", "pan"] {
            assert_eq!(reg.find(id).map(|d| d.id), Some(id));
            assert_eq!(find(id).unwrap().id, reg.find(id).unwrap().id);
        }
        // Unknown ids still fail.
        assert!(reg.validate("nope", &ParamValue::Float(0.0)).is_err());
    }

    #[test]
    fn registry_register_is_idempotent_and_ordered() {
        let mut reg = ParamRegistry::canonical();
        reg.register(PROVIDER_TABLE);
        let n = reg.len();
        // Re-registering (or registering canonical ids) changes nothing.
        reg.register(PROVIDER_TABLE);
        reg.register(CATALOGUE);
        assert_eq!(reg.len(), n);
        // Canonical rows keep catalogue order; provider rows append after.
        let ids: Vec<&str> = reg.iter().map(|d| d.id).collect();
        assert_eq!(&ids[..CATALOGUE.len()], CATALOGUE.iter().map(|d| d.id).collect::<Vec<_>>());
        assert_eq!(ids[CATALOGUE.len()], "wing:stagebox_gain");
    }

    #[test]
    fn strip_kind_wire_names_stable_and_extensible() {
        use crate::StripKind as K;
        // The seven historical wire names are byte-identical.
        for (kind, wire) in [
            (K::In, "in"),
            (K::Ret, "ret"),
            (K::Bus, "bus"),
            (K::FxSnd, "fxsnd"),
            (K::Mtx, "mtx"),
            (K::Main, "main"),
            (K::Dca, "dca"),
        ] {
            assert_eq!(kind.as_str(), wire);
            assert_eq!(K::parse(wire), Some(kind));
            assert_eq!(K::kind_from_str(wire), kind);
        }
        // Unknown wire names fail strict parse but resolve extensibly.
        assert_eq!(K::parse("grp"), None);
        let custom = K::kind_from_str("grp");
        assert_eq!(custom.as_str(), "grp");
        assert!(matches!(custom, K::Custom("grp")));
        // Custom kinds participate in scopes like any other kind.
        let def = ParamDef {
            id: "wing:stagebox_gain",
            kind: ParamKind::Float,
            scope: ParamScope::Channel,
            scopes: &[K::Custom("grp")],
            min: 0.0,
            max: 1.0,
            unit: "",
            default: ParamDefault::Float(0.0),
            label: "x",
        };
        assert!(supports_kind(&def, K::kind_from_str("grp")));
        assert!(!supports_kind(&def, K::In));
    }
}
