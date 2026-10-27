//! Provider-neutral reference global controls, used by the Rust simulation.
//! Providers opt in to individual descriptors and own their wire mapping.
//! Domains follow the directly compiled DL32S app 5.2 table (2026-10-27
//! resource-protocol evidence), not new hardware validation. No routing envelope
//! is treated as a continuous source domain. No talkback-mode wire leaf exists.
use crate::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};

pub fn reference_controls() -> Vec<DestinationControl> {
    let mut rows = Vec::new();
    let mut add = |target: &str,
                   target_label: &str,
                   id: &str,
                   label: &str,
                   default: f32,
                   choices: Vec<DestinationChoice>,
                   range: Option<DestinationRange>| {
        rows.push(DestinationControl {
            revision: Some(0), generation: Some(0), preset: false,
            target: target.into(), target_label: target_label.into(), target_name: None,
            send_slot: None, surface: "global".into(), id: id.into(), label: label.into(),
            gesture: (target == "talkback" && id == "enable").then_some("talkback"),
            tap_tempo: false,
            description: match target {
                "oscillator" => "Shared signal generator. Enabling can produce audio on existing routes; level and routing are independent.",
                "talkback" => "Shared talkback configuration. Enable supports local Momentary, Latch and Auto gestures. Mode is a local preference, not a mixer parameter.",
                "solo" => "Shared solo-bus configuration. Solo in place can affect the main mix.",
                _ => "Shared monitor configuration. LR defeat uses the native polarity; distance is not a time in milliseconds.",
            }.into(),
            choices, range, current: Some(DestinationValue { value: default, verified: true }),
        });
    };
    add(
        "solo",
        "Solo",
        "mode",
        "Solo mode",
        2.,
        choices(&["Single", "Multiple inputs", "Multiple inputs and outputs"]),
        None,
    );
    add(
        "solo",
        "Solo",
        "location",
        "Solo location",
        2.,
        choices(&["Pre fader", "After fader", "PFL inputs / AFL outputs"]),
        None,
    );
    add(
        "solo",
        "Solo",
        "in_place",
        "Solo in place",
        0.,
        boolean(),
        None,
    );
    add(
        "solo",
        "Solo",
        "level",
        "Solo level",
        0.,
        vec![],
        range(-120., 10., "dB", 2),
    );
    add(
        "oscillator",
        "Oscillator",
        "enable",
        "Enable",
        0.,
        boolean(),
        None,
    );
    add(
        "oscillator",
        "Oscillator",
        "level",
        "Level",
        -120.,
        vec![],
        range(-120., 0., "dB", 2),
    );
    add(
        "oscillator",
        "Oscillator",
        "waveform",
        "Waveform",
        0.,
        choices(&[
            "Pink noise",
            "White noise",
            "Sine 100 Hz",
            "Sine 1 kHz",
            "Sine 10 kHz",
            "Variable sine",
        ]),
        None,
    );
    add(
        "oscillator",
        "Oscillator",
        "frequency",
        "Variable frequency",
        100.,
        vec![],
        range(20., 20000., "Hz", 2),
    );
    // Confirmed physical/USB families, None and generator only. Do not offer
    // Dante, matrix or every integer in the compiled 0..17000 envelope.
    let mut sources = vec![DestinationChoice {
        value: 0.,
        label: "None".into(),
    }];
    for (base, label) in [(1000, "Mic Pre"), (2000, "USB")] {
        for index in 0..32 {
            sources.push(DestinationChoice {
                value: (base + index) as f32,
                label: format!("{label} {}", index + 1),
            });
        }
    }
    sources.push(DestinationChoice {
        value: 17000.,
        label: "Oscillator".into(),
    });
    add(
        "talkback", "Talkback", "source", "Source", 1031., sources, None,
    );
    add(
        "talkback",
        "Talkback",
        "enable",
        "Enable (latch)",
        0.,
        boolean(),
        None,
    );
    add(
        "talkback",
        "Talkback",
        "level",
        "Level",
        0.,
        vec![],
        range(-120., 10., "dB", 2),
    );
    add(
        "talkback",
        "Talkback",
        "assign_lr",
        "Assign Main LR",
        0.,
        boolean(),
        None,
    );
    for (family, label, count) in [("aux", "Aux", 8), ("subgroup", "Subgroup", 6)] {
        for index in 1..=count {
            add(
                "talkback",
                "Talkback",
                &format!("assign_{family}_{index}"),
                &format!("Assign {label} {index}"),
                0.,
                boolean(),
                None,
            );
        }
    }
    add(
        "monitor",
        "Monitor",
        "lr_defeat",
        "LR defeat",
        0.,
        boolean(),
        None,
    );
    add(
        "monitor",
        "Monitor",
        "level",
        "Monitor level",
        0.,
        vec![],
        range(-120., 10., "dB", 2),
    );
    add(
        "monitor",
        "Monitor",
        "delay_enable",
        "Delay enable",
        1.,
        boolean(),
        None,
    );
    // Native distance domain. Metres are suggested by a named taper, but
    // labelled UI conversion has not been traced: keep the unit honest.
    add(
        "monitor",
        "Monitor",
        "distance",
        "Delay distance (native)",
        0.,
        vec![],
        range(0., 115.85, "native distance", 6),
    );
    rows
}
fn choices(labels: &[&str]) -> Vec<DestinationChoice> {
    labels
        .iter()
        .enumerate()
        .map(|(value, label)| DestinationChoice {
            value: value as f32,
            label: (*label).into(),
        })
        .collect()
}
fn boolean() -> Vec<DestinationChoice> {
    choices(&["Off", "On"])
}
fn range(min: f32, max: f32, unit: &str, decimals: u8) -> Option<DestinationRange> {
    Some(DestinationRange {
        min,
        max,
        unit: unit.into(),
        decimals,
    })
}
