//! Documented X32 native control domains. See protocol 4.09-02 and X32CfgMain.h.
#[derive(Clone, Copy, Debug)]
pub enum ConsoleControlKind {
    Toggle,
    Choice(&'static [&'static str]),
    IQEq(&'static [&'static str]),
    CardChoice {
        choices: &'static [&'static str],
        card_types: &'static [i32],
    },
    IQSound {
        model_path: &'static str,
    },
    Bitset(&'static [(&'static str, u32)]),
    ReadOnlyChoice(&'static [&'static str]),
    ReadOnlyText(usize),
    UserAssignment(usize),
    Text(usize),
    Number {
        min: f32,
        max: f32,
        unit: &'static str,
        scale: NumberScale,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum NumberScale {
    Linear { step: f32 },
    Logarithmic { steps: u32 },
    Level,
}

#[derive(Clone, Copy, Debug)]
pub struct ConsoleControl {
    pub id: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    pub path: &'static str,
    pub kind: ConsoleControlKind,
}

const SOLO_MODE: &[&str] = &["PFL", "AFL"];
const SOLO_SOURCE: &[&str] = &["OFF", "LR", "LR+C", "LRPFL", "LRAFL", "AUX56", "AUX78"];
const MONO_MODE: &[&str] = &["LR+M", "LCR"];
const TALK_SOURCE: &[&str] = &["INT", "EXT"];
const OSC_FREQ: &[&str] = &["F1", "F2"];
const OSC_TYPE: &[&str] = &["SINE", "PINK", "WHITE"];
const DP48_SCOPE: &[(&str, u32)] = &[
    ("Group name", 1),
    ("Group assign", 2),
    ("Channel level", 4),
    ("Channel pan", 8),
];
const CLOCK_RATES: &[&str] = &["48K", "44K1"];
const CLOCK_SOURCES: &[&str] = &["INT", "AES50A", "AES50B", "Exp Card"];
const REMOTE_PROTOCOL: &[&str] = &["MC", "HUI", "CC"];
const REMOTE_PORT: &[&str] = &["MIDI", "CARD", "RTP"];
const SHOW_CONTROL: &[&str] = &["CUES", "SCENES", "SNIPPETS"];
const CLOCK_MODE: &[&str] = &["24h", "12h"];
const MUTE_LEDS: &[&str] = &["NORM", "INV"];
const RECORDER_CONTROL: &[&str] = &["USB", "XLIVE"];
const HA_FLAGS: &[(&str, u32)] = &[
    ("Lock stagebox", 1),
    ("X32 gain split", 2),
    ("AES50 A gain split", 4),
    ("AES50 B gain split", 8),
];
const REMOTE_FLAGS: &[(&str, u32)] = &[
    ("MIDI input/output", 1),
    ("Card MIDI", 2),
    ("RTP MIDI", 4),
    ("Receive program change", 8),
    ("Send program change", 16),
    ("Receive fader CC", 32),
    ("Send fader CC", 64),
    ("Receive mute CC", 128),
    ("Send mute CC", 256),
    ("Receive pan CC", 512),
    ("Send pan CC", 1024),
    ("OSC over MIDI SysEx", 2048),
    ("X-Touch over MIDI", 4096),
    ("X-Touch over Ethernet", 8192),
];
const TAPE_STATE: &[&str] = &["STOP", "PPAUSE", "PLAY", "RPAUSE", "RECORD", "FF", "REW"];
const OSC_DESTINATIONS: &[&str] = &[
    "Mix bus 1",
    "Mix bus 2",
    "Mix bus 3",
    "Mix bus 4",
    "Mix bus 5",
    "Mix bus 6",
    "Mix bus 7",
    "Mix bus 8",
    "Mix bus 9",
    "Mix bus 10",
    "Mix bus 11",
    "Mix bus 12",
    "Mix bus 13",
    "Mix bus 14",
    "Mix bus 15",
    "Mix bus 16",
    "Left",
    "Right",
    "Left + right",
    "Mono / center",
    "Matrix 1",
    "Matrix 2",
    "Matrix 3",
    "Matrix 4",
    "Matrix 5",
    "Matrix 6",
];

macro_rules! toggle {
    ($id:literal, $category:literal, $label:literal, $path:literal) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::Toggle,
        }
    };
}
macro_rules! choice {
    ($id:literal, $category:literal, $label:literal, $path:literal, $options:ident) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::Choice($options),
        }
    };
}
macro_rules! number {
    ($id:literal, $category:literal, $label:literal, $path:literal, $min:expr, $max:expr, $unit:literal, $scale:expr) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::Number {
                min: $min,
                max: $max,
                unit: $unit,
                scale: $scale,
            },
        }
    };
}
macro_rules! bitset {
    ($id:literal, $category:literal, $label:literal, $path:literal, $options:ident) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::Bitset($options),
        }
    };
}
macro_rules! readonly_choice {
    ($id:literal, $category:literal, $label:literal, $path:literal, $options:ident) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::ReadOnlyChoice($options),
        }
    };
}
macro_rules! readonly_text {
    ($id:literal, $category:literal, $label:literal, $path:literal, $max:expr) => {
        ConsoleControl {
            id: $id,
            category: $category,
            label: $label,
            path: $path,
            kind: ConsoleControlKind::ReadOnlyText($max),
        }
    };
}

/// Only documented, discrete leaf domains. OSC paths are not accepted as IDs.
#[rustfmt::skip]
pub const CONTROLS: &[ConsoleControl] = &[
    toggle!("remote.enabled", "Remote and MIDI", "Remote enabled", "/-prefs/remote/enable"),
    choice!("remote.protocol", "Remote and MIDI", "Remote protocol", "/-prefs/remote/protocol", REMOTE_PROTOCOL),
    choice!("remote.port", "Remote and MIDI", "Remote port", "/-prefs/remote/port", REMOTE_PORT),
    bitset!("remote.features", "Remote and MIDI", "Remote features", "/-prefs/remote/ioenable", REMOTE_FLAGS),
    bitset!("settings.headamp_flags", "Console settings", "Headamp settings", "/-prefs/haflags", HA_FLAGS),
    number!("settings.display_brightness", "Console settings", "Main display brightness", "/-prefs/bright", 10., 100., "%", NumberScale::Linear { step: 5.0 }),
    number!("settings.channel_contrast", "Console settings", "Channel display contrast", "/-prefs/lcdcont", 0., 100., "%", NumberScale::Linear { step: 2.0 }),
    number!("settings.led_brightness", "Console settings", "LED brightness", "/-prefs/ledbright", 10., 100., "%", NumberScale::Linear { step: 5.0 }),
    toggle!("settings.lamp_on", "Console settings", "Lamp power", "/-prefs/lampon"),
    toggle!("settings.return_to_last", "Console settings", "Return to last view", "/-prefs/viewrtn"),
    toggle!("settings.auto_select", "Console settings", "Auto select", "/-prefs/autosel"),
    choice!("settings.show_control", "Console settings", "Show control", "/-prefs/show_control", SHOW_CONTROL),
    choice!("settings.clock_display", "Console settings", "Clock display", "/-prefs/clockmode", CLOCK_MODE),
    choice!("settings.mute_leds", "Console settings", "Mute LEDs", "/-prefs/invertmutes", MUTE_LEDS),
    choice!("settings.recorder_display", "Console settings", "Displayed recorder", "/-prefs/rec_control", RECORDER_CONTROL),
    toggle!("talk.a.on", "Talkback", "Talk A", "/-stat/talk/A"),
    toggle!("talk.b.on", "Talkback", "Talk B", "/-stat/talk/B"),
    readonly_choice!("tape.state", "USB recorder", "Recorder state", "/-stat/tape/state", TAPE_STATE),
    readonly_text!("tape.file", "USB recorder", "Current file", "/-stat/tape/file", 1024),
    readonly_text!("tape.usb_path", "USB recorder", "USB path", "/-usb/path", 1024),
    readonly_text!("tape.usb_title", "USB recorder", "USB title", "/-usb/title", 1024),
    bitset!("dp48.scope", "DP48", "Broadcast scope", "/config/dp48/scope", DP48_SCOPE),
    number!("tape.gain_l", "USB recorder", "Playback gain left", "/config/tape/gainL", -6., 24., "dB", NumberScale::Linear { step: 0.5 }),
    number!("tape.gain_r", "USB recorder", "Playback gain right", "/config/tape/gainR", -6., 24., "dB", NumberScale::Linear { step: 0.5 }),
    toggle!("tape.autoplay", "USB recorder", "Folder playback", "/config/tape/autoplay"),
    toggle!("automix.x", "Automix", "Enable group X", "/config/amixenable/X"),
    toggle!("automix.y", "Automix", "Enable group Y", "/config/amixenable/Y"),
    choice!("settings.clock_rate", "Console settings", "Sample rate", "/-prefs/clockrate", CLOCK_RATES),
    choice!("settings.clock_source", "Console settings", "Clock source", "/-prefs/clocksource", CLOCK_SOURCES),
    toggle!("settings.confirm_general", "Console settings", "General confirmations", "/-prefs/confirm_general"),
    toggle!("settings.confirm_overwrite", "Console settings", "Overwrite confirmations", "/-prefs/confirm_overwrite"),
    toggle!("settings.confirm_scene", "Console settings", "Scene load confirmations", "/-prefs/confirm_sceneload"),
    toggle!("settings.select_follows_bank", "Console settings", "Selection follows bank", "/-prefs/selfollowsbank"),
    toggle!("settings.scene_advance", "Console settings", "Advance scene selection", "/-prefs/scene_advance"),
    toggle!("settings.safe_master_levels", "Console settings", "Safe master levels", "/-prefs/safe_masterlevels"),
    toggle!("settings.hard_mute", "Console settings", "Hard mute", "/-prefs/hardmute"),
    toggle!("settings.dca_mute", "Console settings", "DCA mute", "/-prefs/dcamute"),
    number!("monitor.level", "Monitor and solo", "Monitor level", "/config/solo/level", -90., 10., "dB", NumberScale::Level),
    number!("monitor.source_trim", "Monitor and solo", "Monitor source trim", "/config/solo/sourcetrim", -18., 18., "dB", NumberScale::Linear { step: 0.5 }),
    number!("monitor.dim_attenuation", "Monitor and solo", "Dim attenuation", "/config/solo/dimatt", -40., 0., "dB", NumberScale::Linear { step: 1.0 }),
    number!("monitor.delay_time", "Monitor and solo", "Monitor delay time", "/config/solo/delaytime", 0.3, 500., "ms", NumberScale::Linear { step: 0.1 }),
    number!("talk.a.level", "Talkback", "Talk A level", "/config/talk/A/level", -90., 10., "dB", NumberScale::Level),
    number!("talk.b.level", "Talkback", "Talk B level", "/config/talk/B/level", -90., 10., "dB", NumberScale::Level),
    number!("osc.level", "Oscillator", "Oscillator level", "/config/osc/level", -90., 10., "dB", NumberScale::Level),
    number!("osc.f1", "Oscillator", "Frequency 1", "/config/osc/f1", 20., 20000., "Hz", NumberScale::Logarithmic { steps: 121 }),
    number!("osc.f2", "Oscillator", "Frequency 2", "/config/osc/f2", 20., 20000., "Hz", NumberScale::Logarithmic { steps: 121 }),
    choice!("osc.destination", "Oscillator", "Oscillator destination", "/config/osc/dest", OSC_DESTINATIONS),
    choice!("mono.mode", "Main output", "Mono bus mode", "/config/mono/mode", MONO_MODE),
    toggle!("mono.link", "Main output", "Link mono to main", "/config/mono/link"),
    toggle!("link.preamp_delay", "Stereo linking", "Preamp and delay", "/config/linkcfg/hadly"),
    toggle!("link.eq", "Stereo linking", "Equalizer", "/config/linkcfg/eq"),
    toggle!("link.dynamics", "Stereo linking", "Dynamics", "/config/linkcfg/dyn"),
    toggle!("link.fader_mute", "Stereo linking", "Fader and mute", "/config/linkcfg/fdrmute"),
    choice!("monitor.source", "Monitor and solo", "Monitor source", "/config/solo/source", SOLO_SOURCE),
    choice!("monitor.channel_mode", "Monitor and solo", "Channel solo mode", "/config/solo/chmode", SOLO_MODE),
    choice!("monitor.bus_mode", "Monitor and solo", "Bus solo mode", "/config/solo/busmode", SOLO_MODE),
    choice!("monitor.dca_mode", "Monitor and solo", "DCA solo mode", "/config/solo/dcamode", SOLO_MODE),
    toggle!("monitor.exclusive", "Monitor and solo", "Exclusive solo", "/config/solo/exclusive"),
    toggle!("monitor.follow_selection", "Monitor and solo", "Solo follows selection", "/config/solo/followsel"),
    toggle!("monitor.follow_solo", "Monitor and solo", "Selection follows solo", "/config/solo/followsolo"),
    toggle!("monitor.dim", "Monitor and solo", "Monitor dim", "/config/solo/dim"),
    toggle!("monitor.mono", "Monitor and solo", "Monitor mono", "/config/solo/mono"),
    toggle!("monitor.delay", "Monitor and solo", "Monitor delay", "/config/solo/delay"),
    toggle!("monitor.master_control", "Monitor and solo", "Monitor master control", "/config/solo/masterctrl"),
    toggle!("monitor.mute", "Monitor and solo", "Monitor mute", "/config/solo/mute"),
    toggle!("monitor.dim_pfl", "Monitor and solo", "Dim PFL", "/config/solo/dimpfl"),
    toggle!("talk.enable", "Talkback", "Talkback enabled", "/config/talk/enable"),
    choice!("talk.source", "Talkback", "Talkback source", "/config/talk/source", TALK_SOURCE),
    toggle!("talk.a.dim", "Talkback", "Talk A dim", "/config/talk/A/dim"),
    toggle!("talk.a.latch", "Talkback", "Talk A latch", "/config/talk/A/latch"),
    toggle!("talk.b.dim", "Talkback", "Talk B dim", "/config/talk/B/dim"),
    toggle!("talk.b.latch", "Talkback", "Talk B latch", "/config/talk/B/latch"),
    toggle!("osc.on", "Oscillator", "Oscillator on", "/-stat/osc/on"),
    choice!("osc.frequency_select", "Oscillator", "Oscillator frequency", "/config/osc/fsel", OSC_FREQ),
    choice!("osc.type", "Oscillator", "Oscillator type", "/config/osc/type", OSC_TYPE),
];

pub(super) fn control(id: &str) -> anyhow::Result<&'static ConsoleControl> {
    let id = id.strip_prefix('/').unwrap_or(id);
    CONTROLS
        .iter()
        .chain(super::dp48::CONTROLS.iter())
        .chain(super::iq::CONTROLS.iter())
        .chain(super::user_assign::CONTROLS.iter())
        .chain(super::card::CONTROLS.iter())
        .find(|row| row.id == id)
        .ok_or_else(|| anyhow::anyhow!("x32: unknown console control"))
}
