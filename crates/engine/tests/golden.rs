//! Golden frames: canonical `Cmd`/`Event` JSON fixtures shared by the Rust
//! tests and the TypeScript parser test.
//!
//! `tests/golden/*.json` are committed fixtures (one file per frame type,
//! one per command type). This test validates them: every frame parses as
//! an [`Event`](openmix_engine::Event) and round-trips, every command parses
//! as a [`Cmd`](openmix_engine::Cmd). Run with `GOLDEN_UPDATE=1` to
//! regenerate the fixtures from the constructors below (then commit).

use std::collections::BTreeMap;
use std::path::PathBuf;

use openmix_engine::{Cmd, Event};
use serde_json::{json, Value};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// (filename, example value) pairs. Frame examples are built through the
/// real [`Event`] constructors so the fixtures track the wire shapes; the
/// command examples are the canonical client spellings (including the
/// `chs` array form of `focus` and the `set_main_fader` alias).
fn examples() -> Vec<(String, Value)> {
    let frames: Vec<(&str, Event)> = vec![
        (
            "you",
            Event::You {
                id: "client-1".to_string(),
            },
        ),
        (
            "state",
            Event::State {
                driver: "sim".to_string(),
                strips: vec![
                    openmix_engine::protocol::StateStrip {
                        ch: 0,
                        name: "CH01".to_string(),
                        fader: 0.5,
                        muted: false,
                    },
                    openmix_engine::protocol::StateStrip {
                        ch: 1,
                        name: "Vox".to_string(),
                        fader: 0.25,
                        muted: true,
                    },
                ],
                main_fader: Some(0.8),
                by: Some("client-1".to_string()),
            },
        ),
        (
            "param",
            Event::Param {
                ch: 2,
                field: "gain".to_string(),
                value: json!(12.5),
                by: Some("client-2".to_string()),
            },
        ),
        (
            "shows",
            Event::Shows {
                shows: vec![openmix_engine::protocol::Show {
                    name: "gig".to_string(),
                    scenes: vec![openmix_engine::protocol::Scene {
                        name: "verse".to_string(),
                        main_fader: Some(0.8),
                        strips: vec![
                            openmix_engine::protocol::SceneStrip {
                                ch: 0,
                                name: "CH01".to_string(),
                                fader: 0.5,
                                muted: false,
                            },
                            openmix_engine::protocol::SceneStrip {
                                ch: 1,
                                name: "Vox".to_string(),
                                fader: 0.25,
                                muted: true,
                            },
                        ],
                    }],
                }],
            },
        ),
        (
            "meters",
            Event::Meters {
                levels: vec![0.5, 0.25, 0.0, 0.75],
            },
        ),
        (
            "spectrum",
            Event::Spectrum {
                ch: 0,
                bins: (0..64).map(|i| i as f32 / 64.0).collect(),
            },
        ),
        (
            "gr",
            Event::Gr {
                ch: 1,
                gate: 3.5,
                comp: 0.0,
            },
        ),
        (
            "caps",
            Event::Caps {
                driver: "sim".to_string(),
                params: vec![
                    openmix_engine::protocol::ParamDef {
                        id: "gain".to_string(),
                        kind: "float".to_string(),
                        scope: "channel".to_string(),
                        scopes: vec!["in".to_string()],
                        min: json!(-12.0),
                        max: json!(60.0),
                        unit: "dB".to_string(),
                        default: json!(20.0),
                        label: "Gain".to_string(),
                    },
                    openmix_engine::protocol::ParamDef {
                        id: "phantom".to_string(),
                        kind: "bool".to_string(),
                        scope: "channel".to_string(),
                        scopes: vec!["in".to_string()],
                        min: Value::Null,
                        max: Value::Null,
                        unit: String::new(),
                        default: json!(false),
                        label: "Phantom".to_string(),
                    },
                ],
                unsupported: vec!["usbreturn".to_string()],
                topology: vec![openmix_engine::protocol::TopologyFamily {
                    key: "ch".to_string(),
                    kind: "in".to_string(),
                    label: "Ch 1-32".to_string(),
                    first: 0,
                    count: 32,
                    name_prefix: "Ch".to_string(),
                    color_default: 2,
                    params_scope: vec!["in".to_string()],
                }],
                meter_taps: vec![
                    "in".to_string(),
                    "post".to_string(),
                    "gr_gate".to_string(),
                    "gr_comp".to_string(),
                ],
                // R2 FX/routing blocks (sim: 8 design slots + six type
                // tables; Lcl/Net/USB patching fully supported).
                fx: openmix_engine::protocol::FxCaps {
                    slots: 8,
                    types: vec![openmix_engine::protocol::FxTypeCaps {
                        id: "hall".to_string(),
                        label: "Hall".to_string(),
                        params: vec![openmix_engine::protocol::FxParamCaps {
                            id: "mix".to_string(),
                            label: "Mix".to_string(),
                            unit: "%".to_string(),
                            min: 0.0,
                            max: 100.0,
                            default: 1.0,
                            scale: "lin".to_string(),
                        }],
                    }],
                },
                routing: openmix_engine::protocol::RoutingCaps {
                    groups: vec![openmix_engine::protocol::RoutingGroupCaps {
                        key: "Lcl".to_string(),
                        label: "Local".to_string(),
                        count: 32,
                    }],
                    per_strip: true,
                    supported: true,
                },
                groups: openmix_engine::protocol::GroupsCaps {
                    buses: 8,
                    subgroups: 6,
                    mute_groups: 6,
                    mg_assign: true,
                    sub_assign: true,
                },
                files: openmix_engine::protocol::FilesCaps { supported: true },
            },
        ),
        (
            "params",
            Event::Params {
                ch: 3,
                values: [
                    ("gain".to_string(), json!(12.5)),
                    ("phantom".to_string(), json!(false)),
                    ("name".to_string(), json!("Vox")),
                ]
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
            },
        ),
        (
            "locks",
            Event::Locks {
                holds: [("0".to_string(), "client-1".to_string())]
                    .into_iter()
                    .collect(),
            },
        ),
        (
            "clients",
            Event::Clients {
                count: 2,
                writers: vec!["client-1".to_string()],
            },
        ),
        (
            "error",
            Event::Error {
                message: "strip 0 locked by client-1".to_string(),
                locked_by: Some("client-1".to_string()),
                ch: Some(0),
            },
        ),
        (
            "recalled",
            Event::Recalled {
                overrode_locks: vec![1],
            },
        ),
        (
            "bus",
            Event::Bus {
                bus: 0,
                fader: 0.75,
                muted: false,
            },
        ),
        (
            "subgroup",
            Event::Subgroup {
                group: 1,
                fader: 0.7,
                muted: true,
            },
        ),
        (
            "subgroup_assign",
            Event::SubgroupAssign {
                ch: 0,
                group: 1,
                member: true,
            },
        ),
        (
            "mute_group",
            Event::MuteGroup {
                group: 0,
                muted: true,
            },
        ),
        (
            "mute_group_assign",
            Event::MuteGroupAssign {
                ch: 5,
                group: 2,
                member: false,
            },
        ),
        (
            "file",
            Event::File {
                filename: "Show.zip".to_string(),
                size: 4,
                file_type: 0,
                data: Some("AAECAw==".to_string()),
            },
        ),
    ];
    let mut out: Vec<(String, Value)> = frames
        .into_iter()
        .map(|(name, e)| (name.to_string(), serde_json::to_value(&e).unwrap()))
        .collect();

    // Command fixtures live under `cmd_*` so the `param` frame and the
    // `param` command do not share a file name.
    let commands: Vec<(String, Value)> = vec![
        ("connect", json!({"cmd":"connect","driver":"sim"})),
        ("fader", json!({"cmd":"fader","ch":0,"value":0.5})),
        ("main_fader", json!({"cmd":"main_fader","value":0.55})),
        (
            "set_main_fader",
            json!({"cmd":"set_main_fader","value":0.55}),
        ),
        ("mute", json!({"cmd":"mute","ch":0,"muted":true})),
        (
            "param",
            json!({"cmd":"param","ch":1,"field":"gain","value":12.5}),
        ),
        ("focus", json!({"cmd":"focus","ch":5})),
        ("focus_multi", json!({"cmd":"focus","chs":[3,7]})),
        ("lock", json!({"cmd":"lock","ch":0})),
        ("unlock", json!({"cmd":"unlock","ch":0})),
        ("list_shows", json!({"cmd":"list_shows"})),
        (
            "capture_scene",
            json!({"cmd":"capture_scene","show":"gig","name":"verse"}),
        ),
        (
            "recall_scene",
            json!({"cmd":"recall_scene","show":"gig","name":"verse","fade_ms":0}),
        ),
        (
            "delete_scene",
            json!({"cmd":"delete_scene","show":"gig","name":"verse"}),
        ),
        ("bus_fader", json!({"cmd":"bus_fader","bus":0,"value":0.5})),
        ("bus_mute", json!({"cmd":"bus_mute","bus":0,"muted":true})),
        (
            "subgroup_fader",
            json!({"cmd":"subgroup_fader","group":1,"value":0.7}),
        ),
        (
            "subgroup_mute",
            json!({"cmd":"subgroup_mute","group":1,"muted":true}),
        ),
        (
            "subgroup_assign",
            json!({"cmd":"subgroup_assign","ch":0,"group":1,"member":true}),
        ),
        ("mute_group", json!({"cmd":"mute_group","group":0,"muted":true})),
        (
            "mute_group_assign",
            json!({"cmd":"mute_group_assign","ch":5,"group":2,"member":false}),
        ),
        (
            "file_download",
            json!({"cmd":"file_download","filename":"Show.zip"}),
        ),
        (
            "file_upload",
            json!({"cmd":"file_upload","filename":"Show.zip","data":"AAECAw==","file_type":0}),
        ),
    ]
    .into_iter()
    .map(|(name, v)| (format!("cmd_{name}"), v))
    .collect();
    // Every command example must parse as a Cmd (validates the fixtures).
    for (name, v) in &commands {
        let text = serde_json::to_string(v).unwrap();
        openmix_engine::parse_cmd(&text)
            .unwrap_or_else(|e| panic!("golden command {name} parses: {e}"));
    }
    out.extend(commands);
    out
}

#[test]
fn golden_frames_validate() {
    let dir = golden_dir();
    let update = std::env::var("GOLDEN_UPDATE").is_ok();
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
            .unwrap_or_else(|_| panic!("missing golden fixture {name}.json (run with GOLDEN_UPDATE=1)"));
        let file: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            file, example,
            "golden fixture {name}.json matches the constructor"
        );
        // Frames round-trip through the protocol types.
        if file.get("type").is_some() {
            let event: Event = serde_json::from_value(file.clone()).unwrap_or_else(|e| {
                panic!("golden frame {name}.json parses as Event: {e}")
            });
            let back: Value = serde_json::to_value(&event).unwrap();
            assert_eq!(back, file, "golden frame {name}.json round-trips");
            assert_eq!(event.tag(), name.as_str(), "frame tag matches its file name");
        } else {
            let cmd: Cmd = serde_json::from_value(file.clone())
                .unwrap_or_else(|e| panic!("golden command {name}.json parses as Cmd: {e}"));
            let back = serde_json::to_string(&cmd).unwrap();
            openmix_engine::parse_cmd(&back)
                .unwrap_or_else(|e| panic!("golden command {name} re-parses: {e}"));
        }
    }
    names.sort();
    let files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        files.len(),
        names.len(),
        "no stray fixtures in tests/golden"
    );
}
