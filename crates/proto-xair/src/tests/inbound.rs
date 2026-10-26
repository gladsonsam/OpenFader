use super::*;
use crate::inbound::{DCA_ASSIGN_IDS, LR_STRIP};

/// like their `get_param` arms (same codecs, engineering units), and
/// out-of-scope shapes stay silent. Pure unit test (no sockets): the
/// messages below are the wire shapes the desk (and the strict fake)
/// emits.
#[test]
fn inbound_covers_send_level_gain_color() {
    let msg = |addr: &str, args: Vec<OscType>| OscMessage {
        addr: addr.to_string(),
        args,
    };
    let (strip, param, value) =
        parse_inbound(&msg("/ch/01/mix/01/level", vec![OscType::Float(0.5)]))
            .into_iter()
            .next()
            .expect("send level parses");
    assert_eq!((strip, param), (0, "aux1_level"));
    assert!(
        matches!(value, ParamValue::Float(v) if (v - mix::wire_to_send_db(0.5).unwrap()).abs() < 1e-6)
    );
    let (strip, param, _) = parse_inbound(&msg("/ch/02/mix/07/level", vec![OscType::Float(0.25)]))
        .into_iter()
        .next()
        .expect("fx send level parses");
    assert_eq!((strip, param), (1, "fx1_level"));
    let (strip, param, value) = parse_inbound(&msg("/headamp/01/gain", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("headamp gain parses");
    assert_eq!((strip, param), (0, "gain"));
    assert!(matches!(value, ParamValue::Float(v) if (v - 24.0).abs() < 1e-5));
    let (strip, param, value) = parse_inbound(&msg("/ch/01/config/color", vec![OscType::Int(3)]))
        .into_iter()
        .next()
        .expect("strip color parses");
    assert_eq!((strip, param), (0, "color"));
    assert!(matches!(value, ParamValue::Float(v) if v == 3.0));
    let (strip, param, _) = parse_inbound(&msg("/lr/config/color", vec![OscType::Int(7)]))
        .into_iter()
        .next()
        .expect("lr color parses");
    assert_eq!((strip, param), (LR_STRIP, "color"));
    // Out-of-scope shapes stay silent: bus roots carry no send slots,
    // headamp ends at ch16, slots end at 10, icon has no arm.
    assert!(parse_inbound(&msg("/bus/1/mix/01/level", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/headamp/17/gain", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/ch/01/mix/11/level", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/ch/01/config/icon", vec![OscType::Int(1)])).is_empty());
}

/// Desk-originated solo updates ride `/-stat/solosw/NN` (the same
/// 1-based bank `set_solo`/`get_solo` drive): soloed ch1 decodes to
/// strip 0 `solo` true, cleared ch16 to strip 15 false. Out-of-range
/// leaves and the bare bank-query shape stay silent (XR18: channels
/// 1..=16 only); the clear-solo action leaf is not a state frame.
#[test]
fn inbound_covers_solosw_bank() {
    let msg = |addr: &str, args: Vec<OscType>| OscMessage {
        addr: addr.to_string(),
        args,
    };
    let (strip, param, value) = parse_inbound(&msg("/-stat/solosw/01", vec![OscType::Int(1)]))
        .into_iter()
        .next()
        .expect("solosw on parses");
    assert_eq!((strip, param), (0, "solo"));
    assert!(matches!(value, ParamValue::Bool(true)));
    let (strip, param, value) = parse_inbound(&msg("/-stat/solosw/16", vec![OscType::Int(0)]))
        .into_iter()
        .next()
        .expect("solosw off parses");
    assert_eq!((strip, param), (15, "solo"));
    assert!(matches!(value, ParamValue::Bool(false)));
    assert!(parse_inbound(&msg("/-stat/solosw/17", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/-stat/solosw/00", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/-stat/solosw", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/-action/clearsolo", vec![OscType::Int(1)])).is_empty());
}

/// Desk-originated assign-plane updates (`{root}/mix/lr`,
/// `{root}/grp/dca`, `{root}/automix`): decoded like the matching
/// `get_param` arms. `grp/dca` fans out to one update per DCA id, the
/// automix container to enable + weight; `grp/mute` stays silent (no
/// catalogue ids — membership rides `MuteGroupAssign` events).
#[test]
fn inbound_covers_assign_plane() {
    let msg = |addr: &str, args: Vec<OscType>| OscMessage {
        addr: addr.to_string(),
        args,
    };
    let updates = parse_inbound(&msg("/ch/01/mix/lr", vec![OscType::Int(0)]));
    assert_eq!(updates.len(), 1);
    assert_eq!((updates[0].0, updates[0].1), (0, "lr_assign"));
    assert!(matches!(updates[0].2, ParamValue::Bool(false)));
    let updates = parse_inbound(&msg("/ch/01/grp/dca", vec![OscType::Int(5)]));
    assert_eq!(updates.len(), 4);
    for (i, (strip, id, value)) in updates.iter().enumerate() {
        assert_eq!((*strip, *id), (0, DCA_ASSIGN_IDS[i]));
        assert!(matches!(value, ParamValue::Bool(b) if *b == (i == 0 || i == 2)));
    }
    let updates = parse_inbound(&msg(
        "/ch/01/automix",
        vec![OscType::Int(1), OscType::Float(0.75)],
    ));
    assert_eq!(updates.len(), 2);
    assert_eq!((updates[0].0, updates[0].1), (0, "automix_on"));
    assert!(matches!(updates[0].2, ParamValue::Bool(true)));
    assert_eq!((updates[1].0, updates[1].1), (0, "automix_weight"));
    assert!(matches!(updates[1].2, ParamValue::Float(w) if (w - 0.75).abs() < 1e-6));
    // Mute-group bitmask: no catalogue ids, stays silent here.
    assert_eq!(
        parse_inbound(&msg("/ch/01/grp/mute", vec![OscType::Int(3)])).len(),
        4
    );
    // Silent roots stay silent (no `grp`/`lr` leaf on the desk).
    assert!(parse_inbound(&msg("/lr/grp/dca", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/dca/1/grp/dca", vec![OscType::Int(1)])).is_empty());
}

/// Desk-originated EQ updates (`{root}/eq/on`, `{root}/eq/{band}/{g|f|q/type}`):
/// decoded like the `get_param` eq arms (same codecs, engineering
/// units). Channels/FX returns cap at 4 bands, buses/main LR at 6
/// (mirroring `check_eq_band`); FX-send/DCA roots and out-of-range
/// bands stay silent. Pure unit test (no sockets): the messages below
/// are the wire shapes the desk broadcasts to `/xremote` subscribers.
#[test]
fn inbound_covers_eq_band_leaves() {
    let msg = |addr: &str, args: Vec<OscType>| OscMessage {
        addr: addr.to_string(),
        args,
    };
    let (strip, param, value) = parse_inbound(&msg("/ch/01/eq/1/g", vec![OscType::Float(0.75)]))
        .into_iter()
        .next()
        .expect("eq gain parses");
    assert_eq!((strip, param), (0, "eq1_gain"));
    assert!(
        matches!(value, ParamValue::Float(v) if (v - eq::wire_to_gain_db(0.75).unwrap()).abs() < 1e-6)
    );
    let (strip, param, value) = parse_inbound(&msg("/ch/02/eq/2/f", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("eq freq parses");
    assert_eq!((strip, param), (1, "eq2_freq"));
    assert!(
        matches!(value, ParamValue::Float(v) if (v - eq::wire_to_freq_hz(0.5).unwrap()).abs() < 1e-6)
    );
    let (strip, param, value) = parse_inbound(&msg("/ch/03/eq/3/q", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("eq q parses");
    assert_eq!((strip, param), (2, "eq3_q"));
    assert!(
        matches!(value, ParamValue::Float(v) if (v - eq::wire_to_q(0.5).unwrap()).abs() < 1e-6)
    );
    let (strip, param, value) = parse_inbound(&msg("/ch/04/eq/4/type", vec![OscType::Int(2)]))
        .into_iter()
        .next()
        .expect("eq type parses");
    assert_eq!((strip, param), (3, "eq4_type"));
    assert!(matches!(value, ParamValue::Float(v) if v == 2.0));
    let (strip, param, _) = parse_inbound(&msg("/rtn/1/eq/1/g", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("fxret eq gain parses");
    assert_eq!((strip, param), (16, "eq1_gain"));
    let (strip, param, _) = parse_inbound(&msg("/bus/1/eq/5/g", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("bus eq5 gain parses");
    assert_eq!((strip, param), (20, "eq5_gain"));
    let (strip, param, _) = parse_inbound(&msg("/bus/1/eq/6/type", vec![OscType::Int(5)]))
        .into_iter()
        .next()
        .expect("bus eq6 type parses");
    assert_eq!((strip, param), (20, "eq6_type"));
    let (strip, param, _) = parse_inbound(&msg("/lr/eq/2/g", vec![OscType::Float(0.25)]))
        .into_iter()
        .next()
        .expect("lr eq gain parses");
    assert_eq!((strip, param), (LR_STRIP, "eq2_gain"));
    let (strip, param, value) = parse_inbound(&msg("/ch/01/eq/on", vec![OscType::Int(1)]))
        .into_iter()
        .next()
        .expect("eq on parses");
    assert_eq!((strip, param), (0, "eq_on"));
    assert!(matches!(value, ParamValue::Bool(true)));
    let (strip, param, _) = parse_inbound(&msg("/lr/eq/on", vec![OscType::Int(0)]))
        .into_iter()
        .next()
        .expect("lr eq on parses");
    assert_eq!((strip, param), (LR_STRIP, "eq_on"));
    // Out-of-scope shapes stay silent: band 5+ on a 4-band channel,
    // band 0/7+, FX-send/DCA roots (no EQ there), non-EQ `/type`
    // leaves (FX type select), and garbage roots.
    assert!(parse_inbound(&msg("/ch/01/eq/5/g", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/ch/01/eq/0/g", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/ch/01/eq/7/g", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/fxsend/1/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/dca/1/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/dca/1/eq/on", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/fx/1/type", vec![OscType::Int(3)])).is_empty());
    assert!(!parse_inbound(&msg("/ch/01/gate/filter/type", vec![OscType::Int(1)])).is_empty());
    assert!(parse_inbound(&msg("/ch/99/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
}

#[test]
fn inbound_covers_rtn_aux() {
    let msg = |addr: &str, args: Vec<OscType>| OscMessage {
        addr: addr.to_string(),
        args,
    };
    let (strip, param, _) = parse_inbound(&msg("/rtn/aux/mix/fader", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("aux fader parses");
    assert_eq!((strip, param), (35, "fader"));
    let (strip, param, value) = parse_inbound(&msg("/rtn/aux/mix/on", vec![OscType::Int(0)]))
        .into_iter()
        .next()
        .expect("aux mute parses");
    assert_eq!((strip, param), (35, "mute"));
    assert!(matches!(value, ParamValue::Bool(true)));
    let (strip, param, _) = parse_inbound(&msg(
        "/rtn/aux/config/name",
        vec![OscType::String("COMP LR".to_string())],
    ))
    .into_iter()
    .next()
    .expect("aux name parses");
    assert_eq!((strip, param), (35, "name"));
    let (strip, param, _) = parse_inbound(&msg("/rtn/aux/eq/1/g", vec![OscType::Float(0.5)]))
        .into_iter()
        .next()
        .expect("aux eq gain parses");
    assert_eq!((strip, param), (35, "eq1_gain"));
    // No phantom `/aux/*` or `/auxin/*` roots: only `/rtn/aux` decodes.
    assert!(parse_inbound(&msg("/aux/mix/fader", vec![OscType::Float(0.5)])).is_empty());
    assert!(parse_inbound(&msg("/auxin/mix/fader", vec![OscType::Float(0.5)])).is_empty());
}
