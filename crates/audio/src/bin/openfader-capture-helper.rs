//! Single-session capture subprocess. Protocol is newline-delimited JSON on stdin/stdout.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn main() {
    use openfader_audio::{DeviceInfo, RecorderService};
    use serde_json::{json, Value};
    use std::io::{self, BufRead, Write};

    fn send(value: Value) -> io::Result<()> {
        let mut out = io::stdout().lock();
        serde_json::to_writer(&mut out, &value)?;
        writeln!(out)?;
        out.flush()
    }
    fn run() -> io::Result<()> {
        let mut lines = io::stdin().lock().lines();
        let first = lines
            .next()
            .ok_or_else(|| io::Error::other("missing prepare"))??;
        let request: Value = serde_json::from_str(&first)?;
        let playback = request["op"] == "prepare_playback";
        if request["op"] != "prepare" && !playback {
            return Err(io::Error::other("expected prepare"));
        }
        let generation = request["generation"]
            .as_u64()
            .ok_or_else(|| io::Error::other("missing generation"))?;
        let root = request["root"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing root"))?;
        let device: DeviceInfo = serde_json::from_value(request["device"].clone())?;
        let mask: Vec<bool> = if playback {
            let supplied = request
                .get("output_mask")
                .filter(|v| !v.is_null())
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()?;
            openfader_audio::playback_output_mask(supplied, device.output_channels)?
        } else {
            vec![true; device.output_channels as usize]
        };
        // Lock before READY: once the parent can ask us to play, recovery can
        // prove this process has exited by acquiring the same lock.
        let _playback_guard = if playback {
            Some(openfader_audio::playback_guard(std::path::Path::new(root))?)
        } else {
            None
        };
        if let Some(session) = request.get("manual_session").and_then(Value::as_str) {
            let marker =
                std::fs::read_to_string(std::path::Path::new(root).join("manual-routing-pending"))?;
            if session.is_empty() || marker != session {
                return Err(io::Error::other("manual routing session expired"));
            }
        }
        if !playback {
            send(
                json!({"state":"PROGRESS","generation":generation,"stage":"initializing_recorder_storage"}),
            )?;
        }
        let mut service = RecorderService::new(root)?;
        service.select_discovered_device(device)?;
        if playback {
            service.set_playback_mask(mask)?;
            if let Some(mapping) = request.get("output_mapping").filter(|v| !v.is_null()) {
                service.set_playback_mapping(serde_json::from_value(mapping.clone())?)?;
            }
            let id = request["take_id"]
                .as_str()
                .ok_or_else(|| io::Error::other("missing take_id"))?;
            service.load_take(id)?;
        } else {
            service.prepare_capture_with(|stage| {
                let _ = send(json!({"state":"PROGRESS","generation":generation,"stage":stage}));
            })?;
        }
        send(json!({"state":"READY","generation":generation}))?;
        for line in lines {
            let request: Value = serde_json::from_str(&line?)?;
            if request["generation"].as_u64() != Some(generation) {
                return Err(io::Error::other("generation mismatch"));
            }
            match request["op"].as_str() {
                Some("commit") if !playback => {
                    let id = request["take_id"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("missing take_id"))?;
                    let name = match request.get("name") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(name)) => Some(name.as_str()),
                        Some(_) => return Err(io::Error::other("name must be a string")),
                    };
                    let channel_names: Option<Vec<String>> = match request.get("channel_names") {
                        None | Some(Value::Null) => None,
                        Some(value) => {
                            Some(serde_json::from_value(value.clone()).map_err(io::Error::other)?)
                        }
                    };
                    service.commit_capture_with_channels_progress(
                        id,
                        name,
                        channel_names.as_deref(),
                        |stage| {
                            let _ = send(
                                json!({"state":"PROGRESS","generation":generation,"stage":stage}),
                            );
                        },
                    )?;
                    send(
                        json!({"state":"RECORDING","generation":generation,"status":service.status()}),
                    )?;
                }
                Some("play") if playback => {
                    service.play()?;
                    send(
                        json!({"state":"PLAYED","generation":generation,"status":service.status()}),
                    )?;
                }
                Some("pause") if playback => {
                    service.pause()?;
                    send(
                        json!({"state":"PAUSED","generation":generation,"status":service.status()}),
                    )?;
                }
                Some("seek") if playback => {
                    let frame = request["frame"]
                        .as_u64()
                        .ok_or_else(|| io::Error::other("missing frame"))?;
                    service.seek(frame)?;
                    send(
                        json!({"state":"SEEKED","generation":generation,"status":service.status()}),
                    )?;
                }
                Some("set_loop") if playback => {
                    let start = request["start"]
                        .as_u64()
                        .ok_or_else(|| io::Error::other("missing start"))?;
                    let end = request["end"]
                        .as_u64()
                        .ok_or_else(|| io::Error::other("missing end"))?;
                    let enabled = request["enabled"]
                        .as_bool()
                        .ok_or_else(|| io::Error::other("missing enabled"))?;
                    service.set_loop(start, end, enabled)?;
                    send(
                        json!({"state":"LOOP_SET","generation":generation,"status":service.status()}),
                    )?;
                }
                Some("status") => send(
                    json!({"state":"STATUS","generation":generation,"status":service.status()}),
                )?,
                Some("stop") => {
                    // Stop removes the stream and joins its writer even when an
                    // interrupted take cannot finalize. Acknowledge termination
                    // with the diagnostic intact so the host can offer recovery.
                    let stop_error = service.stop().err().map(|e| e.to_string());
                    send(
                        json!({"state":"STOPPED","generation":generation,"status":service.status(),"stop_error":stop_error}),
                    )?;
                    return Ok(());
                }
                _ => return Err(io::Error::other("unknown operation")),
            }
        }
        service.stop()
    }
    if std::env::args().nth(1).as_deref() == Some("--list-devices") {
        let result = RecorderService::new(std::env::temp_dir()).and_then(|s| s.devices());
        let response = match result {
            Ok(devices) => json!({"state":"DEVICES", "devices":devices}),
            Err(error) => json!({"state":"ERROR", "error":error.to_string()}),
        };
        if send(response).is_err() {
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--probe-output") {
        let _ = send(json!({"stage":"starting"}));
        let result = (|| -> io::Result<()> {
            let args: Vec<_> = std::env::args().collect();
            if args.len() != 4 || args[2].len() > 4096 {
                return Err(io::Error::other("expected device id and sample rate"));
            }
            let rate: u32 = args[3]
                .parse()
                .map_err(|_| io::Error::other("invalid sample rate"))?;
            send(json!({"stage":"building_stream"}))?;
            RecorderService::probe_output_with(&args[2], rate, || {
                let _ = send(json!({"stage":"stream_built"}));
            })?;
            send(json!({"stage":"stream_dropped"}))
        })();
        if let Err(error) = result {
            let message: String = error.to_string().chars().take(512).collect();
            let _ = send(json!({"stage":"error", "error":message}));
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run() {
        let _ = send(json!({"state":"ERROR","error":error.to_string()}));
        std::process::exit(1);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn main() {
    eprintln!("capture helper unsupported on this platform");
    std::process::exit(1);
}
