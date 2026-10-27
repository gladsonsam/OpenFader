use super::*;

#[test]
fn capabilities_match_loopback_tested_set() {
    use openfader_core::params;
    // Every advertised id exists in the canonical catalogue…
    for id in XAIR_CAPABILITIES {
        assert!(
            params::find(id).is_some(),
            "xair capability {id} not in the catalogue"
        );
    }
    // …and the unsupported remainder is exactly the honest-no-address
    // set: delay + hpf_slope (X32-only), source (insrc live-silent),
    // icon (no firmware leaf), link (global chlink, no per-channel
    // leaf), aux7/8 (only 6 bus sends), send11..16 (only 6+4 send
    // slots), pan outside bus-pair slots 1/3/5, all twelve legacy
    // aux/fx send mutes plus the ten v2
    // sendN_on (no send on/off leaf, T7), per-channel eq5 stays out
    // (channels carry 4 bands; eq5/6 ride buses + main and ARE in caps),
    // plus the non-strip v2b ids: solo dim/mute/level/source (global
    // `/config/solo/*`, driver methods) and fx_insert (slot-addressed
    // `/fx/N/insert`, driver method).
    let mut unsupported: Vec<&str> = params::CATALOGUE
        .iter()
        .map(|d| d.id)
        .filter(|id| !XAIR_CAPABILITIES.contains(id))
        .collect();
    unsupported.sort_unstable();
    let mut expected = vec![
        "aux1_mute",
        "aux2_mute",
        "aux3_mute",
        "aux4_mute",
        "aux5_mute",
        "aux6_mute",
        "aux7_level",
        "aux7_mute",
        "aux8_level",
        "aux8_mute",
        "delay_ms",
        "delay_on",
        "fx1_mute",
        "fx2_mute",
        "fx3_mute",
        "fx4_mute",
        "fx_insert",
        "hpf_slope",
        "icon",
        "link",
        "send10_on",
        "send10_pan",
        "send11_level",
        "send11_on",
        "send11_pan",
        "send11_tap",
        "send12_level",
        "send12_on",
        "send12_pan",
        "send12_tap",
        "send13_level",
        "send13_on",
        "send13_pan",
        "send13_tap",
        "send14_level",
        "send14_on",
        "send14_pan",
        "send14_tap",
        "send15_level",
        "send15_on",
        "send15_pan",
        "send15_tap",
        "send16_level",
        "send16_on",
        "send16_pan",
        "send16_tap",
        "send1_on",
        "send2_on",
        "send2_pan",
        "send3_on",
        "send4_on",
        "send4_pan",
        "send5_on",
        "send6_on",
        "send6_pan",
        "send7_on",
        "send7_pan",
        "send8_on",
        "send8_pan",
        "send9_on",
        "send9_pan",
        "solo_dim",
        "solo_level",
        "solo_mute",
        "solo_source",
        "source",
    ];
    expected.sort_unstable();
    assert_eq!(unsupported, expected);
    assert_eq!(XAirDriver::new().capabilities(), XAIR_CAPABILITIES);
}

/// XR18 strip map shape: ordered, contiguous, 36 strips; roots resolve
/// per family; out-of-range stays a local error.
#[test]
fn topology_matches_strip_roots() {
    assert_eq!(XAIR_STRIPS, 36);
    let mut next = 0usize;
    for fam in XAIR_TOPOLOGY {
        assert_eq!(fam.first, next, "gap/overlap before {}", fam.key);
        next = fam.end();
    }
    assert_eq!(next, XAIR_STRIPS);
    let (k, r) = strip_root(0).unwrap();
    assert_eq!((k, r.as_str()), (openfader_core::StripKind::In, "/ch/01"));
    let (k, r) = strip_root(15).unwrap();
    assert_eq!((k, r.as_str()), (openfader_core::StripKind::In, "/ch/16"));
    assert_eq!(strip_root(16).unwrap().1, "/rtn/1");
    assert_eq!(strip_root(19).unwrap().1, "/rtn/4");
    assert_eq!(strip_root(20).unwrap().1, "/bus/1");
    assert_eq!(strip_root(25).unwrap().1, "/bus/6");
    assert_eq!(strip_root(26).unwrap().1, "/fxsend/1");
    assert_eq!(strip_root(30).unwrap().1, "/lr");
    assert_eq!(strip_root(31).unwrap().1, "/dca/1");
    assert_eq!(strip_root(34).unwrap().1, "/dca/4");
    let (k, r) = strip_root(35).unwrap();
    assert_eq!(
        (k, r.as_str()),
        (openfader_core::StripKind::Ret, "/rtn/aux")
    );
    assert!(strip_root(36).is_err());
    assert!(strip_root(99).is_err());
    assert_eq!(strip_of_root("/rtn/aux"), Some(35));
    assert_eq!(strip_of_root("/rtn/1"), Some(16));
}
