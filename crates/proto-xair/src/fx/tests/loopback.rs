use super::*;

/// Loopback: a fake desk serving `/fx/N/type` (int) + `/fx/N/par/MM`
/// (floats) proves the read path — query type, map to the effect, then
/// drive that effect's labeled pars bit-exact.
#[tokio::test]
async fn loopback_type_then_labeled_pars() {
    use openfader_transport::{osc, OscSession};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::UdpSocket;

    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = socket.local_addr().unwrap();
    let types: Arc<StdMutex<HashMap<u8, i32>>> =
        Arc::new(StdMutex::new(HashMap::from([(1u8, 10i32), (2u8, 60i32)])));
    let pars: Arc<StdMutex<HashMap<String, f32>>> = Arc::new(StdMutex::new(HashMap::new()));
    let worker = socket.clone();
    let wtypes = types.clone();
    let wpars = pars.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                break;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                continue;
            };
            for msg in osc::flatten(packet) {
                let parts: Vec<&str> = msg.addr.split('/').collect();
                if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                    let Ok(slot) = parts[2].parse::<u8>() else {
                        continue;
                    };
                    match msg.args.first() {
                        Some(OscType::Int(i)) => {
                            wtypes.lock().unwrap().insert(slot, *i);
                        }
                        Some(OscType::Float(f)) => {
                            wtypes.lock().unwrap().insert(slot, *f as i32);
                        }
                        _ => {
                            let t = wtypes.lock().unwrap().get(&slot).copied().unwrap_or(0);
                            let bytes =
                                osc::encode_message(&msg.addr, vec![OscType::Int(t)]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                    }
                    continue;
                }
                if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                    match msg.args.first() {
                        Some(OscType::Float(f)) => {
                            wpars.lock().unwrap().insert(msg.addr.clone(), *f);
                        }
                        _ => {
                            let v = wpars.lock().unwrap().get(&msg.addr).copied().unwrap_or(0.0);
                            let bytes =
                                osc::encode_message(&msg.addr, vec![OscType::Float(v)]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                    }
                }
            }
        }
    });

    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(addr).await.unwrap();
    let session = OscSession::from_connected_socket(sock, None);

    let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
    let loaded = parse_fx_type(&reply.args).unwrap();
    assert_eq!(loaded, 10);
    assert_eq!(fx_effect_name(loaded), Some("DLY"));
    let mode = fx_param_def(loaded, 3).unwrap();
    assert_eq!(mode.label, "Mode");
    assert_eq!(mode.options, Some(DLY_MODE));

    let reply = session.query(&fx_type(2).unwrap()).await.unwrap();
    let loaded = parse_fx_type(&reply.args).unwrap();
    assert_eq!(loaded, 60);
    assert_eq!(fx_param_count(loaded), Some(6));
    assert!(fx_param_def(loaded, 7).is_none());

    session
        .send(&fx_param(1, 0).unwrap(), vec![OscType::Float(0.5)])
        .await
        .unwrap();
    let back = session.query(&fx_param(1, 0).unwrap()).await.unwrap();
    assert_eq!(osc::fader_value(&back).unwrap(), 0.5);
    session
        .send(&fx_param(2, 2).unwrap(), vec![OscType::Float(0.25)])
        .await
        .unwrap();
    let back = session.query(&fx_param(2, 2).unwrap()).await.unwrap();
    assert_eq!(osc::fader_value(&back).unwrap(), 0.25);
    let back = session.query(&fx_param(1, 5).unwrap()).await.unwrap();
    assert_eq!(osc::fader_value(&back).unwrap(), 0.0);
}

/// Loopback: a fake serving a delay type + time par proves the tap path
/// — tap target par writes round-trip bit-exact via `set_fx_param`.
#[tokio::test]
async fn loopback_tap_target_par_roundtrip() {
    use openfader_transport::{osc, OscSession};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::UdpSocket;

    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = socket.local_addr().unwrap();
    let pars: Arc<StdMutex<HashMap<String, f32>>> = Arc::new(StdMutex::new(HashMap::new()));
    let worker = socket.clone();
    let wpars = pars.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                break;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                continue;
            };
            for msg in osc::flatten(packet) {
                let parts: Vec<&str> = msg.addr.split('/').collect();
                if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                    if msg.args.is_empty() {
                        let bytes = osc::encode_message(&msg.addr, vec![OscType::Int(10)]).unwrap();
                        let _ = worker.send_to(&bytes, peer).await;
                    }
                    continue;
                }
                if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                    match msg.args.first() {
                        Some(OscType::Float(f)) => {
                            wpars.lock().unwrap().insert(msg.addr.clone(), *f);
                        }
                        _ => {
                            let v = wpars.lock().unwrap().get(&msg.addr).copied().unwrap_or(0.0);
                            let bytes =
                                osc::encode_message(&msg.addr, vec![OscType::Float(v)]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                    }
                }
            }
        }
    });

    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(addr).await.unwrap();
    let session = OscSession::from_connected_socket(sock, None);

    let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
    let loaded = parse_fx_type(&reply.args).unwrap();
    assert_eq!(loaded, 10);
    let target = fx_tap_target(loaded).unwrap();
    assert_eq!(target.par, 2);
    let ms = tap_delay_ms(120.0, TapSubdiv::Quarter);
    assert!((ms - 500.0).abs() < 1e-9);
    let v01 = tap_ms_to_normalized(ms, target.min_ms, target.max_ms);
    let idx = target.par - 1; // wire par is 1-based; driver index 0-based
    session
        .send(&fx_param(1, idx).unwrap(), vec![OscType::Float(v01)])
        .await
        .unwrap();
    let back = session.query(&fx_param(1, idx).unwrap()).await.unwrap();
    assert_eq!(osc::fader_value(&back).unwrap(), v01);
    let vdot = tap_ms_to_normalized(
        tap_delay_ms(120.0, TapSubdiv::Dotted8th),
        target.min_ms,
        target.max_ms,
    );
    assert!(vdot < v01);
    session
        .send(&fx_param(1, idx).unwrap(), vec![OscType::Float(vdot)])
        .await
        .unwrap();
    let back = session.query(&fx_param(1, idx).unwrap()).await.unwrap();
    assert_eq!(osc::fader_value(&back).unwrap(), vdot);
}

/// Loopback: a fake serving the hardware-proven mix — INT enum/switch
/// pars (D/FL par 02/08/09 = 0/1/1, VRM par 13 = 0), float continuous
/// pars, and float-NaN padding past each effect's own count — proves
/// the read path: int decode per def, NaN rejection, and bit-exact
/// float roundtrips.
#[tokio::test]
async fn loopback_mixed_int_float_pars_with_nan_padding() {
    use openfader_transport::{osc, OscSession};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::UdpSocket;

    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = socket.local_addr().unwrap();
    // Verbatim wire storage so ints stay ints and NaN stays NaN.
    let pars: Arc<StdMutex<HashMap<String, OscType>>> = Arc::new(StdMutex::new(HashMap::from([
        ("/fx/3/par/01".to_string(), OscType::Float(0.5)),
        ("/fx/3/par/02".to_string(), OscType::Int(0)),
        ("/fx/3/par/08".to_string(), OscType::Int(1)),
        ("/fx/3/par/09".to_string(), OscType::Int(1)),
        ("/fx/3/par/13".to_string(), OscType::Float(0.75)),
        ("/fx/3/par/14".to_string(), OscType::Float(f32::NAN)),
        ("/fx/1/par/13".to_string(), OscType::Int(0)),
        ("/fx/1/par/14".to_string(), OscType::Float(f32::NAN)),
    ])));
    let types: Arc<StdMutex<HashMap<u8, i32>>> =
        Arc::new(StdMutex::new(HashMap::from([(1u8, 7i32), (3u8, 26i32)])));
    let worker = socket.clone();
    let wpars = pars.clone();
    let wtypes = types.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                break;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                continue;
            };
            for msg in osc::flatten(packet) {
                let parts: Vec<&str> = msg.addr.split('/').collect();
                if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                    if msg.args.is_empty() {
                        let t = wtypes
                            .lock()
                            .unwrap()
                            .get(&parts[2].parse::<u8>().unwrap_or(0))
                            .copied()
                            .unwrap_or(0);
                        let bytes = osc::encode_message(&msg.addr, vec![OscType::Int(t)]).unwrap();
                        let _ = worker.send_to(&bytes, peer).await;
                    }
                    continue;
                }
                if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                    match msg.args.first() {
                        Some(arg @ (OscType::Float(_) | OscType::Int(_))) => {
                            wpars.lock().unwrap().insert(msg.addr.clone(), arg.clone());
                        }
                        // Queries answer the stored wire value verbatim;
                        // untouched pars read back NaN, like live padding.
                        _ => {
                            let v = wpars
                                .lock()
                                .unwrap()
                                .get(&msg.addr)
                                .cloned()
                                .unwrap_or(OscType::Float(f32::NAN));
                            let bytes = osc::encode_message(&msg.addr, vec![v]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                    }
                }
            }
        }
    });

    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    sock.connect(addr).await.unwrap();
    let session = OscSession::from_connected_socket(sock, None);

    let reply = session.query(&fx_type(3).unwrap()).await.unwrap();
    let loaded = parse_fx_type(&reply.args).unwrap();
    assert_eq!(loaded, 26);
    let def = |par| fx_param_def(loaded, par);
    let back = session.query(&fx_param(3, 1).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, def(2).as_ref()).unwrap(), 0.0);
    let back = session.query(&fx_param(3, 7).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, def(8).as_ref()).unwrap(), 1.0);
    let back = session.query(&fx_param(3, 8).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, def(9).as_ref()).unwrap(), 1.0);
    let back = session.query(&fx_param(3, 0).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, def(1).as_ref()).unwrap(), 0.5);
    // Padding past the effect's own count replies NaN → absent, never a value.
    let back = session.query(&fx_param(3, 13).unwrap()).await.unwrap();
    let err = parse_fx_param(&back.args, def(14).as_ref()).unwrap_err();
    assert!(err.to_string().contains("absent"), "unexpected: {err}");

    // Slot 1 loads VRM (7): Freeze int 0 → 0.0, par 14 NaN → absent.
    let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
    let loaded = parse_fx_type(&reply.args).unwrap();
    assert_eq!(loaded, 7);
    let back = session.query(&fx_param(1, 12).unwrap()).await.unwrap();
    let freeze = fx_param_def(loaded, 13);
    assert_eq!(parse_fx_param(&back.args, freeze.as_ref()).unwrap(), 0.0);
    let back = session.query(&fx_param(1, 13).unwrap()).await.unwrap();
    let err = parse_fx_param(&back.args, fx_param_def(loaded, 14).as_ref()).unwrap_err();
    assert!(err.to_string().contains("absent"), "unexpected: {err}");

    session
        .send(&fx_param(3, 0).unwrap(), vec![OscType::Float(0.5)])
        .await
        .unwrap();
    let back = session.query(&fx_param(3, 0).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, def(1).as_ref()).unwrap(), 0.5);
    session
        .send(&fx_param(1, 12).unwrap(), vec![OscType::Int(1)])
        .await
        .unwrap();
    let back = session.query(&fx_param(1, 12).unwrap()).await.unwrap();
    assert_eq!(parse_fx_param(&back.args, freeze.as_ref()).unwrap(), 1.0);
}
