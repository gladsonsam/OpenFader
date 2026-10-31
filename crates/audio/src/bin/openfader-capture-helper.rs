//! Single-session capture subprocess. Protocol is newline-delimited JSON on stdin/stdout.
#[cfg(any(target_os = "macos", target_os = "windows"))]
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
        let mut service = RecorderService::new(root)?;
        service.select_discovered_device(device)?;
        if playback {
            service.set_playback_mask(mask)?;
            let id = request["take_id"]
                .as_str()
                .ok_or_else(|| io::Error::other("missing take_id"))?;
            service.load_take(id)?;
        } else {
            service.prepare_capture()?;
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
                    service.commit_capture(id)?;
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
                    service.stop()?;
                    send(
                        json!({"state":"STOPPED","generation":generation,"status":service.status()}),
                    )?;
                    return Ok(());
                }
                _ => return Err(io::Error::other("unknown operation")),
            }
        }
        service.stop()
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("capture helper unsupported on this platform");
    std::process::exit(1);
}
