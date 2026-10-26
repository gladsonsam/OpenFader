use super::*;
#[test]
fn address_shapes() {
    assert_eq!(fx_param(1, 0).unwrap(), "/fx/1/par/01");
    assert_eq!(fx_param(1, 9).unwrap(), "/fx/1/par/10");
    assert_eq!(fx_param(4, 63).unwrap(), "/fx/4/par/64");
    assert_eq!(fx_field(1, 0).unwrap(), "fx1_par01");
    assert_eq!(fx_field(4, 63).unwrap(), "fx4_par64");
    assert_eq!(fx_type(1).unwrap(), "/fx/1/type");
    assert_eq!(fx_type(4).unwrap(), "/fx/4/type");
    assert_eq!(fx_insert(1).unwrap(), "/fx/1/insert");
    assert_eq!(fx_insert(4).unwrap(), "/fx/4/insert");
    assert!(fx_insert(0).is_err());
    assert!(fx_insert(5).is_err());
    assert_eq!(fx_type_to_arg(0).unwrap(), OscType::Int(0));
    assert_eq!(fx_type_to_arg(60).unwrap(), OscType::Int(60));
    assert!(fx_type_to_arg(61).is_err());
}

#[test]
fn rejects_out_of_range() {
    assert!(fx_param(0, 0).is_err());
    assert!(fx_param(5, 0).is_err());
    assert!(fx_param(1, 64).is_err());
    assert!(fx_field(0, 0).is_err());
    assert!(fx_type(0).is_err());
    assert!(fx_type(5).is_err());
    assert!(check_value(f32::NAN).is_err());
    assert!(check_value(1.5).is_err());
    assert_eq!(check_value(0.5).unwrap(), 0.5);
}

#[test]
fn parse_type_shapes() {
    assert_eq!(parse_fx_type(&[OscType::Int(10)]).unwrap(), 10);
    assert_eq!(parse_fx_type(&[OscType::Int(0)]).unwrap(), 0);
    assert_eq!(parse_fx_type(&[OscType::Int(60)]).unwrap(), 60);
    assert_eq!(parse_fx_type(&[OscType::Float(10.0)]).unwrap(), 10);
    assert_eq!(parse_fx_type(&[OscType::Double(60.0)]).unwrap(), 60);
    assert!(parse_fx_type(&[]).is_err());
    assert!(parse_fx_type(&[OscType::Int(61)]).is_err());
    assert!(parse_fx_type(&[OscType::Int(-1)]).is_err());
    assert!(parse_fx_type(&[OscType::Float(10.5)]).is_err());
    assert!(parse_fx_type(&[OscType::Float(f32::NAN)]).is_err());
    assert!(parse_fx_type(&[OscType::String("HALL".into())]).is_err());
}

#[test]
fn int_pars_decode_per_def_and_nan_is_absent() {
    let freeze = fx_param_def(7, 13);
    assert_eq!(freeze.unwrap().options, Some(OFF_ON));
    assert_eq!(
        parse_fx_param(&[OscType::Int(0)], freeze.as_ref()).unwrap(),
        0.0
    );
    assert_eq!(
        parse_fx_param(&[OscType::Int(1)], freeze.as_ref()).unwrap(),
        1.0
    );
    assert_eq!(
        parse_fx_param(&[OscType::Long(1)], freeze.as_ref()).unwrap(),
        1.0
    );
    assert!(parse_fx_param(&[OscType::Int(2)], freeze.as_ref()).is_err());
    assert!(parse_fx_param(&[OscType::Int(-1)], freeze.as_ref()).is_err());

    let pattern = fx_param_def(26, 2);
    assert_eq!(pattern.unwrap().options, Some(PATTERN14));
    assert_eq!(
        parse_fx_param(&[OscType::Int(0)], pattern.as_ref()).unwrap(),
        0.0
    );
    assert_eq!(
        parse_fx_param(&[OscType::Int(13)], pattern.as_ref()).unwrap(),
        1.0
    );
    assert!(
        (parse_fx_param(&[OscType::Int(1)], pattern.as_ref()).unwrap() - 1.0 / 13.0).abs() < 1e-6
    );
    assert!(parse_fx_param(&[OscType::Int(14)], pattern.as_ref()).is_err());

    let depth = fx_param_def(26, 8);
    assert!(depth.unwrap().options.is_none());
    assert_eq!(
        parse_fx_param(&[OscType::Int(1)], depth.as_ref()).unwrap(),
        1.0
    );
    assert_eq!(
        parse_fx_param(&[OscType::Int(0)], depth.as_ref()).unwrap(),
        0.0
    );
    assert!(parse_fx_param(&[OscType::Int(2)], depth.as_ref()).is_err());

    let hall = fx_param_def(0, 1);
    assert_eq!(
        parse_fx_param(&[OscType::Float(0.5)], hall.as_ref()).unwrap(),
        0.5
    );
    assert_eq!(
        parse_fx_param(&[OscType::Double(0.25)], hall.as_ref()).unwrap(),
        0.25
    );
    assert_eq!(parse_fx_param(&[OscType::Float(0.25)], None).unwrap(), 0.25);
    assert_eq!(parse_fx_param(&[OscType::Int(1)], None).unwrap(), 1.0);
    assert!(parse_fx_param(&[OscType::Float(1.5)], hall.as_ref()).is_err());

    // NaN (float or double, known def or not) means absent: bail with
    // an "absent" error, never surface NaN as a value.
    for args in [
        vec![OscType::Float(f32::NAN)],
        vec![OscType::Double(f64::NAN)],
    ] {
        for def in [hall.as_ref(), None] {
            let err = parse_fx_param(&args, def).unwrap_err();
            assert!(err.to_string().contains("absent"), "unexpected: {err}");
        }
    }
    assert!(parse_fx_param(&[OscType::Float(f32::INFINITY)], hall.as_ref()).is_err());
    assert!(parse_fx_param(&[], hall.as_ref()).is_err());
    assert!(parse_fx_param(&[OscType::String("x".into())], hall.as_ref()).is_err());
}
