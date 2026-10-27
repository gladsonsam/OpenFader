use openfader_proto_wing::native::{definitions, Kind};

fn string(out: &mut Vec<u8>, s: &str) {
    out.push(s.len() as u8);
    out.extend_from_slice(s.as_bytes());
}
fn record(name: &str, flags: u16, domain: &[u8], long_len: bool) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(-1i32).to_be_bytes());
    body.extend_from_slice(&0x12df3456i32.to_be_bytes());
    body.extend_from_slice(&3u16.to_be_bytes());
    string(&mut body, name);
    string(&mut body, "Delay Δ");
    body.extend_from_slice(&flags.to_be_bytes());
    body.extend_from_slice(domain);
    let mut out = vec![0xdf];
    if long_len {
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    } else {
        out.extend_from_slice(&(body.len() as u16).to_be_bytes());
    }
    out.extend(body);
    out
}
fn floats(min: f32, max: f32, steps: i32) -> Vec<u8> {
    [min.to_be_bytes(), max.to_be_bytes(), steps.to_be_bytes()].concat()
}

#[test]
fn definitions_follow_manual_flags_lengths_and_all_native_domains() {
    let mut choices = 2u16.to_be_bytes().to_vec();
    string(&mut choices, "M");
    string(&mut choices, "Meters");
    string(&mut choices, "MS");
    string(&mut choices, "Milliseconds");
    let mut fchoices = 2u16.to_be_bytes().to_vec();
    fchoices.extend_from_slice(&1.1f32.to_be_bytes());
    string(&mut fchoices, "Gentle");
    fchoices.extend_from_slice(&100f32.to_be_bytes());
    string(&mut fchoices, "Limit");
    let mut blob = Vec::new();
    blob.extend(record("node", 0, &[], false));
    blob.extend(record("dly", 0x15, &floats(0.1, 150., 1500), true));
    blob.extend(record("freq", 0x24, &floats(20., 20000., 901), false));
    blob.extend(record("fdr", 0x31, &[], false));
    blob.extend(record("on", 0x240, &[0, 0, 0, 0, 0, 0, 0, 1], false));
    blob.extend(record("mode", 0x50, &choices, false));
    blob.extend(record("ratio", 0x60, &fchoices, false));
    blob.extend(record("tags", 0x70, &[0, 80], false));
    blob.push(0xde);
    let defs = definitions(&blob).unwrap();
    assert_eq!(defs.len(), 8);
    assert_eq!(defs[1].parent, -1);
    assert_eq!(defs[1].hash, 0x12df3456);
    assert_eq!(defs[1].index, 3);
    assert_eq!(defs[1].label, "Delay Δ");
    assert_eq!(defs[1].unit, 5);
    assert_eq!(defs[0].kind, Kind::Node);
    assert_eq!(
        defs[1].kind,
        Kind::Linear {
            min: 0.1,
            max: 150.,
            steps: 1500
        }
    );
    assert_eq!(
        defs[2].kind,
        Kind::Log {
            min: 20.,
            max: 20000.,
            steps: 901
        }
    );
    assert_eq!(defs[3].kind, Kind::Fader);
    assert_eq!(defs[4].kind, Kind::Integer { min: 0, max: 1 });
    assert!(defs[4].read_only);
    assert!(!defs[1].read_only);
    assert_eq!(
        defs[5].kind,
        Kind::Enum(vec![
            ("M".into(), "Meters".into()),
            ("MS".into(), "Milliseconds".into())
        ])
    );
    assert_eq!(
        defs[6].kind,
        Kind::FloatEnum(vec![(1.1, "Gentle".into()), (100., "Limit".into())])
    );
    assert_eq!(defs[7].kind, Kind::Text { max_bytes: 80 });
    // Every proper truncation must be an error, not a panic or a partial tree.
    for end in 0..blob.len() {
        assert!(definitions(&blob[..end]).is_err(), "{end}");
    }
}

#[test]
fn malformed_definition_frames_never_become_editable_metadata() {
    let mut valid = record("dly", 0x15, &floats(0.1, 150., 1500), false);
    valid.push(0xde);
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(definitions(&trailing).is_err());
    let mut short = valid.clone();
    short[1..3].copy_from_slice(&1u16.to_be_bytes());
    assert!(definitions(&short).is_err());
    assert!(definitions(&[0xdf, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xde]).is_err());
    for flags in [0x80, 0xf0] {
        let mut unknown = record("future", flags, &[], false);
        unknown.push(0xde);
        assert!(definitions(&unknown).is_err());
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut bad = record("dly", 0x15, &floats(value, 150., 1500), false);
        bad.push(0xde);
        assert!(definitions(&bad).is_err());
    }
    let mut bad_enum = record("mode", 0x50, &[0xff, 0xff], false);
    bad_enum.push(0xde);
    assert!(definitions(&bad_enum).is_err());
    let mut extra_field = record("node", 0, &[123], false);
    extra_field.push(0xde);
    assert!(definitions(&extra_field).is_err());
    let mut invalid_text = record("name", 0, &[], false);
    invalid_text[14] = 0xff;
    invalid_text.push(0xde);
    assert!(definitions(&invalid_text).is_err());
}

#[test]
fn definitions_preserve_constants_and_descending_ranges_without_inventing_controls() {
    let mut blob = record("descending", 0x10, &floats(40., 0., 41), false);
    blob.extend(record("constant", 0x10, &floats(1., 1., 1), false));
    blob.push(0xde);
    let defs = definitions(&blob).unwrap();
    assert_eq!(
        defs[0].kind,
        Kind::Linear {
            min: 40.,
            max: 0.,
            steps: 41
        }
    );
    assert_eq!(
        defs[1].kind,
        Kind::Linear {
            min: 1.,
            max: 1.,
            steps: 1
        }
    );
}

#[tokio::test]
async fn live_definition_query_selects_exact_child_and_rejects_ambiguous_metadata() {
    use openfader_core::MixerDriver;
    use openfader_proto_wing::WingDriver;
    use openfader_transport::osc;
    use rosc::OscType as O;
    use std::sync::{Arc, Mutex};
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap().to_string();
    let mut original = record("on", 0x40, &[0, 0, 0, 0, 0, 0, 0, 1], false);
    original.extend(record("dly", 0x15, &floats(0.1, 150., 1500), false));
    original.push(0xde);
    let reply_data = Arc::new(Mutex::new(original));
    let responses = reply_data.clone();
    let task = tokio::spawn(async move {
        let mut data = [0u8; 4096];
        loop {
            let (n, peer) = socket.recv_from(&mut data).await.unwrap();
            let messages = osc::flatten(osc::decode_packet(&data[..n]).unwrap());
            for msg in messages {
                let args = if msg.addr == "/?" {
                    assert!(msg.args.is_empty());
                    vec![O::String(
                        "WING,127.0.0.1,Definitions,wing-compact,TEST,3.0.5".into(),
                    )]
                } else {
                    assert_eq!(msg.addr, "/bus/1/dly");
                    assert_eq!(msg.args, vec![O::Blob(vec![0xdd])]);
                    // Ordinary node listings at the same address are not DD replies.
                    let ordinary = osc::encode_message(
                        &msg.addr,
                        vec![O::String("on".into()), O::String("mode".into())],
                    )
                    .unwrap();
                    socket.send_to(&ordinary, peer).await.unwrap();
                    vec![O::Blob(responses.lock().unwrap().clone())]
                };
                let reply = osc::encode_message(&msg.addr, args).unwrap();
                socket.send_to(&reply, peer).await.unwrap();
            }
        }
    });
    let driver = WingDriver::new();
    driver.connect(&addr).await.unwrap();
    let definition = driver.parameter_definition("/bus/1/dly/dly").await.unwrap();
    assert_eq!(definition.name, "dly");
    assert_eq!(
        definition.kind,
        Kind::Linear {
            min: 0.1,
            max: 150.,
            steps: 1500
        }
    );
    assert!(!definition.read_only);
    assert!(driver
        .parameter_definition("/bus/1/dly/missing")
        .await
        .is_err());
    let mut duplicate = record("dly", 0x15, &floats(0.1, 150., 1500), false);
    duplicate.extend(duplicate.clone());
    duplicate.push(0xde);
    *reply_data.lock().unwrap() = duplicate;
    assert!(driver
        .parameter_definition("/bus/1/dly/dly")
        .await
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    *reply_data.lock().unwrap() = vec![0xdf, 0, 12, 0xde];
    assert!(driver.parameter_definition("/bus/1/dly/dly").await.is_err());
    for path in ["bad", "/", "/dly", "bus/1/dly/dly"] {
        assert!(driver.parameter_definition(path).await.is_err());
    }
    driver.disconnect().await.unwrap();
    task.abort();
}

#[test]
fn decodes_unmodified_daw_definition_records_from_the_protocol_manual() {
    // V3.0.5-1 pp.25–26: the DAW ENABLE and CONNECTION records returned
    // by /$ctl/daw with a DD blob. This anchors the fixture builder to a
    // published wire example rather than only testing its own assumptions.
    let hex = concat!(
        "df0022df17c2423cb129d50000026f6e0a44415720454e41424c4500400000000000000001",
        "df0028df17c2424e5c7f34000004636f6e6e0a434f4e4e454354494f4e005000020344494e000355534200",
        "de"
    );
    let bytes: Vec<_> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let defs = definitions(&bytes).unwrap();
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0].parent, 0xdf17c242u32 as i32);
    assert_eq!(defs[0].hash, 0x3cb129d5);
    assert_eq!(defs[0].name, "on");
    assert_eq!(defs[0].label, "DAW ENABLE");
    assert_eq!(defs[0].kind, Kind::Integer { min: 0, max: 1 });
    assert_eq!(defs[1].name, "conn");
    assert_eq!(defs[1].label, "CONNECTION");
    assert_eq!(
        defs[1].kind,
        Kind::Enum(vec![
            ("DIN".into(), String::new()),
            ("USB".into(), String::new())
        ])
    );
}
