//! Workspace integration: sim fader roundtrip + CLI --help smoke.
use openfader_core::{MixerDriver, ParamValue, StripId};
use std::process::Command;

#[tokio::test]
async fn sim_fader_roundtrip() {
    let drv = openfader_sim::SimDriver::new();
    drv.connect("sim://local").await.unwrap();
    assert_eq!(drv.name(), "sim");
    assert_eq!(drv.channel_count(), 32);

    // Fader roundtrip via dedicated + generic param paths.
    drv.set_fader(StripId(0), 0.5).await.unwrap();
    assert!((drv.get_fader(StripId(0)).await.unwrap() - 0.5).abs() < 1e-6);
    drv.set_param(StripId(1), "fader", ParamValue::Float(0.25))
        .await
        .unwrap();
    assert!(matches!(
        drv.get_param(StripId(1), "fader").await.unwrap(),
        ParamValue::Float(v) if (v - 0.25).abs() < 1e-6
    ));

    // Mute + naming.
    drv.set_mute(StripId(2), true).await.unwrap();
    assert!(drv.get_mute(StripId(2)).await.unwrap());
    drv.set_name(StripId(0), "KICK").await.unwrap();
    assert_eq!(drv.get_name(StripId(0)).await.unwrap(), "KICK");

    // Buses + main.
    drv.set_bus_fader(0, 0.6).await.unwrap();
    assert!((drv.get_bus_fader(0).await.unwrap() - 0.6).abs() < 1e-6);
    drv.set_main_fader(0.8).await.unwrap();
    assert!((drv.get_main_fader().await.unwrap() - 0.8).abs() < 1e-6);

    // Snapshot recall restores prior state.
    let snap = drv.snapshot().await.unwrap();
    assert_eq!(snap.strips.len(), openfader_sim::SIM_STRIPS); // full v2 topology (77 strips)
    drv.set_fader(StripId(0), 0.9).await.unwrap();
    drv.recall(&snap).await.unwrap();
    assert!((drv.get_fader(StripId(0)).await.unwrap() - 0.5).abs() < 1e-6);

    // Rejects bad input.
    assert!(drv.set_fader(StripId(0), 2.0).await.is_err());
    assert!(drv.set_fader(StripId(999), 0.5).await.is_err());

    // Fake meters: one frame per strip, muted strip reads 0.
    let frame = drv.poll_meters();
    assert_eq!(frame.levels.len(), openfader_sim::SIM_STRIPS);
    assert_eq!(frame.levels[2], 0.0);
    assert!(frame.levels[0] > 0.0);
}

#[test]
fn cli_help_smoke() {
    let bin = env!("CARGO_BIN_EXE_openfader-cli");
    let out = Command::new(bin)
        .arg("--help")
        .output()
        .expect("run openfader-cli --help");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Usage"), "help text missing Usage: {text}");
    assert!(text.contains("fader"));
    assert!(text.contains("mute"));
}

#[test]
fn cli_list_smoke() {
    let bin = env!("CARGO_BIN_EXE_openfader-cli");
    let out = Command::new(bin)
        .args(["--driver", "sim", "list"])
        .output()
        .expect("run openfader-cli list");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("CH01"), "list missing strips: {text}");
}
