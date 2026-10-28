/// XR18 strip index map (global, 36 strips). Channels 0..16 keep their
/// historic meaning; buses stay single-digit on the wire (T2); the FX-return
/// family lives at `/rtn/N` (the firmware map has no `/fxrtn` root —
/// `/fxrtn/N/*` is silent on FW 1.20); the stereo aux input lives at
/// `/rtn/aux` (NOT `/aux/*` or `/auxin/*` — the desk is silent there,
/// owner-rig verified 2026-10-20); DCA strips address fader/on directly
/// under `/dca/N` (no `/mix` segment — `/dca/N/mix/fader` is silent).
/// Every family below is live-verified on the owner rig except `/rtn/N`
/// fader/on (verified present 2026-10-18: fader + on answer) — see
/// XR18 baseline notes:
/// | indices | family | kind  | root       |
/// |---------|--------|-------|------------|
/// | 0..16   | ch     | in    | `/ch/NN`   |
/// | 16..20  | fxret  | ret   | `/rtn/N`   |
/// | 20..26  | bus    | bus   | `/bus/N`   |
/// | 26..30  | fxsnd  | fxsnd | `/fxsend/N`|
/// | 30..31  | main   | main  | `/lr`      |
/// | 31..35  | dca    | dca   | `/dca/N`   |
/// | 35..36  | aux    | ret   | `/rtn/aux` |
pub const XAIR_TOPOLOGY: [openfader_core::StripFamily; 7] = [
    openfader_core::StripFamily {
        key: "ch",
        kind: openfader_core::StripKind::In,
        label: "Ch 1-16",
        first: 0,
        count: 16,
        name_prefix: "Ch",
        color_default: 2,
    },
    openfader_core::StripFamily {
        key: "fxret",
        kind: openfader_core::StripKind::Ret,
        label: "FxRet 1-4",
        first: 16,
        count: 4,
        name_prefix: "FxRet",
        color_default: 6,
    },
    openfader_core::StripFamily {
        key: "bus",
        kind: openfader_core::StripKind::Bus,
        label: "Bus 1-6",
        first: 20,
        count: 6,
        name_prefix: "Mix",
        color_default: 6,
    },
    openfader_core::StripFamily {
        key: "fxsnd",
        kind: openfader_core::StripKind::FxSnd,
        label: "FxSnd 1-4",
        first: 26,
        count: 4,
        name_prefix: "FxSnd",
        color_default: 4,
    },
    openfader_core::StripFamily {
        key: "main",
        kind: openfader_core::StripKind::Main,
        label: "Main LR",
        first: 30,
        count: 1,
        name_prefix: "Main",
        color_default: 11,
    },
    openfader_core::StripFamily {
        key: "dca",
        kind: openfader_core::StripKind::Dca,
        label: "DCA 1-4",
        first: 31,
        count: 4,
        name_prefix: "DCA",
        color_default: 0,
    },
    openfader_core::StripFamily {
        key: "aux",
        kind: openfader_core::StripKind::Ret,
        label: "Aux",
        first: 35,
        count: 1,
        name_prefix: "Aux",
        color_default: 2,
    },
];

/// XR18 color indices follow the X-Air OSC color enumeration (0–7 and inverted 8–15).
pub const XAIR_COLOR_SWATCHES: &[openfader_core::params::ParamSwatch] = &[
    openfader_core::params::ParamSwatch {
        value: 0.0,
        label: "None",
        rgb: None,
    },
    openfader_core::params::ParamSwatch {
        value: 1.0,
        label: "Red",
        rgb: Some(0xe74c3c),
    },
    openfader_core::params::ParamSwatch {
        value: 2.0,
        label: "Green",
        rgb: Some(0x43a66c),
    },
    openfader_core::params::ParamSwatch {
        value: 3.0,
        label: "Yellow",
        rgb: Some(0xe9c94a),
    },
    openfader_core::params::ParamSwatch {
        value: 4.0,
        label: "Blue",
        rgb: Some(0x4c83cf),
    },
    openfader_core::params::ParamSwatch {
        value: 5.0,
        label: "Magenta",
        rgb: Some(0xb55fba),
    },
    openfader_core::params::ParamSwatch {
        value: 6.0,
        label: "Cyan",
        rgb: Some(0x50b8c4),
    },
    openfader_core::params::ParamSwatch {
        value: 7.0,
        label: "White",
        rgb: Some(0xe3e3e3),
    },
    openfader_core::params::ParamSwatch {
        value: 8.0,
        label: "None (inverted)",
        rgb: None,
    },
    openfader_core::params::ParamSwatch {
        value: 9.0,
        label: "Red (inverted)",
        rgb: Some(0xe74c3c),
    },
    openfader_core::params::ParamSwatch {
        value: 10.0,
        label: "Green (inverted)",
        rgb: Some(0x43a66c),
    },
    openfader_core::params::ParamSwatch {
        value: 11.0,
        label: "Yellow (inverted)",
        rgb: Some(0xe9c94a),
    },
    openfader_core::params::ParamSwatch {
        value: 12.0,
        label: "Blue (inverted)",
        rgb: Some(0x4c83cf),
    },
    openfader_core::params::ParamSwatch {
        value: 13.0,
        label: "Magenta (inverted)",
        rgb: Some(0xb55fba),
    },
    openfader_core::params::ParamSwatch {
        value: 14.0,
        label: "Cyan (inverted)",
        rgb: Some(0x50b8c4),
    },
    openfader_core::params::ParamSwatch {
        value: 15.0,
        label: "White (inverted)",
        rgb: Some(0xe3e3e3),
    },
];

/// Total addressable strips on the XR18 map (16 + 4 + 6 + 4 + 1 + 4 + 1).
pub const XAIR_STRIPS: usize = 36;

/// Param-plane capabilities implemented by this driver. Most have a strict
/// fake-desk loopback and XR18 FW 1.20 evidence; `source` is a read-only
/// derived value verified through live reads. Everything else in
/// [`openfader_core::params::CATALOGUE`] is reported in `caps.unsupported`
/// so the UI hides it. Honest exclusions: `delay_on`/`delay_ms`
/// (X32-only), `hpf_slope` (X32-only), `icon` (no such leaf in the firmware map),
/// `send1..10_on` + all `auxN_mute`/`fxN_mute` (the firmware send block has
/// no on/off leaf and `/ch/NN/mix/MM/on` is live-silent — baseline T7),
/// `send11..16_*` (only 6+4 send slots), per-channel `eq5_*` (channels and
/// aux returns carry 4 bands; `eq5/6_*` ride buses + main only),
/// `solo_dim`/`solo_mute`/`solo_level`/`solo_source` (global
/// `/config/solo/*`, driven via driver methods, not strip params) and
/// `fx_insert` (slot-addressed `/fx/N/insert`, likewise a driver method).
/// `source` is a read-only active-source label, not an input patch command.
pub const XAIR_CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "link",
    "source",
    "xair:preamp_state",
    "xair:automix_gain",
    "gain",
    "phantom",
    "trim",
    "polarity",
    "pan",
    "lr_assign",
    "dca1_assign",
    "dca2_assign",
    "dca3_assign",
    "dca4_assign",
    "hpf_on",
    "hpf_freq",
    "automix_on",
    "automix_group",
    "automix_weight",
    "insert_on",
    "insert_sel",
    "color",
    "gate_on",
    "gate_thr",
    "gate_range",
    "gate_att",
    "gate_hold",
    "gate_rel",
    "gate_mode",
    "gate_key_src",
    "gate_filter_on",
    "gate_filter_type",
    "gate_filter_freq",
    "comp_on",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_makeup",
    "comp_att",
    "comp_hold",
    "comp_rel",
    "comp_mode",
    "comp_det",
    "comp_env",
    "comp_mix",
    "comp_auto",
    "comp_key_src",
    "comp_filter_on",
    "comp_filter_type",
    "comp_filter_freq",
    "eq_on",
    "eq1_gain",
    "eq1_freq",
    "eq1_q",
    "eq1_type",
    "eq2_gain",
    "eq2_freq",
    "eq2_q",
    "eq2_type",
    "eq3_gain",
    "eq3_freq",
    "eq3_q",
    "eq3_type",
    "eq4_gain",
    "eq4_freq",
    "eq4_q",
    "eq4_type",
    "eq5_gain",
    "eq5_freq",
    "eq5_q",
    "eq5_type",
    "eq6_gain",
    "eq6_freq",
    "eq6_q",
    "eq6_type",
    "aux1_level",
    "aux2_level",
    "aux3_level",
    "aux4_level",
    "aux5_level",
    "aux6_level",
    "fx1_level",
    "fx2_level",
    "fx3_level",
    "fx4_level",
    "send1_level",
    "send1_tap",
    "send1_pan",
    "send1_grpon",
    "send2_level",
    "send2_tap",
    "send2_grpon",
    "send3_level",
    "send3_tap",
    "send3_pan",
    "send3_grpon",
    "send4_level",
    "send4_tap",
    "send4_grpon",
    "send5_level",
    "send5_tap",
    "send5_pan",
    "send5_grpon",
    "send6_level",
    "send6_tap",
    "send6_grpon",
    "send7_level",
    "send7_tap",
    "send7_grpon",
    "send8_level",
    "send8_tap",
    "send8_grpon",
    "send9_level",
    "send9_tap",
    "send9_grpon",
    "send10_level",
    "send10_tap",
    "send10_grpon",
];
