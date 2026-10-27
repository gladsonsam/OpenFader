//! Provider capabilities, engineering ranges and presentation catalogue.
use crate::Dl32sDriver;

/// Wave-1+2 canonical surface: every id below names a DL32S leaf whose
/// position was confirmed on hardware AND whose value domain was confirmed
/// live — by the read-only push-cache survey (varied values prove the desk
/// stores the domain) or by write+relay with net-zero restore 2026-10-20.
/// See DL32S protocol notes and the `verified` flags in [`crate::addrs`].
///
/// Deliberately absent (reachable only by raw table key for experts):
/// - `gate_mode`: native CH7 Modern=0/Vintage=1 verified with independent
///   readback and exact restore on 2026-10-25. Vintage uses discrete
///   attack/release controls; modern timing units must not be assumed.
/// - `eq_mode` labels remain unverified. Compressor model/knee enums,
///   Modern ranges and Vintage selections verified on CH7, 2026-10-25;
///   native inf:1 is wire value 101 (see provider presentations).
/// - `eqN_type` — all four bands carry a leading type word (offsets
///   34/38/42/46). Raw 2 = Bell, 3 = LowShelf, 4 = HighShelf verified live
///   on B1/B4 (CH7 write+relay, net-zero restored 2026-10-24); native 0/1/5
///   meanings unverified, values relay exact, no Rust canonical remap
///   (frontend translates presentation LowShelf canonical 1 <-> native 3).
/// - Mute/subgroup membership uses engine group commands rather than strip
///   caps; VCA/View membership uses the advertised assignment bool leaves.
/// - `source_a`/`source_b` — opaque native routing IDs (1000+/2000+).
///   Provider-labelled physical/USB routing IDs; unknown IDs remain opaque.
/// - `source_select` — native A/B selector, 0/1, exposed as a provider row.
/// - `sendN_tap`/`sendN_grpon`/`sendN_on` — no confirmed leaf (the desk is
///   silent; the UI collapses them, never faked).
/// - `hpf_slope` — reads constant 2, meaning inferred.
///
/// Return/FX-return/VCA applicability is resolved separately by `strips::scopes`
/// from the native compiled schemas. Those additions are statically confirmed;
/// input hardware evidence below does not certify the new families live.
///
/// Per-provider caps ranges ride `param_range_overrides` (documented
/// modern-DL32S bounds, see `DL32S_RANGE_OVERRIDES`); the canonical
/// catalogue is untouched.
pub const DL32S_CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "pan",
    "gain",
    "phantom",
    "trim",
    "source_a",
    "source_b",
    "source_select",
    "polarity",
    "hpf_on",
    "hpf_freq",
    "lr_assign",
    "dca1_assign",
    "dca2_assign",
    "dca3_assign",
    "dca4_assign",
    "dca5_assign",
    "dca6_assign",
    "view1_assign",
    "view2_assign",
    "view3_assign",
    "view4_assign",
    "view5_assign",
    "view6_assign",
    "link",
    "color",
    "icon",
    "gate_on",
    "gate_thr",
    "gate_att",
    "gate_mode",
    "gate_rel",
    "gate_range",
    "gate_hold",
    "comp_on",
    "comp_mode",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_att",
    "comp_rel",
    "comp_makeup",
    "eq_on",
    "eq_mode",
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
    "aux1_level",
    "aux1_mute",
    "aux2_level",
    "aux2_mute",
    "aux3_level",
    "aux3_mute",
    "aux4_level",
    "aux4_mute",
    "aux5_level",
    "aux5_mute",
    "aux6_level",
    "aux6_mute",
    "aux7_level",
    "aux7_mute",
    "aux8_level",
    "aux8_mute",
    "fx1_level",
    "fx1_mute",
    "fx2_level",
    "fx2_mute",
    "fx3_level",
    "fx3_mute",
    "fx4_level",
    "fx4_mute",
    "send1_level",
    "send1_pan",
    "send2_level",
    "send2_pan",
    "send3_level",
    "send3_pan",
    "send4_level",
    "send4_pan",
    "send5_level",
    "send5_pan",
    "send6_level",
    "send6_pan",
    "send7_level",
    "send7_pan",
    "send8_level",
    "send8_pan",
    "send9_level",
    "send10_level",
    "send11_level",
    "send12_level",
];

// Native row-major stock table. Clear is separate from the crossed-circle
// stock icon. Endpoints, keyboard and clear verified on CH7, 2026-10-26.
static DL32S_ICON_SYMBOLS: &[openfader_core::params::ParamSymbol] = &[
    openfader_core::params::ParamSymbol { value: 0.0, label: "Clear image", symbol: "none" },
    openfader_core::params::ParamSymbol { value: 1000.0, label: "Kick drum", symbol: "kick" },
    openfader_core::params::ParamSymbol { value: 1001.0, label: "Snare drum", symbol: "snare" },
    openfader_core::params::ParamSymbol { value: 1002.0, label: "Tom drum", symbol: "tom" },
    openfader_core::params::ParamSymbol { value: 1003.0, label: "Hi-hat", symbol: "hihat" },
    openfader_core::params::ParamSymbol { value: 1004.0, label: "Cymbals", symbol: "cymbals" },
    openfader_core::params::ParamSymbol { value: 1005.0, label: "Drum kit", symbol: "drumkit" },
    openfader_core::params::ParamSymbol { value: 1006.0, label: "Cowbell", symbol: "cowbell" },
    openfader_core::params::ParamSymbol { value: 1007.0, label: "Bass guitar 1", symbol: "bass1" },
    openfader_core::params::ParamSymbol { value: 1008.0, label: "Bass guitar 2", symbol: "bass2" },
    openfader_core::params::ParamSymbol { value: 1009.0, label: "Guitar 1", symbol: "guitar1" },
    openfader_core::params::ParamSymbol { value: 1010.0, label: "Guitar 2", symbol: "guitar2" },
    openfader_core::params::ParamSymbol { value: 1011.0, label: "Guitar 3", symbol: "guitar3" },
    openfader_core::params::ParamSymbol { value: 1012.0, label: "Acoustic guitar 1", symbol: "acoustic1" },
    openfader_core::params::ParamSymbol { value: 1013.0, label: "Acoustic guitar 2", symbol: "acoustic2" },
    openfader_core::params::ParamSymbol { value: 1014.0, label: "Bowed strings 1", symbol: "strings1" },
    openfader_core::params::ParamSymbol { value: 1015.0, label: "Bowed strings 2", symbol: "strings2" },
    openfader_core::params::ParamSymbol { value: 1016.0, label: "Trumpet", symbol: "trumpet" },
    openfader_core::params::ParamSymbol { value: 1017.0, label: "Grand piano", symbol: "piano" },
    openfader_core::params::ParamSymbol { value: 1018.0, label: "Keyboards", symbol: "keyboards" },
    openfader_core::params::ParamSymbol { value: 1019.0, label: "Synthesizer", symbol: "synth" },
    openfader_core::params::ParamSymbol { value: 1020.0, label: "Amplifier", symbol: "amp" },
    openfader_core::params::ParamSymbol { value: 1021.0, label: "Vocalist 1", symbol: "vocal1" },
    openfader_core::params::ParamSymbol { value: 1022.0, label: "Vocalist 2", symbol: "vocal2" },
    openfader_core::params::ParamSymbol { value: 1023.0, label: "Vocal group", symbol: "group" },
    openfader_core::params::ParamSymbol { value: 1024.0, label: "Phone", symbol: "phone" },
    openfader_core::params::ParamSymbol { value: 1025.0, label: "Wedge monitor 1", symbol: "wedge" },
    openfader_core::params::ParamSymbol { value: 1026.0, label: "Wedge monitor 2", symbol: "wedges" },
    openfader_core::params::ParamSymbol { value: 1027.0, label: "Mixer", symbol: "mixer" },
    openfader_core::params::ParamSymbol { value: 1028.0, label: "Arches", symbol: "arches" },
    openfader_core::params::ParamSymbol { value: 1029.0, label: "Clock", symbol: "clock" },
    openfader_core::params::ParamSymbol { value: 1030.0, label: "PA speakers", symbol: "pa" },
    openfader_core::params::ParamSymbol { value: 1031.0, label: "Speaker", symbol: "speaker" },
    openfader_core::params::ParamSymbol { value: 1032.0, label: "Crossed circle", symbol: "prohibited" },
];

// Native input palette order; CH7 endpoints and None verified 2026-10-26.
static DL32S_COLOR_SWATCHES: &[openfader_core::params::ParamSwatch] = &[
    openfader_core::params::ParamSwatch { value: -1.0, label: "None", rgb: None },
    openfader_core::params::ParamSwatch { value: 0.0, label: "Pink", rgb: Some(0xed1176) },
    openfader_core::params::ParamSwatch { value: 1.0, label: "Coral", rgb: Some(0xf16464) },
    openfader_core::params::ParamSwatch { value: 2.0, label: "Red", rgb: Some(0xee3b25) },
    openfader_core::params::ParamSwatch { value: 3.0, label: "Orange", rgb: Some(0xf56522) },
    openfader_core::params::ParamSwatch { value: 4.0, label: "Amber", rgb: Some(0xffb52d) },
    openfader_core::params::ParamSwatch { value: 5.0, label: "Gold", rgb: Some(0xffd100) },
    openfader_core::params::ParamSwatch { value: 6.0, label: "Yellow", rgb: Some(0xf9ef42) },
    openfader_core::params::ParamSwatch { value: 7.0, label: "Lime", rgb: Some(0xa8d25d) },
    openfader_core::params::ParamSwatch { value: 8.0, label: "Green", rgb: Some(0x54bf79) },
    openfader_core::params::ParamSwatch { value: 9.0, label: "Mint", rgb: Some(0x8fd3b2) },
    openfader_core::params::ParamSwatch { value: 10.0, label: "Teal", rgb: Some(0x22bcb5) },
    openfader_core::params::ParamSwatch { value: 11.0, label: "Cyan", rgb: Some(0x65cee3) },
    openfader_core::params::ParamSwatch { value: 12.0, label: "Sky blue", rgb: Some(0x1baed1) },
    openfader_core::params::ParamSwatch { value: 13.0, label: "Blue", rgb: Some(0x357dbe) },
    openfader_core::params::ParamSwatch { value: 14.0, label: "Lavender", rgb: Some(0xa782ba) },
    openfader_core::params::ParamSwatch { value: 15.0, label: "Purple", rgb: Some(0x9156a5) },
    openfader_core::params::ParamSwatch { value: 16.0, label: "White", rgb: Some(0xffffff) },
];

/// Canonical id -> documented modern-DL32S `(min, max)`, served through
/// [`MixerDriver::param_range_overrides`](openfader_core::MixerDriver::param_range_overrides).
/// Provenance per entry (manufacturer mixer software 5.1 manual unless noted):
/// - `gain 0..60`: p90 mic/balanced slider (CH7 MicPre native 0 observed).
/// - `trim -40..+20`: p91.
/// - `hpf_freq 20..700`: p103 Modern EQ; CH7 endpoints verified against
///   manufacturer mixer software 5.2 with independent readback and restore 2026-10-25.
/// - `gate_att 0.1..300`, `gate_hold 0..5000`, `gate_rel 50..3000` ms,
///   `gate_range 0..60` dB: native CH7 endpoints independently read and
///   restored 2026-10-25. Vintage timing selections share these values.
/// - `gate_mode 0..1`: CH7 native Modern/Vintage selector verified with
///   independent readback and restore 2026-10-25 (canonical has five modes).
/// - `eqN_q 0.5..16`: p103 Modern EQ (all four bands).
/// - `comp_thr -80..0`: p112 Modern compressor.
/// - `comp_makeup 0..20`: p112 Modern compressor gain.
///
/// EQ gains need no entry (canonical already documents ±15 per p103);
/// `gate_thr` needs none (canonical already -80..0 per p112).
static DL32S_RANGE_OVERRIDES: &[(&str, f32, f32)] = &[
    ("icon", 0.0, 1032.0), // 0 clears; stock symbols use 1000..1032
    ("color", -1.0, 16.0), // None plus 17 native swatches; CH7 verified 2026-10-26
    ("gain", 0.0, 60.0),
    ("trim", -40.0, 20.0),
    ("hpf_freq", 20.0, 700.0),
    ("gate_mode", 0.0, 1.0),
    ("gate_att", 0.1, 300.0),
    ("gate_hold", 0.0, 5000.0),
    ("gate_rel", 50.0, 3000.0),
    ("gate_range", 0.0, 60.0),
    ("eq1_q", 0.5, 16.0),
    ("eq2_q", 0.5, 16.0),
    ("eq3_q", 0.5, 16.0),
    ("eq4_q", 0.5, 16.0),
    ("comp_mode", 0.0, 1.0),
    ("comp_knee", 0.0, 1.0),
    ("comp_att", 0.1, 300.0),
    ("comp_rel", 50.0, 3000.0),
    ("comp_ratio", 1.0, 101.0), // native 101 = inf:1, verified 2026-10-25
    ("comp_thr", -80.0, 0.0),
    ("comp_makeup", 0.0, 20.0),
];

// Input labels from native routing captures (physical, USB, oscillator, None).
// Source labels describe the routed ID, never the destination strip number.
macro_rules! input_source_labels {
    ($($index:literal => $number:literal),+ $(,)?) => {
        &[
            openfader_core::params::ParamChoice { value: 0.0, label: "None" },
            openfader_core::params::ParamChoice { value: 17000.0, label: "Oscillator" },
            $(openfader_core::params::ParamChoice {
                value: (1000 + $index) as f32, label: concat!("Mic Pre ", $number),
            }, openfader_core::params::ParamChoice {
                value: (2000 + $index) as f32, label: concat!("USB ", $number),
            },)+
        ]
    };
}

static DL32S_SOURCE_LABELS: &[openfader_core::params::ParamChoice] = input_source_labels!(
    0 => 1, 1 => 2, 2 => 3, 3 => 4, 4 => 5, 5 => 6, 6 => 7, 7 => 8,
    8 => 9, 9 => 10, 10 => 11, 11 => 12, 12 => 13, 13 => 14, 14 => 15, 15 => 16,
    16 => 17, 17 => 18, 18 => 19, 19 => 20, 20 => 21, 21 => 22, 22 => 23, 23 => 24,
    24 => 25, 25 => 26, 26 => 27, 27 => 28, 28 => 29, 29 => 30, 30 => 31, 31 => 32,
);

// A/B slots can each be patched to a preamp or a digital source. Only
// a selected physical preamp exposes gain/phantom, irrespective of slot.
const DL32S_SELECTED_PREAMP: openfader_core::params::ParamCondition = {
    use openfader_core::params::ParamCondition::*;
    Any { any: &[
        All { all: &[
            Equals { id: "source_select", value: 1.0 },
            Range { id: "source_a", min: 1000.0, max: 1031.0 },
        ] },
        All { all: &[
            Equals { id: "source_select", value: 0.0 },
            Range { id: "source_b", min: 1000.0, max: 1031.0 },
        ] },
    ] }
};

// Native CH7 selections, independently read and restored 2026-10-25.
// Unmatched timings retain their exact value; clients must not snap them
// to a named selection merely by displaying this metadata.
static DL32S_PARAM_PRESENTATIONS: &[openfader_core::params::ParamPresentation] = &[
    openfader_core::params::ParamPresentation {
        id: "gain", when: None, hidden: true, curve: None, limiter_value: None,
        choices: &[], value_labels: &[],
    },
    openfader_core::params::ParamPresentation {
        id: "gain", when: Some(DL32S_SELECTED_PREAMP), hidden: false, curve: None, limiter_value: None,
        choices: &[], value_labels: &[],
    },
    openfader_core::params::ParamPresentation {
        id: "phantom", when: None, hidden: true, curve: None, limiter_value: None,
        choices: &[], value_labels: &[],
    },
    openfader_core::params::ParamPresentation {
        id: "phantom", when: Some(DL32S_SELECTED_PREAMP), hidden: false, curve: None, limiter_value: None,
        choices: &[], value_labels: &[],
    },
    openfader_core::params::ParamPresentation {
        id: "source_a", when: None, hidden: false, curve: None, limiter_value: None,
        choices: &[], value_labels: DL32S_SOURCE_LABELS,
    },
    openfader_core::params::ParamPresentation {
        id: "source_b", when: None, hidden: false, curve: None, limiter_value: None,
        choices: &[], value_labels: DL32S_SOURCE_LABELS,
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_mode", when: None, hidden: false, curve: None, value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 0.0, label: "Modern" }, openfader_core::params::ParamChoice { value: 1.0, label: "Vintage" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_knee", when: None, hidden: false, curve: None, value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 0.0, label: "Soft Knee" }, openfader_core::params::ParamChoice { value: 1.0, label: "Hard Knee" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_att", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_mode", value: 1.0 }), hidden: false, curve: None, value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 0.1, label: "Fast" }, openfader_core::params::ParamChoice { value: 50.0, label: "Medium" }, openfader_core::params::ParamChoice { value: 300.0, label: "Slow" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_rel", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_mode", value: 1.0 }), hidden: false, curve: None, value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 50.0, label: "Fast" }, openfader_core::params::ParamChoice { value: 500.0, label: "Medium" }, openfader_core::params::ParamChoice { value: 3000.0, label: "Slow" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_ratio", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_mode", value: 1.0 }), hidden: false, curve: None, value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 2.0, label: "2:1" }, openfader_core::params::ParamChoice { value: 4.0, label: "4:1" }, openfader_core::params::ParamChoice { value: 8.0, label: "8:1" }, openfader_core::params::ParamChoice { value: 12.0, label: "12:1" }, openfader_core::params::ParamChoice { value: 20.0, label: "20:1" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_knee", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_knee", value: 1.0 }), hidden: false,
        curve: Some("hard_knee"), value_labels: &[],
        choices: &[openfader_core::params::ParamChoice { value: 0.0, label: "Soft Knee" }, openfader_core::params::ParamChoice { value: 1.0, label: "Hard Knee" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        id: "comp_knee", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_mode", value: 1.0 }), hidden: true, curve: None, value_labels: &[],
        choices: &[],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: Some(101.0),
        id: "comp_ratio", when: Some(openfader_core::params::ParamCondition::Equals { id: "comp_mode", value: 0.0 }), hidden: false, curve: None,
        choices: &[], value_labels: &[openfader_core::params::ParamChoice { value: 101.0, label: "∞:1" }],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        value_labels: &[],
        id: "gate_mode", when: None, hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 0.0, label: "Modern" },
            openfader_core::params::ParamChoice { value: 1.0, label: "Vintage" },
        ],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        value_labels: &[],
        id: "gate_mode", when: Some(openfader_core::params::ParamCondition::Equals { id: "gate_mode", value: 0.0 }), hidden: false,
        curve: Some("gate_attenuation"), choices: &[
            openfader_core::params::ParamChoice { value: 0.0, label: "Modern" },
            openfader_core::params::ParamChoice { value: 1.0, label: "Vintage" },
        ],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        value_labels: &[],
        id: "gate_att", when: Some(openfader_core::params::ParamCondition::Equals { id: "gate_mode", value: 1.0 }), hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 0.1, label: "Fast" },
            openfader_core::params::ParamChoice { value: 50.0, label: "Medium" },
            openfader_core::params::ParamChoice { value: 300.0, label: "Slow" },
        ],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        value_labels: &[],
        id: "gate_rel", when: Some(openfader_core::params::ParamCondition::Equals { id: "gate_mode", value: 1.0 }), hidden: false, curve: None,
        choices: &[
            openfader_core::params::ParamChoice { value: 50.0, label: "Fast" },
            openfader_core::params::ParamChoice { value: 500.0, label: "Medium" },
            openfader_core::params::ParamChoice { value: 3000.0, label: "Slow" },
        ],
    },
    openfader_core::params::ParamPresentation {
        limiter_value: None,
        value_labels: &[],
        id: "gate_range", when: Some(openfader_core::params::ParamCondition::Equals { id: "gate_mode", value: 1.0 }), hidden: true, choices: &[], curve: None,
    },
];

impl Dl32sDriver {
    /// Provider rows extend the canonical catalogue with source IDs,
    /// model selection and additional assignment groups. Canonical IDs
    /// retain precedence when a future catalogue adds an equivalent row.
    pub(super) fn metadata_param_defs(&self) -> &'static [openfader_core::params::ParamDef] {
        static DEFS: std::sync::OnceLock<Vec<openfader_core::params::ParamDef>> =
            std::sync::OnceLock::new();
        DEFS.get_or_init(|| {
            let source = |id, max, label| openfader_core::params::ParamDef {
                id,
                kind: openfader_core::params::ParamKind::Enum,
                scope: openfader_core::params::ParamScope::Channel,
                scopes: openfader_core::params::SCOPES_INPUT,
                min: 0.0,
                max,
                unit: "",
                default: openfader_core::params::ParamDefault::Float(0.0),
                label,
            };
            let mut defs = vec![
                openfader_core::params::ParamDef {
                    id: "eq_mode",
                    kind: openfader_core::params::ParamKind::Enum,
                    scope: openfader_core::params::ParamScope::Channel,
                    scopes: openfader_core::params::SCOPES_EQ,
                    min: 0.0,
                    max: 1.0,
                    unit: "",
                    default: openfader_core::params::ParamDefault::Float(0.0),
                    label: "EQ modern/vintage",
                },
                // Source IDs remain native integers; routing writes are restricted
                // to verified physical-preamp, USB, oscillator and None values.
                source("source_a", u32::MAX as f32, "Input A source ID"),
                source("source_b", u32::MAX as f32, "Input B source ID"),
                source("source_select", 1.0, "Input A/B"),
            ];
            for (id, label) in [
                ("dca5_assign", "VCA 5"), ("dca6_assign", "VCA 6"),
                ("view1_assign", "View A"), ("view2_assign", "View B"),
                ("view3_assign", "View C"), ("view4_assign", "View D"),
                ("view5_assign", "View E"), ("view6_assign", "View F"),
            ] {
                defs.push(openfader_core::params::ParamDef { id, label,
                    kind: openfader_core::params::ParamKind::Bool,
                    scope: openfader_core::params::ParamScope::Channel,
                    scopes: openfader_core::params::SCOPES_INPUT,
                    min: 0.0, max: 1.0, unit: "",
                    default: openfader_core::params::ParamDefault::Bool(false),
                });
            }
            defs
        })
    }

    /// Documented modern-DL32S engineering-unit ranges (manufacturer mixer software 5.1
    /// manual; the desk runs native manufacturer mixer software 5.2 / mixer firmware 1.6).
    /// Applied over the canonical (X-Air-shaped) catalogue in this
    /// provider's registry only — sim and xair keep canonical ranges.
    /// Every entry below has known units and a direct wire encoding (dB
    /// leaves ride `Encoding::Db`, Hz/unitless leaves ride
    /// `Encoding::Float`); kinds, scopes, labels and defaults are
    /// untouched. Except for the live-verified HPF endpoints and Gate controls
    /// domain noted in `DL32S_RANGE_OVERRIDES`, bounds are documentation-
    /// based (hardware endpoint writes pending); the desk is believed to
    /// clamp outside its maxima.
    /// Deliberately absent (canonical kept, with reason):
    /// - Line-input `-20..40` effective attenuation (p90): no
    ///   input-socket detection exists, so the single mic/balanced `0..60`
    ///   slider is served (CH7 MicPre native 0 observed, consistent); the
    ///   existing wire bound is retained without assuming how the physical
    ///   offset is represented.
    ///
    /// Compressor endpoints, model/knee enums and Vintage choices are
    /// independently hardware-verified on CH7, 2026-10-25. Native inf:1 is
    /// the finite wire value 101, labelled through presentation metadata.
    /// EQ mode labels and curve semantics remain unverified.
    pub(super) fn metadata_param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        DL32S_RANGE_OVERRIDES
    }

    pub(super) fn metadata_param_symbols(&self, param: &str) -> &'static [openfader_core::params::ParamSymbol] {
        if param == "icon" { DL32S_ICON_SYMBOLS } else { &[] }
    }

    pub(super) fn metadata_param_text_rules(&self, param: &str) -> Option<openfader_core::params::ParamTextRules> {
        (param == "name").then_some(openfader_core::params::ParamTextRules {
            max_utf8_bytes: Some(63), allow_empty: true, allow_edge_whitespace: false,
        })
    }

    pub(super) fn metadata_param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        if param == "color" { DL32S_COLOR_SWATCHES } else { &[] }
    }

    pub(super) fn metadata_param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        DL32S_PARAM_PRESENTATIONS
    }
}
