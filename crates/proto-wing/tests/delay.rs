//! Delay tests use intentionally unit-specific live ranges, including a sample
//! range unlike the static manual, so an implementation cannot pass by guessing.
use openfader_core::{MixerDriver, ParamValue, StripId};
use openfader_proto_wing::{delay, WingDriver};
use openfader_transport::osc;
use rosc::OscType as O;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{net::UdpSocket, task::JoinHandle};

#[derive(Clone)]
struct Value {
    unit: usize,
    value: f32,
    on: bool,
    readonly: bool,
    ignored: bool,
    flip: bool,
}
struct Desk {
    addr: String,
    state: Arc<Mutex<HashMap<String, Value>>>,
    writes: Arc<Mutex<Vec<(String, O)>>>,
    task: JoinHandle<()>,
}
impl Drop for Desk {
    fn drop(&mut self) {
        self.task.abort();
    }
}
const BASES: &[(u16, &str)] = &[
    (0, "/ch/1/in/set"),
    (40, "/aux/1/in/set"),
    (48, "/bus/1/dly"),
    (64, "/main/1/dly"),
    (68, "/mtx/1/dly"),
];
fn domain(unit: usize) -> (f32, f32, i32) {
    [
        (0.1, 150., 1500),
        (0.5, 500., 1000),
        (0.5, 500., 4996),
        (7., 77., 71),
    ][unit]
}
fn descriptor(value: &Value) -> Vec<u8> {
    let mut body = vec![0; 10]; // parent/hash/index (not used for address matching)
    body.extend_from_slice(&[3, b'd', b'l', b'y', 5, b'D', b'E', b'L', b'A', b'Y']);
    body.extend_from_slice(&(0x10u16 | if value.readonly { 0x200 } else { 0 }).to_be_bytes());
    let (min, max, steps) = domain(value.unit);
    body.extend_from_slice(&min.to_be_bytes());
    body.extend_from_slice(&max.to_be_bytes());
    body.extend_from_slice(&steps.to_be_bytes());
    let mut out = vec![0xdf];
    out.extend_from_slice(&(body.len() as u16).to_be_bytes());
    out.extend(body);
    out.push(0xde);
    out
}
impl Desk {
    async fn new() -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap().to_string();
        let state = Arc::new(Mutex::new(
            BASES
                .iter()
                .map(|(_, base)| {
                    (
                        base.to_string(),
                        Value {
                            unit: 0,
                            value: 20.,
                            on: false,
                            readonly: false,
                            ignored: false,
                            flip: false,
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
        ));
        let values = state.clone();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let log = writes.clone();
        let task = tokio::spawn(async move {
            let mut data = [0u8; 4096];
            loop {
                let (n, peer) = socket.recv_from(&mut data).await.unwrap();
                for msg in osc::flatten(osc::decode_packet(&data[..n]).unwrap()) {
                    let reply = if msg.addr == "/?" {
                        vec![O::String(
                            "WING,127.0.0.1,Delay fixture,wing-compact,TEST,3.0.5".into(),
                        )]
                    } else {
                        let is_def = msg.args == vec![O::Blob(vec![0xdd])];
                        let (base, leaf) = if is_def {
                            (msg.addr.as_str(), "")
                        } else {
                            msg.addr.rsplit_once('/').unwrap()
                        };
                        let mut map = values.lock().unwrap();
                        let value = map.get_mut(base).unwrap();
                        if is_def {
                            if value.flip {
                                value.unit = 2;
                                value.flip = false;
                            }
                            vec![O::Blob(descriptor(value))]
                        } else if !msg.args.is_empty() {
                            let arg = msg.args[0].clone();
                            log.lock().unwrap().push((msg.addr.clone(), arg.clone()));
                            if !value.ignored {
                                match (leaf, arg) {
                                    ("on" | "dlyon", O::Int(on)) => value.on = on != 0,
                                    ("mode" | "dlymode", O::String(unit)) => {
                                        value.unit =
                                            delay::UNITS.iter().position(|u| *u == unit).unwrap();
                                        let (min, max, _) = domain(value.unit);
                                        value.value = value.value.clamp(min, max);
                                    }
                                    ("dly", O::Float(amount)) => value.value = amount,
                                    other => panic!("unexpected delay write {other:?}"),
                                }
                            }
                            continue;
                        } else {
                            match leaf {
                                "on" | "dlyon" => vec![
                                    O::String("state".into()),
                                    O::Float(0.),
                                    O::Int(value.on as i32),
                                ],
                                "mode" | "dlymode" => {
                                    vec![O::String(delay::UNITS[value.unit].into())]
                                }
                                "dly" => vec![
                                    O::String("display".into()),
                                    O::Float(0.123),
                                    O::Float(value.value),
                                ],
                                _ => panic!("unexpected delay query {leaf}"),
                            }
                        }
                    };
                    let bytes = osc::encode_message(&msg.addr, reply).unwrap();
                    socket.send_to(&bytes, peer).await.unwrap();
                }
            }
        });
        Self {
            addr,
            state,
            writes,
            task,
        }
    }
}

#[tokio::test]
async fn every_audio_family_roundtrips_native_units_and_live_domains() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    for &(strip, base) in BASES {
        for unit in 0..4 {
            driver
                .set_param(StripId(strip), delay::UNIT, ParamValue::Float(unit as f32))
                .await
                .unwrap();
            driver
                .set_param(StripId(strip), "delay_on", ParamValue::Bool(true))
                .await
                .unwrap();
            let state = driver.delay_state(StripId(strip)).await.unwrap();
            let (min, max, steps) = domain(unit);
            assert_eq!((state.min, state.max, state.steps), (min, max, steps));
            assert_eq!(state.unit, unit);
            assert!(state.on && state.editable);
            driver
                .set_param(
                    StripId(strip),
                    delay::VALUE,
                    ParamValue::Text(format!("{}:30", delay::UNITS[unit])),
                )
                .await
                .unwrap();
            assert_eq!(
                driver
                    .get_param(StripId(strip), delay::VALUE)
                    .await
                    .unwrap(),
                ParamValue::Text(format!("{}:30", delay::UNITS[unit]))
            );
            let state = driver
                .get_param(StripId(strip), delay::STATE)
                .await
                .unwrap();
            let ParamValue::Text(json) = state else {
                panic!("state text expected")
            };
            assert!(json.contains("\"value\":30"));
            assert!(desk
                .writes
                .lock()
                .unwrap()
                .iter()
                .any(|(path, v)| path == &format!("{base}/dly") && *v == O::Float(30.)));
            // The SMP range is intentionally 7..77 from DD, not 16..500 from the manual.
            let before = desk.writes.lock().unwrap().len();
            assert!(driver
                .set_delay_value(
                    StripId(strip),
                    &format!("{}:{}", delay::UNITS[unit], max + 1.)
                )
                .await
                .is_err());
            assert_eq!(desk.writes.lock().unwrap().len(), before);
        }
    }
    assert!(driver.delay_state(StripId(76)).await.is_err());
    assert!(driver.set_delay_value(StripId(76), "M:20").await.is_err());
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn stale_units_readonly_definitions_and_bad_types_never_write_delay_values() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    for bad in ["20", "ms:20", "M:NaN", "M:inf", "M:1:2", "FT:20", "M:-1"] {
        assert!(driver.set_delay_value(strip, bad).await.is_err(), "{bad}");
    }
    assert!(driver
        .set_param(strip, delay::VALUE, ParamValue::Float(20.))
        .await
        .is_err());
    assert!(driver
        .set_param(strip, delay::UNIT, ParamValue::Float(1.5))
        .await
        .is_err());
    assert!(driver
        .set_param(strip, delay::STATE, ParamValue::Text("invented".into()))
        .await
        .is_err());
    desk.state
        .lock()
        .unwrap()
        .get_mut(BASES[0].1)
        .unwrap()
        .readonly = true;
    assert!(!driver.delay_state(strip).await.unwrap().editable);
    assert!(driver.set_delay_value(strip, "M:20").await.is_err());
    desk.state
        .lock()
        .unwrap()
        .get_mut(BASES[0].1)
        .unwrap()
        .readonly = false;
    desk.state.lock().unwrap().get_mut(BASES[0].1).unwrap().flip = true;
    assert!(driver.delay_state(strip).await.is_err());
    desk.state.lock().unwrap().get_mut(BASES[0].1).unwrap().unit = 0;
    desk.state.lock().unwrap().get_mut(BASES[0].1).unwrap().flip = true;
    assert!(driver.set_delay_value(strip, "M:20").await.is_err());
    assert!(desk.writes.lock().unwrap().is_empty());
    driver.disconnect().await.unwrap();
}

#[tokio::test]
async fn ignored_writes_fail_real_readback_and_unit_tag_survives_scene_style_replay() {
    let desk = Desk::new().await;
    let driver = WingDriver::new();
    driver.connect(&desk.addr).await.unwrap();
    let strip = StripId(0);
    let stored = driver.get_param(strip, delay::VALUE).await.unwrap();
    desk.state
        .lock()
        .unwrap()
        .get_mut(BASES[0].1)
        .unwrap()
        .ignored = true;
    assert!(driver.set_delay_value(strip, "M:50").await.is_err());
    assert!(driver
        .set_param(strip, delay::UNIT, ParamValue::Float(1.))
        .await
        .is_err());
    assert!(driver
        .set_param(strip, "delay_on", ParamValue::Bool(true))
        .await
        .is_err());
    assert_eq!(driver.delay_state(strip).await.unwrap().value, 20.);
    desk.state
        .lock()
        .unwrap()
        .get_mut(BASES[0].1)
        .unwrap()
        .ignored = false;
    driver
        .set_param(strip, delay::UNIT, ParamValue::Float(2.))
        .await
        .unwrap();
    let count = desk.writes.lock().unwrap().len();
    // Replaying a value into a different unit must fail instead of converting it silently.
    assert!(driver.set_param(strip, delay::VALUE, stored).await.is_err());
    assert_eq!(desk.writes.lock().unwrap().len(), count);
    driver.disconnect().await.unwrap();
}

#[test]
fn tagged_readback_compares_units_and_numbers_without_equating_text_or_nonfinite_values() {
    assert!(delay::tagged_agree("MS:12", "MS:12.0"));
    assert!(delay::tagged_agree("M:12.3", "M:12.300000190734863"));
    for (a, b) in [
        ("M:12", "MS:12"),
        ("M:12", "M:13"),
        ("M:NaN", "M:NaN"),
        ("M:inf", "M:inf"),
        ("x:1", "x:1"),
    ] {
        assert!(!delay::tagged_agree(a, b));
    }
}
