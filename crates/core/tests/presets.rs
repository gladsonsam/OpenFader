//! Presets apply against the SimDriver (integration: one core build).
use openmix_core::presets::{PRESETS, apply_preset, preset_by_id};
use openmix_core::{MixerDriver, StripId};

#[tokio::test]
async fn every_preset_applies_to_sim() {
    let d = openmix_sim::SimDriver::new();
    let strip = StripId(0);
    for p in PRESETS {
        apply_preset(&d, strip, p)
            .await
            .unwrap_or_else(|e| panic!("preset {}: {e}", p.id));
        // Every field reads back exactly what the preset set.
        for s in p.settings {
            let got = d
                .get_param(strip, s.field)
                .await
                .unwrap_or_else(|e| panic!("preset {} read {}: {e}", p.id, s.field));
            assert_eq!(got, s.value, "preset {} field {}", p.id, s.field);
        }
    }
}

#[tokio::test]
async fn unknown_preset_id_resolves_none() {
    assert!(preset_by_id("nope").is_none());
    assert_eq!(PRESETS.len(), 10);
}
