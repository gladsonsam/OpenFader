use super::*;

#[test]
fn effect_names_cover_wire_order() {
    let named: Vec<u8> = (0..=60).filter(|i| fx_effect_name(*i).is_some()).collect();
    assert_eq!(named.len(), 60);
    assert!(!named.contains(&15));
    assert_eq!(fx_effect_name(0), Some("HALL"));
    assert_eq!(fx_effect_name(10), Some("DLY"));
    assert_eq!(fx_effect_name(16), Some("PHAS"));
    assert_eq!(fx_effect_name(28), Some("GEQ2"));
    assert_eq!(fx_effect_name(43), Some("FAC1M"));
    assert_eq!(fx_effect_name(47), Some("ULC2"));
    assert_eq!(fx_effect_name(48), Some("ENH2"));
    assert_eq!(fx_effect_name(59), Some("PIT2"));
    assert_eq!(fx_effect_name(60), Some("PIT"));
    assert_eq!(fx_effect_name(61), None);
    assert_eq!(fx_effect_name(255), None);
}

#[test]
fn full_names_cover_wire_order() {
    let named: Vec<u8> = (0..=60)
        .filter(|i| fx_effect_full_name(*i).is_some())
        .collect();
    assert_eq!(named.len(), 60);
    assert!(!named.contains(&15));
    for i in named {
        let full = fx_effect_full_name(i).unwrap();
        assert!(!full.is_empty(), "index {i}");
        assert_ne!(full, fx_effect_name(i).unwrap(), "index {i} full == code");
    }
    assert_eq!(fx_effect_full_name(0), Some("Hall Reverb"));
    assert_eq!(fx_effect_full_name(2), Some("Rich Plate Reverb"));
    assert_eq!(fx_effect_full_name(10), Some("Stereo Delay"));
    assert_eq!(fx_effect_full_name(11), Some("3-Tap Delay"));
    assert_eq!(fx_effect_full_name(12), Some("Rhythm Delay"));
    assert_eq!(fx_effect_full_name(20), Some("Tremolo / Panner"));
    assert_eq!(fx_effect_full_name(21), Some("Suboctaver"));
    assert_eq!(fx_effect_full_name(22), Some("Delay + Chamber"));
    assert_eq!(fx_effect_full_name(27), Some("Modulation Delay"));
    assert_eq!(fx_effect_full_name(43), Some("M/S Fair Compressor"));
    assert_eq!(fx_effect_full_name(47), Some("Dual Ultimo Compressor"));
    assert_eq!(fx_effect_full_name(53), Some("Edison EX1"));
    assert_eq!(fx_effect_full_name(54), Some("Sound Maxer"));
    assert_eq!(fx_effect_full_name(60), Some("Stereo Pitch Shifter"));
    assert_eq!(fx_effect_full_name(15), None);
    assert_eq!(fx_effect_full_name(61), None);
}

#[test]
fn used_counts_match_notes() {
    let cases: &[(u8, u8)] = &[
        (0, 12),
        (1, 10),
        (2, 16),
        (7, 13),
        (9, 9),
        (11, 15),
        (13, 11),
        (14, 12),
        (16, 12),
        (17, 7),
        (19, 8),
        (21, 10),
        (27, 13),
        (28, 64),
        (29, 32),
        (32, 6),
        (34, 11),
        (35, 22),
        (36, 9),
        (37, 18),
        (39, 8),
        (40, 29),
        (41, 58),
        (42, 7),
        (43, 14),
        (45, 5),
        (46, 10),
        (47, 12),
        (48, 16),
        (49, 9),
        (50, 14),
        (51, 7),
        (52, 7),
        (53, 8),
        (54, 8),
        (55, 18),
        (56, 9),
        (57, 20),
        (58, 10),
        (59, 12),
        (60, 6),
    ];
    for (index, used) in cases {
        assert_eq!(fx_param_count(*index), Some(*used), "index {index}");
    }
    assert_eq!(fx_param_count(15), None);
    assert_eq!(fx_param_count(99), None);
}

#[test]
fn padding_and_unknown_yield_none() {
    assert!(fx_param_def(0, 13).is_none()); // HALL uses 01–12
    assert!(fx_param_def(17, 8).is_none()); // DIMC uses 01–07
    assert!(fx_param_def(45, 6).is_none()); // LEC uses 01–05
    assert!(fx_param_def(60, 7).is_none()); // PIT uses 01–06
    assert!(fx_param_def(29, 33).is_none()); // GEQ uses 01–32
    assert!(fx_param_def(15, 1).is_none()); // undocumented gap
    assert!(fx_param_def(99, 1).is_none()); // unknown effect
    assert!(fx_param_def(0, 0).is_none());
    assert!(fx_param_def(0, 65).is_none());
}

#[test]
fn spot_labels_units_ranges() {
    let d = fx_param_def(0, 1).unwrap(); // HALL PreDelay
    assert_eq!(
        (d.label, d.unit, d.min, d.max),
        ("PreDelay", "ms", "0", "200")
    );
    assert!(!d.inferred && d.options.is_none() && d.bank.is_none());

    let d = fx_param_def(10, 3).unwrap(); // DLY Mode
    assert_eq!(d.label, "Mode");
    assert_eq!(d.options, Some(DLY_MODE));

    let d = fx_param_def(29, 1).unwrap(); // GEQ 20 Hz band
    assert_eq!((d.label, d.unit, d.min, d.max), ("20", "Hz", "-15", "+15"));
    let d = fx_param_def(29, 32).unwrap(); // GEQ master
    assert_eq!(d.label, "Lvl");
    let d = fx_param_def(28, 64).unwrap(); // GEQ2 B master
    assert_eq!((d.label, d.bank), ("Lvl", Some("B")));

    let d = fx_param_def(40, 11).unwrap(); // CMB ratio
    assert_eq!(d.options, Some(CMB_RATIO));
    assert_eq!(d.options.unwrap().len(), 14);

    let d = fx_param_def(6, 5).unwrap(); // VREV Output
    assert_eq!(d.options, Some(VINTAGE_OUT));

    let d = fx_param_def(33, 5).unwrap(); // DES Voice
    assert_eq!(d.options, Some(DES_VOICE));
}

#[test]
fn inferred_flags_match_notes() {
    assert!(!fx_param_def(0, 1).unwrap().inferred);
    assert!(!fx_param_def(49, 3).unwrap().inferred);
    assert!(!fx_param_def(56, 1).unwrap().inferred);
    assert!(fx_param_def(4, 16).unwrap().inferred); // CHAM ERLevelR
    assert!(fx_param_def(8, 4).unwrap().inferred); // GATE Density range
    assert!(fx_param_def(60, 3).unwrap().inferred); // PIT Delay range
    assert!(fx_param_def(42, 2).unwrap().inferred); // FAC InGain range
    assert!(!fx_param_def(42, 3).unwrap().inferred);
    assert!(fx_param_def(49, 2).unwrap().inferred); // ENH Spread name
    assert!(fx_param_def(49, 6).unwrap().inferred); // ENH MidQ name
    assert!(fx_param_def(56, 6).unwrap().inferred); // AMP Level order
    assert!(!fx_param_def(56, 9).unwrap().inferred); // AMP Cabinet ok
    assert!(fx_param_def(54, 3).unwrap().inferred); // SON Process scale
    assert!(!fx_param_def(54, 1).unwrap().inferred);
    // Contested 47 (grid ULC2 vs section ULC): every row inferred.
    for par in 1..=12 {
        assert!(fx_param_def(47, par).unwrap().inferred, "ULC2 par {par}");
    }
    // Wholly flagged effects: every row inferred.
    for par in 1..=12 {
        assert!(fx_param_def(59, par).unwrap().inferred, "PIT2 par {par}");
    }
    for par in 1..=11 {
        assert!(fx_param_def(34, par).unwrap().inferred, "P1A par {par}");
    }
    for par in 1..=16 {
        assert!(fx_param_def(48, par).unwrap().inferred, "ENH2 par {par}");
    }
    for par in 1..=14 {
        assert!(fx_param_def(43, par).unwrap().inferred, "FAC1M par {par}");
    }
    assert!(!fx_param_def(44, 1).unwrap().inferred); // FAC2 same layout, clean
}

#[test]
fn dual_banks_split_cleanly() {
    let a = fx_param_def(46, 2).unwrap(); // LEC2 Gain A
    assert_eq!((a.label, a.bank), ("Gain", Some("A")));
    let b = fx_param_def(46, 7).unwrap(); // LEC2 Gain B
    assert_eq!((b.label, b.bank, b.par), ("Gain", Some("B"), 7));
    assert!(fx_param_def(46, 11).is_none());

    let b = fx_param_def(21, 6).unwrap(); // SUB On B
    assert_eq!((b.label, b.bank), ("On", Some("B")));

    let b = fx_param_def(41, 30).unwrap(); // CMB2 On B
    assert_eq!((b.label, b.bank), ("On", Some("B")));
    let lock = fx_param_def(41, 57).unwrap(); // CMB2 Bd5 Lock B
    assert_eq!((lock.label, lock.bank), ("Bd5 Lock", Some("B")));
    assert_eq!(lock.options, Some(ZERO_ONE));
    assert!(fx_param_def(41, 59).is_none());

    let b = fx_param_def(55, 18).unwrap(); // AMP2 Cabinet B
    assert_eq!((b.label, b.bank), ("Cabinet", Some("B")));
    assert!(fx_param_def(55, 6).unwrap().inferred); // order flag survives banking
}
