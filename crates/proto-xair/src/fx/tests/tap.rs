use super::*;

#[test]
fn tap_targets_cover_confident_delays_only() {
    let dly = fx_tap_target(10).unwrap();
    assert_eq!(
        (dly.par, dly.min_ms, dly.max_ms, dly.subdiv),
        (2, 1.0, 3000.0, true)
    );
    for index in [11u8, 12, 22, 25, 26, 27] {
        let t = fx_tap_target(index).unwrap();
        assert_eq!(
            (t.par, t.min_ms, t.max_ms, t.subdiv),
            (1, 1.0, 3000.0, true)
        );
    }
    for index in [0u8, 1, 7, 13, 14, 16, 34, 59, 60, 15, 99] {
        assert!(fx_tap_target(index).is_none(), "index {index}");
    }
}

#[test]
fn tap_bpm_needs_four_taps_in_window() {
    assert!((tap_bpm_from_intervals_ms(&[500.0, 500.0, 500.0]).unwrap() - 120.0).abs() < 1e-9);
    assert!((tap_bpm_from_intervals_ms(&[250.0, 250.0, 250.0]).unwrap() - 240.0).abs() < 1e-9);
    assert!(tap_bpm_from_intervals_ms(&[500.0, 500.0]).is_none());
    assert!(tap_bpm_from_intervals_ms(&[]).is_none());
    assert!(tap_bpm_from_intervals_ms(&[0.0, 500.0, 500.0]).is_none());
    assert!(tap_bpm_from_intervals_ms(&[50.0, 50.0, 50.0]).is_none()); // >300 BPM
    assert!(tap_bpm_from_intervals_ms(&[5000.0, 5000.0, 5000.0]).is_none()); // <20 BPM
    assert!((tap_delay_ms(120.0, TapSubdiv::Quarter) - 500.0).abs() < 1e-9);
    assert!((tap_delay_ms(120.0, TapSubdiv::Dotted8th) - 375.0).abs() < 1e-9);
    assert!((tap_ms_to_normalized(500.0, 1.0, 3000.0) - 499.0 / 2999.0) < 1e-6);
    assert_eq!(tap_ms_to_normalized(1.0, 1.0, 3000.0), 0.0);
    assert_eq!(tap_ms_to_normalized(3000.0, 1.0, 3000.0), 1.0);
    assert_eq!(tap_ms_to_normalized(9999.0, 1.0, 3000.0), 1.0);
    assert_eq!(tap_ms_to_normalized(0.5, 1.0, 3000.0), 0.0);
    assert_eq!(tap_ms_to_normalized(f64::NAN, 1.0, 3000.0), 0.0);
}
