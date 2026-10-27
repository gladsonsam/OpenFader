//! Stable hard-solo identities. A VCA flag never fabricates member input flags.
use crate::destinations::{DestinationChoice, DestinationControl, DestinationValue};

pub fn is_solo(id: &str) -> bool {
    matches!(id, "solo" | "send_solo" | "return_solo")
}

/// Reference static domain, shared by the simulation and DL32S provider.
/// Main and matrix deliberately have no row. FX return is one stereo bit.
pub fn controls() -> Vec<DestinationControl> {
    let mut rows = Vec::new();
    for (family, label, count, id) in [
        ("ch", "Input", 32, "solo"),
        ("return", "Return", 2, "solo"),
        ("fx", "FX send", 4, "send_solo"),
        ("fx", "FX return", 4, "return_solo"),
        ("subgroup", "Subgroup", 6, "solo"),
        ("bus", "Aux", 8, "solo"),
        ("vca", "VCA", 6, "solo"),
    ] {
        for index in 1..=count {
            let target = format!("{family}/{index:02}");
            rows.push(DestinationControl {
                members: vec![target.clone()], target,
                target_label: format!("{label} {index}"), target_name: None,
                revision: Some(0), generation: Some(0), preset: false,
                surface: "solo".into(), send_slot: None, id: id.into(), label: format!("{label} solo"),
                description: "Own hard solo. VCA member highlighting is separate; linked partners are protected without extra writes.".into(),
                tap_tempo: false, gesture: None,
                choices: vec![DestinationChoice { value: 0., label: "Off".into() }, DestinationChoice { value: 1., label: "On".into() }],
                range: None, current: Some(DestinationValue { value: 0., verified: true }),
            });
        }
    }
    rows
}
