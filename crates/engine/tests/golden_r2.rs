//! R2 golden frames: canonical `Cmd`/`Event` JSON fixtures for every R2
//! addition (devices/link/info/scenes/fx/routing/offline_diff/taps) plus
//! the binary taps layout.
//!
//! Lives in `tests/golden_r2/` — NOT `tests/golden/`: that directory is
//! consumed 1:1 by `apps/web/src/lib/golden.test.ts`, which rejects
//! unknown frame types (the TS parser itself ignores them, but its test
//! enumerates known shapes). Same precedent as `tests/golden_bin/`.
//! Regenerate with `R2GOLDEN_UPDATE=1`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use openmix_engine::protocol::{
    tap_id, taps_level_to_u16, u16_to_taps_level, BIN_HEADER_LEN, BIN_TYPE_TAPS,
    BIN_VERSION, TAPS_GR_DB_FULLSCALE,
};
use openmix_engine::{Cmd, Event};
use serde_json::{json, Value};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden_r2")
}

fn examples() -> Vec<(String, Value)> {
    let frames: Vec<(&str, Event)> = vec![
        (
            "devices",
            Event::Devices {
                list: vec![
                    openmix_engine::protocol::DeviceDesc {
                        id: "foh".to_string(),
                        name: "FOH Console".to_string(),
                        model: "SimConsole 48".to_string(),
                        ip: "192.168.1.20".to_string(),
                        kind: "console".to_string(),
                        state: "connected".to_string(),
                        fw: Some("0.1.0".to_string()),
                        via: None,
                    },
                    openmix_engine::protocol::DeviceDesc {
                        id: "sb1".to_string(),
                        name: "Stage Box A".to_string(),
                        model: "SimIO 32".to_string(),
                        ip: "192.168.1.40".to_string(),
                        kind: "io".to_string(),
                        state: "in_use".to_string(),
                        fw: None,
                        via: None,
                    },
                ],
                scanned_at: 1700000000,
            },
        ),
        (
            "link",
            Event::Link {
                state: "connected".to_string(),
                name: "FOH Console".to_string(),
                ip: "192.168.1.20".to_string(),
                fs: 48000,
                latency_ms: 1.2,
            },
        ),
        (
            "info",
            Event::Info {
                model: "SimDriver 32+8".to_string(),
                firmware: "0.1.0".to_string(),
                sample_rate: 48000,
                console_name: "sim".to_string(),
                driver: "sim".to_string(),
            },
        ),
        (
            "scenes",
            Event::Scenes {
                scenes: vec![
                    openmix_engine::protocol::SceneEntry {
                        number: 1,
                        name: "Soundcheck".to_string(),
                        current: false,
                        saved_at: 1700000000,
                    },
                    openmix_engine::protocol::SceneEntry {
                        number: 5,
                        name: "Set 1".to_string(),
                        current: true,
                        saved_at: 1700000100,
                    },
                ],
            },
        ),
        (
            "fx",
            Event::Fx {
                slots: vec![openmix_engine::protocol::FxSlotState {
                    slot: 1,
                    fx_type: "Hall".to_string(),
                    on: true,
                    bypass: false,
                    ret_label: "Ret 1/2".to_string(),
                    level: 0.5,
                    params: [("decay".to_string(), 0.42), ("mix".to_string(), 1.0)]
                        .into_iter()
                        .collect::<BTreeMap<_, _>>(),
                }],
            },
        ),
        (
            "routing",
            Event::Routing {
                patches: vec![openmix_engine::protocol::PatchEntry {
                    strip: 0,
                    key: None,
                    group: "Net".to_string(),
                    index: 5,
                    label: "Net 05".to_string(),
                }],
            },
        ),
        (
            "offline_diff",
            Event::OfflineDiff {
                changes: vec![openmix_engine::protocol::OfflineChange {
                    strip: 0,
                    key: None,
                    id: "gain".to_string(),
                    from: json!(20.0),
                    to: json!(25.0),
                }],
                count: 1,
            },
        ),
        (
            "taps",
            Event::Taps {
                tap: "in".to_string(),
                levels: vec![0.5, 0.25, 0.0, 0.75],
            },
        ),
        (
            "hello_taps",
            Event::Hello {
                binary_meters: false,
                taps: true,
            },
        ),
    ];
    let mut out: Vec<(String, Value)> = frames
        .into_iter()
        .map(|(name, e)| (name.to_string(), serde_json::to_value(&e).unwrap()))
        .collect();

    let commands: Vec<(String, Value)> = vec![
        ("scan", json!({"cmd":"scan"})),
        ("connect_device", json!({"cmd":"connect_device","id":"mon"})),
        ("disconnect", json!({"cmd":"disconnect"})),
        ("info", json!({"cmd":"info"})),
        ("scene_save", json!({"cmd":"scene_save","n":1,"name":"Set 1"})),
        ("scene_recall", json!({"cmd":"scene_recall","n":1,"fade_ms":0})),
        ("scene_rename", json!({"cmd":"scene_rename","n":1,"name":"Set 1b"})),
        ("scene_delete", json!({"cmd":"scene_delete","n":1})),
        ("scene_clear", json!({"cmd":"scene_clear"})),
        ("fx_set_type", json!({"cmd":"fx_set_type","slot":1,"type":"Hall"})),
        ("fx_param", json!({"cmd":"fx_param","slot":1,"id":"mix","value":0.8})),
        ("fx_bypass", json!({"cmd":"fx_bypass","slot":1,"bypass":true})),
        ("fx_on", json!({"cmd":"fx_on","slot":1,"on":false})),
        ("patch_set", json!({"cmd":"patch_set","strip":0,"group":"Net","index":5})),
        ("patch_clear", json!({"cmd":"patch_clear","strip":0})),
        ("offline", json!({"cmd":"offline","model":"xair"})),
        ("offline_push", json!({"cmd":"offline_push","confirm":true})),
        ("offline_discard", json!({"cmd":"offline_discard"})),
        ("hello_taps", json!({"cmd":"hello","taps":true})),
    ]
    .into_iter()
    .map(|(name, v)| (format!("cmd_{name}"), v))
    .collect();
    for (name, v) in &commands {
        let text = serde_json::to_string(v).unwrap();
        openmix_engine::parse_cmd(&text)
            .unwrap_or_else(|e| panic!("r2 golden command {name} parses: {e}"));
    }
    out.extend(commands);
    out
}

#[test]
fn r2_golden_frames_validate() {
    let dir = golden_dir();
    let update = std::env::var("R2GOLDEN_UPDATE").is_ok();
    if update {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut names: Vec<String> = Vec::new();
    for (name, example) in examples() {
        names.push(name.clone());
        let path = dir.join(format!("{name}.json"));
        if update {
            std::fs::write(&path, serde_json::to_string_pretty(&example).unwrap() + "\n")
                .unwrap();
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing r2 golden fixture {name}.json (run with R2GOLDEN_UPDATE=1)"));
        let file: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(file, example, "r2 golden fixture {name}.json matches the constructor");
        // Commands carry `cmd` (an fx command also carries a `type` field —
        // the FX type — so `cmd` wins, mirroring golden.test.ts).
        if file.get("cmd").and_then(Value::as_str).is_some() {
            let cmd: Cmd = serde_json::from_value(file.clone())
                .unwrap_or_else(|e| panic!("r2 golden command {name}.json parses as Cmd: {e}"));
            let back = serde_json::to_string(&cmd).unwrap();
            openmix_engine::parse_cmd(&back)
                .unwrap_or_else(|e| panic!("r2 golden command {name} re-parses: {e}"));
        } else {
            let event: Event = serde_json::from_value(file.clone()).unwrap_or_else(|e| {
                panic!("r2 golden frame {name}.json parses as Event: {e}")
            });
            let back: Value = serde_json::to_value(&event).unwrap();
            assert_eq!(back, file, "r2 golden frame {name}.json round-trips");
            // Frame tag matches the file name (hello_taps is the hello ack
            // with taps negotiated).
            let tag = event.tag();
            assert!(
                tag == name.as_str() || (name == "hello_taps" && tag == "hello"),
                "frame tag matches its file name ({name} -> {tag})"
            );
        }
    }
    names.sort();
    let files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|f| f.ends_with(".json"))
        .collect();
    assert_eq!(files.len(), names.len(), "no stray JSON fixtures in tests/golden_r2");
}

/// Reference decoder for binary taps frames (type 3): header like meters
/// plus one tap-id byte, then `n × u16`. Mirrors `to_binary`; the node
/// transcript re-implements it in JS.
fn decode_taps(bytes: &[u8]) -> (u8, u16, u32, u32, u8, Vec<u16>) {
    assert!(bytes.len() > BIN_HEADER_LEN, "taps frame shorter than header+tap");
    assert_eq!(bytes[0], BIN_TYPE_TAPS);
    assert_eq!(bytes[1], BIN_VERSION);
    let n = u16::from_le_bytes([bytes[2], bytes[3]]);
    let seq = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let t_ms = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let tap = bytes[12];
    let body = &bytes[13..];
    assert_eq!(body.len(), n as usize * 2, "taps payload is n × u16");
    let qs = body
        .as_chunks::<2>().0.iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    (bytes[0], n, seq, t_ms, tap, qs)
}

fn hex_of(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

fn bytes_of(hex: &str) -> Vec<u8> {
    let h = hex.trim();
    assert!(h.len().is_multiple_of(2), "odd hex length");
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Golden binary taps frames (deterministic seq/t_ms so the hex is
/// stable). Regenerate with `R2GOLDEN_UPDATE=1`.
#[test]
fn r2_binary_taps_golden() {
    let dir = golden_dir();
    let update = std::env::var("R2GOLDEN_UPDATE").is_ok();
    let cases: Vec<(&str, Event)> = vec![
        (
            "taps_in",
            Event::Taps {
                tap: "in".to_string(),
                levels: vec![0.5, 0.25, 0.0, 0.75],
            },
        ),
        (
            "taps_gr",
            Event::Taps {
                tap: "gr_comp".to_string(),
                levels: vec![0.0, 6.0, 12.0],
            },
        ),
    ];
    for (name, event) in &cases {
        let bytes = event.to_binary(7, 1234).expect("taps encodes");
        let path = dir.join(format!("{name}.bin.hex"));
        if update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, hex_of(&bytes) + "\n").unwrap();
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing {name}.bin.hex (run with R2GOLDEN_UPDATE=1)"));
        assert_eq!(bytes_of(&raw), bytes, "{name}.bin.hex matches to_binary");
    }
    // Decode round-trip through the reference decoder.
    let bytes = cases[0].1.to_binary(7, 1234).unwrap();
    let (ftype, n, seq, t_ms, tap, qs) = decode_taps(&bytes);
    assert_eq!((ftype, n, seq, t_ms), (BIN_TYPE_TAPS, 4, 7, 1234));
    assert_eq!(tap, tap_id("in"));
    let levels: Vec<f32> = qs.iter().map(|q| u16_to_taps_level("in", *q)).collect();
    for (got, want) in levels.iter().zip([0.5f32, 0.25, 0.0, 0.75]) {
        assert!((got - want).abs() <= 1.0 / 65535.0 + 1e-6, "in tap {got} ~= {want}");
    }
    // GR taps scale through TAPS_GR_DB_FULLSCALE (24 dB).
    let bytes = cases[1].1.to_binary(7, 1234).unwrap();
    let (_, n, _, _, tap, qs) = decode_taps(&bytes);
    assert_eq!((n, tap), (3, tap_id("gr_comp")));
    assert_eq!(taps_level_to_u16("gr_comp", 12.0), (0.5f32 * 65535.0).round() as u16);
    let back: Vec<f32> = qs.iter().map(|q| u16_to_taps_level("gr_comp", *q)).collect();
    for (got, want) in back.iter().zip([0.0f32, 6.0, 12.0]) {
        assert!((got - want).abs() <= TAPS_GR_DB_FULLSCALE / 65535.0 + 1e-6, "gr {got} ~= {want}");
    }
    // Unknown tap names map to id 255 and ride the level codec.
    assert_eq!(tap_id("nope"), 255);
}
