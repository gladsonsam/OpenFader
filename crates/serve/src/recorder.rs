//! Serialized desktop audio worker. The service and streams remain on its thread.
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

#[path = "soundcheck_backend.rs"]
pub mod soundcheck_backend;

#[derive(Clone)]
pub struct Recorder {
    tx: tokio::sync::mpsc::UnboundedSender<(Value, oneshot::Sender<Value>)>,
}
impl Recorder {
    pub fn start(root: PathBuf, engine: std::sync::Arc<openfader_engine::Engine>) -> Self {
        let config = crate::coordinator_backend::BackendConfig::new(
            root.join("routing-journal.json"),
            root.clone(),
        );
        let backend = crate::coordinator_backend::CoordinatorBackend::new(engine.clone(), config);
        Self::start_with_backend(root, engine, Some(Box::new(backend)))
    }
    /// Inject a backend for the serialized recorder worker.
    pub fn start_with_backend(
        root: PathBuf,
        engine: std::sync::Arc<openfader_engine::Engine>,
        backend: Option<Box<dyn soundcheck_backend::SoundcheckBackend>>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<(Value, oneshot::Sender<Value>)>();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(worker(root, rx, engine, backend));
        });
        Self { tx }
    }
    pub async fn request(&self, value: Value) -> Value {
        let (tx, rx) = oneshot::channel();
        if self.tx.send((value, tx)).is_err() {
            return json!({"error":"recorder worker unavailable"});
        }
        rx.await
            .unwrap_or_else(|_| json!({"error":"recorder worker unavailable"}))
    }
}
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/OpenFader/Recorder"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("OpenFader/Recorder"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn worker(
    root: PathBuf,
    rx: tokio::sync::mpsc::UnboundedReceiver<(Value, oneshot::Sender<Value>)>,
    engine: std::sync::Arc<openfader_engine::Engine>,
    backend: Option<Box<dyn soundcheck_backend::SoundcheckBackend>>,
) {
    worker_with_config(root, rx, None, Some(engine), backend).await;
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn worker_with_config(
    root: PathBuf,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(Value, oneshot::Sender<Value>)>,
    fixture: Option<(PathBuf, openfader_audio::DeviceInfo)>,
    engine: Option<std::sync::Arc<openfader_engine::Engine>>,
    mut backend: Option<Box<dyn soundcheck_backend::SoundcheckBackend>>,
) {
    let mut soundcheck = crate::soundcheck::Soundcheck::new(
        crate::routing_transaction::RoutingTransaction::new(root.join("routing-journal.json")),
    );
    let mut service = openfader_audio::RecorderService::new(&root);
    let mut requests: HashMap<String, (Value, Value)> = HashMap::new();
    let mut shutting_down = false;
    // CoreAudio device drivers can block indefinitely while enumerating. Keep
    // that call off the serialized transport worker and never spawn a second
    // scan while the first one is still outstanding.
    let mut scan: Option<mpsc::Receiver<Result<Vec<openfader_audio::DeviceInfo>, String>>> = None;
    let mut devices = Vec::new();
    let mut scan_started = false;
    let mut scan_error: Option<String> = None;
    let helper = std::env::var_os("OPENFADER_CAPTURE_HELPER")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().map(|p| {
                p.with_file_name(if cfg!(windows) {
                    "openfader-capture-helper.exe"
                } else {
                    "openfader-capture-helper"
                })
            })
        });
    let mut selected = None;
    let helper = if let Some((path, device)) = fixture {
        devices.push(device.clone());
        selected = Some(device);
        Some(path)
    } else {
        helper
    };
    let mut generation = 0u64;
    type Pending = (
        String,
        Value,
        Vec<oneshot::Sender<Value>>,
        tokio::task::JoinHandle<Result<(crate::capture_client::CaptureClient, Value), String>>,
    );
    let mut pending: Option<Pending> = None;
    let mut client: Option<crate::capture_client::CaptureClient> = None;
    let mut capture_status: Option<Value> = None;
    let mut capture_error: Option<String> = None;
    let mut cached_backend = backend.as_ref().map(|b| (b.state(), b.available()));
    let mut soundcheck_reason = backend.as_ref().and_then(|b| b.unavailable_reason());
    let mut playback_telemetry: Option<Value> = None;
    let mut backend_operation = 0u64;
    type BackendTask = (
        String,
        Value,
        Vec<oneshot::Sender<Value>>,
        tokio::task::JoinHandle<(
            Box<dyn soundcheck_backend::SoundcheckBackend>,
            Result<(), String>,
        )>,
        u64,
    );
    let mut backend_task: Option<BackendTask> = None;
    let mut queued_stops: Vec<(Value, oneshot::Sender<Value>)> = Vec::new();
    let mut backend_cancel: Option<soundcheck_backend::CancellationHandle> = None;
    let mut quarantined = false;
    let mut quarantine_handle: Option<soundcheck_backend::CancellationHandle> = None;
    let mut backend_deadline: Option<tokio::time::Instant> = None;
    let mut backend_overdue = false;
    let mut rx_open = true;
    loop {
        if !rx_open && backend_task.is_none() && queued_stops.is_empty() {
            break;
        }
        let (request, reply) = tokio::select! {
            _ = async { tokio::time::sleep_until(backend_deadline.unwrap()).await }, if backend_deadline.is_some() => {
                backend_deadline = None;
                backend_overdue = true;
                if let Some(handle) = &backend_cancel {
                    handle.cancel();
                    quarantine_handle = Some(handle.clone());
                    quarantined = true;
                }
                continue;
            },
            completion = async { (&mut backend_task.as_mut().unwrap().3).await }, if backend_task.is_some() => {
                backend_deadline = None;
                backend_overdue = false;
                let (id, cmd, replies, _, operation) = backend_task.take().unwrap();
                let stale = operation != backend_operation;
                let handle = backend_cancel.take();
                let cancelled = stale || handle.as_ref().is_some_and(|h| h.cancellation_requested());
                if cancelled {
                    quarantined = true;
                    if quarantine_handle.is_none() { quarantine_handle = handle; }
                }
                let response = match completion {
                    Ok((returned, result)) => {
                        let state = returned.state();
                        if quarantined && !cancelled && result.is_ok()
                            && matches!(cmd["action"].as_str(), Some("return_live" | "accept_current_routing")) && state.input_mode == "live"
                            && !root.join("routing-journal.json").exists()
                            && quarantine_handle.as_ref().is_some_and(|h| h.output_terminated())
                        {
                            quarantined = false;
                            quarantine_handle = None;
                        }
                        if !stale { soundcheck_reason = returned.unavailable_reason(); cached_backend = Some((state, returned.available())); }
                        // The actor serializes backend ownership; retain the completed owner for recovery.
                        if backend.is_none() { backend = Some(returned); }
                        match result {
                            _ if cancelled => json!({"ok":false,"request_id":id,"error":"operation cancelled; routing recovery required"}),
                            Ok(()) => json!({"ok":true,"request_id":id,"error":null}),
                            Err(e) => json!({"ok":false,"request_id":id,"error":e}),
                        }
                    }
                    Err(e) => json!({"ok":false,"request_id":id,"error":format!("backend task failed: {e}")}),
                };
                requests.insert(id, (cmd, response.clone()));
                for reply in replies { let _ = reply.send(response.clone()); }
                continue;
            },
            queued = async { queued_stops.remove(0) }, if backend_task.is_none() && !queued_stops.is_empty() => queued,
            completion = async { (&mut pending.as_mut().unwrap().3).await }, if pending.is_some() => {
                let (id, cmd, replies, _) = pending.take().unwrap();
                let response = match completion {
                    Ok(Ok((c, status))) => { client = Some(c); capture_status = Some(status); capture_error = None; json!({"ok":true,"request_id":id,"error":null}) },
                    other => {
                        let error = format!("capture failed: {}", match other { Ok(Err(e)) => e, Err(e) => e.to_string(), _ => unreachable!() });
                        capture_error = Some(error.clone());
                        json!({"ok":false,"request_id":id,"error":error})
                    },
                };
                requests.insert(id, (cmd, response.clone()));
                for reply in replies { let _ = reply.send(response.clone()); }
                continue;
            },
            request = rx.recv(), if rx_open => match request { Some(v) => v, None => { rx_open = false; continue; } },
        };
        let result = if shutting_down {
            json!({"error":"host shutting down"})
        } else {
            match service.as_mut() {
                Err(e) => json!({"error":format!("recorder initialization failed: {e}")}),
                Ok(_s)
                    if quarantined
                        && !matches!(request["op"].as_str(), Some("state" | "command")) =>
                {
                    json!({"error":"routing recovery required; wait for backend and confirmed output termination"})
                }
                Ok(s) => match request["op"].as_str().unwrap_or("") {
                    "shutdown" => {
                        if backend_task.is_some()
                            || pending.is_some()
                            || client.is_some()
                            || s.status().transport == "recording"
                            || root.join("routing-journal.json").exists()
                        {
                            json!({"ok":false,"error":"recording or routing recovery active"})
                        } else {
                            shutting_down = true;
                            json!({"ok":true})
                        }
                    }
                    "state" => {
                        if let Some(b) = backend.as_mut() {
                            playback_telemetry = b.telemetry().await;
                            cached_backend = Some((b.state(), b.available()));
                            soundcheck_reason = b.unavailable_reason();
                        }
                        if let Some(c) = &client {
                            match c.status().await {
                                Ok(v) => capture_status = Some(v["status"].clone()),
                                Err(e) => {
                                    client = None;
                                    capture_status = None;
                                    capture_error = Some(e);
                                }
                            }
                        }
                        let mut v = capture_status.clone().unwrap_or_else(|| {
                            serde_json::to_value(s.status()).unwrap_or_else(|_| json!({}))
                        });
                        if let Some(e) = &capture_error {
                            v["capture_error"] = json!(e);
                        }
                        if let Some(o) = v.as_object_mut() {
                            o.insert("available".into(), json!(true));
                            o.insert(
                                "channels".into(),
                                selected
                                    .as_ref()
                                    .map(|device| json!(device.input_channels))
                                    .unwrap_or(Value::Null),
                            );
                            o.insert("soundcheck_reason".into(), if selected.is_none() {
                                json!("Select an audio interface")
                            } else if !cached_backend.as_ref().is_some_and(|(_, available)| *available) {
                                json!(soundcheck_reason.as_deref().unwrap_or("The connected driver has no available verified soundcheck profile"))
                            } else { Value::Null });
                            o.insert(
                                "input_mode".into(),
                                json!(match soundcheck.mode() {
                                    crate::soundcheck::InputMode::Live => "live",
                                    crate::soundcheck::InputMode::Soundcheck => "soundcheck",
                                    crate::soundcheck::InputMode::RecoveryRequired =>
                                        "recovery_required",
                                }),
                            );
                            if let Some((state, _)) = &cached_backend {
                                o.insert("input_mode".into(), json!(state.input_mode));
                                if client.is_none() && pending.is_none() {
                                    o.insert("transport".into(), json!(state.transport));
                                    if state.input_mode == "soundcheck" {
                                        if let Some(telemetry) =
                                            playback_telemetry.as_ref().and_then(Value::as_object)
                                        {
                                            for (key, value) in telemetry {
                                                o.insert(key.clone(), value.clone());
                                            }
                                        }
                                    }
                                }
                            }
                            if quarantined {
                                o.insert("input_mode".into(), json!("recovery_required"));
                                o.insert(
                                    "transport".into(),
                                    json!(if quarantine_handle
                                        .as_ref()
                                        .is_some_and(|h| h.output_terminated())
                                    {
                                        "stopped"
                                    } else {
                                        "unknown"
                                    }),
                                );
                                o.insert(
                                    "output_termination_confirmed".into(),
                                    json!(quarantine_handle
                                        .as_ref()
                                        .is_some_and(|h| h.output_terminated())),
                                );
                            }
                            o.insert("soundcheck_pending".into(), json!(backend_task.is_some()));
                            o.insert("soundcheck_overdue".into(), json!(backend_overdue));
                            o.insert("disk_free_bytes".into(), Value::Null);
                            o.insert(
                                "capabilities".into(),
                                json!({"recording":helper.as_ref().is_some_and(|p| p.is_file()),"soundcheck":cached_backend.as_ref().is_some_and(|(_, available)| *available), "play": backend.is_some(), "pause": backend.is_some(), "seek": backend.is_some(), "loop": backend.is_some()}),
                            );
                        }
                        v
                    }
                    "devices" | "refresh_devices" => {
                        if let Some(rx) = &scan {
                            match rx.try_recv() {
                                Ok(Ok(found)) => {
                                    devices = found;
                                    scan = None;
                                    scan_error = None;
                                }
                                Ok(Err(e)) => {
                                    scan = None;
                                    scan_error = Some(e);
                                }
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    scan = None;
                                    scan_error = Some("device scan failed".into());
                                }
                                Err(mpsc::TryRecvError::Empty) => {}
                            }
                        }
                        if scan.is_none() && (!scan_started || request["op"] == "refresh_devices") {
                            let (tx, rx) = mpsc::channel();
                            let scan_root = root.clone();
                            std::thread::spawn(move || {
                                let result = openfader_audio::RecorderService::new(scan_root)
                                    .and_then(|service| service.devices())
                                    .map_err(|e| e.to_string());
                                let _ = tx.send(result);
                            });
                            scan_started = true;
                            scan = Some(rx);
                            scan_error = None;
                        }
                        json!({"devices":devices, "scan_pending":scan.is_some(), "scan_error":scan_error})
                    }
                    "takes" => match s.takes() {
                        Ok(v) => {
                            json!({"takes":v.into_iter().map(|(id,manifest)| json!({"id":id,"manifest":manifest})).collect::<Vec<_>>()})
                        }
                        Err(e) => json!({"takes":[],"error":e.to_string()}),
                    },
                    "command" => {
                        let cmd = &request["command"];
                        let id = cmd["request_id"].as_str().unwrap_or("");
                        if id.is_empty() || id.len() > 128 {
                            json!({"ok":false,"request_id":cmd["request_id"],"error":"request_id must be a nonempty string of at most 128 bytes"})
                        } else if backend_task.as_ref().is_some_and(|p| p.0 == id) {
                            let p = backend_task.as_mut().unwrap();
                            if p.1 == *cmd {
                                p.2.push(reply);
                                continue;
                            }
                            json!({"ok":false,"request_id":id,"error":"request_id already used for a different command"})
                        } else if let Some((queued, _)) = queued_stops
                            .iter()
                            .find(|(v, _)| v["command"]["request_id"] == id)
                        {
                            if queued["command"] == *cmd {
                                queued_stops.push((request.clone(), reply));
                                continue;
                            }
                            json!({"ok":false,"request_id":id,"error":"request_id already used for a different command"})
                        } else if pending.as_ref().is_some_and(|p| p.0 == id) {
                            let p = pending.as_mut().unwrap();
                            if p.1 == *cmd {
                                p.2.push(reply);
                                continue;
                            }
                            json!({"ok":false,"request_id":id,"error":"request_id already used for a different command"})
                        } else if let Some((old, result)) = requests.get(id) {
                            if old == cmd {
                                result.clone()
                            } else {
                                json!({"ok":false,"request_id":id,"error":"request_id already used for a different command"})
                            }
                        } else {
                            let args = &cmd["args"];
                            let action = cmd["action"].as_str().unwrap_or("");
                            if backend_task.is_some() {
                                if action == "stop" {
                                    if let Some(handle) = &backend_cancel {
                                        handle.cancel();
                                    }
                                    backend_operation = backend_operation
                                        .checked_add(1)
                                        .expect("backend operation id exhausted");
                                    quarantined = true;
                                    quarantine_handle = backend_cancel.clone();
                                    let response = json!({"ok":false,"request_id":id,"error":"cancellation requested; output termination and routing recovery unconfirmed"});
                                    requests.insert(id.to_owned(), (cmd.clone(), response.clone()));
                                    let _ = reply.send(response);
                                    continue;
                                }
                                let error = if action == "start_recording" {
                                    "soundcheck transition active"
                                } else {
                                    "soundcheck operation pending"
                                };
                                let response = json!({"ok":false,"request_id":id,"error":error});
                                requests.insert(id.to_owned(), (cmd.clone(), response.clone()));
                                let _ = reply.send(response);
                                continue;
                            }
                            if quarantined
                                && !(action == "stop"
                                    || (matches!(
                                        action,
                                        "rebind_recovery"
                                            | "return_live"
                                            | "accept_current_routing"
                                    ) && quarantine_handle
                                        .as_ref()
                                        .is_some_and(|h| h.output_terminated())))
                            {
                                let response = json!({"ok":false,"request_id":id,"error":"routing recovery required; wait for backend and confirmed output termination, then rebind recovery and return live"});
                                requests.insert(id.to_owned(), (cmd.clone(), response.clone()));
                                let _ = reply.send(response);
                                continue;
                            }
                            let capture_active = pending.is_some()
                                || client.is_some()
                                || s.status().transport == "recording";
                            if action == "probe_output" {
                                let idle = !capture_active
                                    && !quarantined
                                    && !root.join("routing-journal.json").exists()
                                    && cached_backend
                                        .as_ref()
                                        .is_none_or(|(state, _)| state.input_mode == "live");
                                let response = if !idle || args["diagnostic_ack"] != true {
                                    json!({"ok":false,"request_id":id,"error":"probe requires idle live transport and explicit diagnostic_ack"})
                                } else if let (Some(helper), Some(device)) =
                                    (helper.clone(), selected.clone())
                                {
                                    let outcome = tokio::task::spawn_blocking(move || {
                                        use openfader_audio::process_supervisor::ProcessSupervisor;
                                        let deadline = std::time::Instant::now() + Duration::from_secs(10);
                                        let mut stages = Vec::new();
                                        let mut child = ProcessSupervisor::spawn(helper, ["--probe-output", device.id.as_str(), &device.sample_rate.to_string()])
                                            .map_err(|e| e.to_string())?;
                                        let pid = child.id();
                                        let result = child.wait_for_ready(deadline, 4096, |value| {
                                            stages.push(value.clone());
                                            value["stage"] == "stream_dropped" || value["stage"] == "error"
                                        });
                                        let exit = child.stop(deadline).map_err(|e| e.to_string())?;
                                        Ok::<_, String>(json!({"pid":pid,"stages":stages,"result":result.map_err(|e| e.to_string()),"exit_success":exit.success()}))
                                    }).await;
                                    match outcome {
                                        Ok(Ok(probe)) => {
                                            json!({"ok":true,"request_id":id,"probe":probe})
                                        }
                                        other => {
                                            json!({"ok":false,"request_id":id,"error":format!("probe failed: {other:?}")})
                                        }
                                    }
                                } else {
                                    json!({"ok":false,"request_id":id,"error":"select an interface first"})
                                };
                                requests.insert(id.to_owned(), (cmd.clone(), response.clone()));
                                let _ = reply.send(response);
                                continue;
                            }
                            let dispatch = backend.is_some()
                                && match action {
                                    "enter_soundcheck" | "accept_current_routing" => {
                                        !capture_active
                                    }
                                    "play" | "pause" | "seek" | "set_loop" | "rebind_recovery"
                                    | "return_live" => true,
                                    "stop" => {
                                        quarantined
                                            || !capture_active
                                                && cached_backend.as_ref().is_some_and(
                                                    |(state, _)| state.input_mode != "live",
                                                )
                                    }
                                    _ => false,
                                };
                            if dispatch {
                                backend_operation = backend_operation
                                    .checked_add(1)
                                    .expect("backend operation id exhausted");
                                let mut owned = backend.take().unwrap();
                                if matches!(action, "enter_soundcheck" | "play") {
                                    owned.configure(helper.clone(), selected.clone());
                                }
                                backend_cancel = Some(owned.cancellation_handle());
                                let action = action.to_owned();
                                let args = args.clone();
                                let task = tokio::spawn(async move {
                                    let result = owned.command(&action, &args).await;
                                    (owned, result)
                                });
                                backend_deadline =
                                    Some(tokio::time::Instant::now() + Duration::from_secs(30));
                                backend_task = Some((
                                    id.to_owned(),
                                    cmd.clone(),
                                    vec![reply],
                                    task,
                                    backend_operation,
                                ));
                                continue;
                            }
                            let result = match cmd["action"].as_str().unwrap_or("") {
                                "select_device" => args["device_id"]
                                    .as_str()
                                    .ok_or_else(|| "device_id required".to_string())
                                    .and_then(|id| {
                                        devices.iter().find(|d| d.id == id).cloned().ok_or_else(
                                            || "device is not in the latest scan".to_string(),
                                        )
                                    })
                                    .and_then(|device| {
                                        if pending.is_some() || client.is_some() {
                                            return Err("capture active".into());
                                        }
                                        s.select_discovered_device(device.clone())
                                            .map_err(|e| e.to_string())?;
                                        selected = Some(device);
                                        if let Some(backend) = &mut backend {
                                            backend.configure(helper.clone(), selected.clone());
                                            cached_backend =
                                                Some((backend.state(), backend.available()));
                                            soundcheck_reason = backend.unavailable_reason();
                                        }
                                        Ok(())
                                    }),
                                "start_recording" => {
                                    if cached_backend
                                        .as_ref()
                                        .is_some_and(|(state, _)| state.input_mode != "live")
                                        || root.join("routing-journal.json").exists()
                                    {
                                        Err("routing recovery required before recording".into())
                                    } else if pending.is_some() || client.is_some() {
                                        Err("capture active".into())
                                    } else if let (Some(device), Some(path), Some(take)) = (
                                        selected.clone(),
                                        helper.clone().filter(|p| p.is_file()),
                                        args["take_id"].as_str(),
                                    ) {
                                        generation = generation.wrapping_add(1);
                                        let gen = generation;
                                        let prep = crate::capture_client::CaptureClient::prepare(
                                            path,
                                            device,
                                            root.clone(),
                                            gen,
                                            Instant::now() + Duration::from_secs(10),
                                        );
                                        let take = take.to_owned();
                                        let task = tokio::spawn(async move {
                                            let c = prep.ready().await?;
                                            let v = c.commit(gen, &take).await?;
                                            Ok((c, v["status"].clone()))
                                        });
                                        pending =
                                            Some((id.to_owned(), cmd.clone(), vec![reply], task));
                                        continue;
                                    } else {
                                        Err("selected device, capture helper and take_id required"
                                            .into())
                                    }
                                }
                                "stop" => {
                                    let capture_active = pending.is_some()
                                        || client.is_some()
                                        || s.status().transport == "recording";
                                    generation = generation.wrapping_add(1);
                                    if let Some((id, cmd, replies, task)) = pending.take() {
                                        task.abort();
                                        let response = json!({"ok":false,"request_id":id,"error":"capture cancelled"});
                                        requests.insert(id, (cmd, response.clone()));
                                        for reply in replies {
                                            let _ = reply.send(response.clone());
                                        }
                                    }
                                    if let Some(c) = client.take() {
                                        let result = c
                                            .stop(Instant::now() + Duration::from_secs(2))
                                            .await
                                            .map(|_| ());
                                        capture_status = None;
                                        result
                                    } else if !capture_active
                                        && backend
                                            .as_ref()
                                            .is_some_and(|b| b.state().input_mode != "live")
                                    {
                                        backend.as_mut().unwrap().command("stop", args).await
                                    } else {
                                        s.stop().map_err(|e| e.to_string())
                                    }
                                }
                                "import_take" | "export_take" if client.is_some() || pending.is_some() || s.status().transport != "stopped" => {
                                    Err("stop transport before transferring takes".to_string())
                                }
                                "import_take" | "export_take" => {
                                    let safe = |s: &str| {
                                        !s.is_empty() && !s.starts_with('.') && s.len() <= 128
                                            && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
                                            && !s.ends_with('.')
                                            && !matches!(s.split('.').next().unwrap_or("").to_ascii_uppercase().as_str(), "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9")
                                    };
                                    match (args["take_id"].as_str(), args["transfer_name"].as_str(), args["format"].as_str()) {
                                        (Some(take), Some(name), Some(format)) if safe(take) && safe(name) && !take.eq_ignore_ascii_case("transfers") => {
                                            let transfers = root.join("transfers");
                                            std::fs::create_dir_all(&transfers).map_err(|e| e.to_string()).and_then(|_| {
                                                openfader_audio::transfer::reject_link(&transfers).map_err(|e| e.to_string())?;
                                                let transfer = transfers.join(name);
                                                let take = root.join(take);
                                                match (action, format) {
                                                    ("import_take", "wav") => openfader_audio::transfer::import_wav(&transfer, &take),
                                                    ("import_take", "native") => openfader_audio::transfer::copy_take(&transfer, &take),
                                                    ("import_take", "tracks") => openfader_audio::transfer::import_tracks(&transfer, &take),
                                                    ("export_take", "wav") => openfader_audio::transfer::export_wav(&take, &transfer),
                                                    ("export_take", "native") => openfader_audio::transfer::copy_take(&take, &transfer),
                                                    ("export_take", "tracks") => openfader_audio::transfer::export_tracks(&take, &transfer),
                                                    _ => return Err("format must be wav, native, or tracks".to_string()),
                                                }.map(|_| ()).map_err(|e| e.to_string())
                                            })
                                        }
                                        _ => Err("valid take_id, transfer_name and format required".to_string()),
                                    }
                                }
                                "load_take" => args["take_id"]
                                    .as_str()
                                    .ok_or_else(|| "take_id required".to_string())
                                    .and_then(|id| {
                                        s.load_take(id).map(|_| ()).map_err(|e| e.to_string())
                                    }),
                                "pause" if backend.is_some() => {
                                    backend.as_mut().unwrap().command("pause", args).await
                                }
                                "pause" => s.pause().map_err(|e| e.to_string()),
                                "seek" | "set_loop" if backend.is_some() => {
                                    backend
                                        .as_mut()
                                        .unwrap()
                                        .command(cmd["action"].as_str().unwrap(), args)
                                        .await
                                }
                                "seek" => args["position_frames"]
                                    .as_u64()
                                    .ok_or_else(|| "position_frames required".to_string())
                                    .and_then(|frame| s.seek(frame).map_err(|e| e.to_string())),
                                "set_loop" => {
                                    match (
                                        args["start_frame"].as_u64(),
                                        args["end_frame"].as_u64(),
                                        args["enabled"].as_bool(),
                                    ) {
                                        (Some(start), Some(end), Some(enabled)) => s
                                            .set_loop(start, end, enabled)
                                            .map_err(|e| e.to_string()),
                                        _ => Err("start_frame, end_frame and enabled required"
                                            .to_string()),
                                    }
                                }
                                "rebind_recovery" | "return_live" => {
                                    if let Some(backend) = &mut backend {
                                        backend.command(cmd["action"].as_str().unwrap(), args).await
                                    } else if let Some(engine) = &engine {
                                        match engine.selector_session().await {
                                            Ok(session) => {
                                                let provider = match soundcheck.recovery_keys() {
                                                    Ok(Some(keys)) => Ok(crate::engine_adapter::EngineAdapter::with_snapshot_keys(session, keys)),
                                                    Ok(None) => crate::engine_adapter::EngineAdapter::new(session).await,
                                                    Err(e) => Err(e),
                                                };
                                                match provider {
                                                    Ok(mut provider) => {
                                                        let result = if cmd["action"]
                                                            == "rebind_recovery"
                                                        {
                                                            soundcheck
                                                                .rebind_for_recovery(&mut provider)
                                                                .await
                                                        } else {
                                                            soundcheck
                                                                .return_live(&mut provider)
                                                                .await
                                                        };
                                                        result.map_err(|e| e.to_string())
                                                    }
                                                    Err(e) => Err(e.to_string()),
                                                }
                                            }
                                            Err(e) => Err(e.to_string()),
                                        }
                                    } else {
                                        Err("engine unavailable".into())
                                    }
                                }
                                "enter_soundcheck"
                                    if pending.is_some()
                                        || client.is_some()
                                        || s.status().transport == "recording" =>
                                {
                                    Err("capture active".into())
                                }
                                "enter_soundcheck" | "play" => {
                                    if let Some(backend) = &mut backend {
                                        backend.configure(helper.clone(), selected.clone());
                                        backend.command(cmd["action"].as_str().unwrap(), args).await
                                    } else {
                                        Err("physical routing is not verified".to_string())
                                    }
                                }
                                _ => Err("unknown action".to_string()),
                            };
                            let response = match result {
                                Ok(()) => json!({"ok":true,"request_id":id,"error":null}),
                                Err(e) => json!({"ok":false,"request_id":id,"error":e}),
                            };
                            // Retain for the process lifetime: eviction could repeat a side effect.
                            requests.insert(id.to_owned(), (cmd.clone(), response.clone()));
                            response
                        }
                    }
                    _ => json!({"error":"unknown request"}),
                },
            }
        };
        let _ = reply.send(result);
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn worker(
    _root: PathBuf,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(Value, oneshot::Sender<Value>)>,
    _engine: std::sync::Arc<openfader_engine::Engine>,
    _backend: Option<Box<dyn soundcheck_backend::SoundcheckBackend>>,
) {
    let _ = HashMap::<String, String>::new();
    while let Some((_, reply)) = rx.recv().await {
        let _ = reply.send(json!({"error":"recorder unsupported on this platform"}));
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod tests {
    use super::*;
    #[cfg(unix)]
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    #[cfg(unix)]
    async fn fixture(script: &str) -> (Recorder, PathBuf) {
        fixture_with_backend(script, None).await
    }
    #[cfg(unix)]
    async fn fixture_with_backend(
        script: &str,
        backend: Option<Box<dyn soundcheck_backend::SoundcheckBackend>>,
    ) -> (Recorder, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "openfader-worker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        std::fs::write(&helper, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let device = openfader_audio::DeviceInfo {
            id: "fixture".into(),
            name: "fixture".into(),
            input_channels: 18,
            output_channels: 18,
            sample_rate: 48000,
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(worker_with_config(
            root.clone(),
            rx,
            Some((helper, device)),
            None,
            backend,
        ));
        (Recorder { tx }, root)
    }
    #[cfg(unix)]
    fn command(id: &str, action: &str) -> Value {
        json!({"op":"command","command":{"request_id":id,"action":action,"args":{"take_id":"test"}}})
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn recording_does_not_require_a_soundcheck_profile() {
        struct Unsupported;
        impl soundcheck_backend::SoundcheckBackend for Unsupported {
            fn state(&self) -> soundcheck_backend::SoundcheckState {
                soundcheck_backend::SoundcheckState {
                    input_mode: "live".into(),
                    transport: "stopped".into(),
                }
            }
            fn unavailable_reason(&self) -> Option<String> {
                Some("driver signal path is unverified".into())
            }
            fn command<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>
            {
                Box::pin(async { Err("driver signal path is unverified".into()) })
            }
        }
        let (recorder, root) = fixture_with_backend(
            r#"read line
printf '{"state":"READY","generation":1}\n'
while read line; do
 case "$line" in
 *commit*) printf '{"state":"RECORDING","generation":1,"status":{"transport":"recording"}}\n';;
 *status*) printf '{"state":"STATUS","generation":1,"status":{"transport":"recording"}}\n';;
 *stop*) printf '{"state":"STOPPED","generation":1,"status":{}}\n'; exit 0;;
 esac
done"#,
            Some(Box::new(Unsupported)),
        )
        .await;
        let state = recorder.request(json!({"op":"state"})).await;
        assert_eq!(state["capabilities"]["recording"], true);
        assert_eq!(state["capabilities"]["soundcheck"], false);
        assert_eq!(
            state["soundcheck_reason"],
            "driver signal path is unverified"
        );
        assert_eq!(
            recorder.request(command("record", "start_recording")).await["ok"],
            true
        );
        assert_eq!(recorder.request(command("stop", "stop")).await["ok"], true);
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn probe_requires_ack_and_idle_and_returns_stages() {
        let (recorder, root) = fixture("echo '{\"stage\":\"stream_dropped\"}'").await;
        let probe = |id, ack| json!({"op":"command","command":{"request_id":id,"action":"probe_output","args":{"diagnostic_ack":ack}}});
        let denied = recorder.request(probe("no-ack", false)).await;
        assert_eq!(denied["ok"], false);
        std::fs::write(root.join("routing-journal.json"), "{}").unwrap();
        let busy = recorder.request(probe("journal", true)).await;
        assert_eq!(busy["ok"], false);
        std::fs::remove_file(root.join("routing-journal.json")).unwrap();
        let result = recorder.request(probe("allowed", true)).await;
        assert_eq!(result["ok"], true);
        assert_eq!(result["probe"]["stages"][0]["stage"], "stream_dropped");
        assert_eq!(result["probe"]["result"]["Ok"]["stage"], "stream_dropped");
        assert!(result["probe"]["pid"].is_u64());
        let _ = std::fs::remove_dir_all(root);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn pending_duplicate_and_stop_cancel_preparation() {
        let (recorder, root) =
            fixture("echo $$ >> \"$(dirname \"$0\")/pids\"; read line; exec sleep 30").await;
        // The helper's own directory is the test root; wait for it to start.
        let start = tokio::spawn({
            let r = recorder.clone();
            async move { r.request(command("start", "start_recording")).await }
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("pids").exists() {
            assert!(Instant::now() < deadline, "helper did not start");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let busy = recorder.request(command("busy", "enter_soundcheck")).await;
        assert_eq!(busy["error"], "capture active");
        let duplicate = tokio::spawn({
            let r = recorder.clone();
            async move { r.request(command("start", "start_recording")).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let stopped = tokio::time::timeout(
            Duration::from_secs(2),
            recorder.request(command("stop", "stop")),
        )
        .await
        .unwrap();
        assert_eq!(stopped["ok"], true);
        let first = start.await.unwrap();
        assert_eq!(first["error"], "capture cancelled");
        assert_eq!(duplicate.await.unwrap(), first);
        assert_eq!(
            recorder.request(command("start", "start_recording")).await,
            first
        );
        let pids = std::fs::read_to_string(root.join("pids")).unwrap();
        assert_eq!(pids.lines().count(), 1);
        let pid = pids.trim();
        let deadline = Instant::now() + Duration::from_secs(2);
        while std::process::Command::new("kill")
            .args(["-0", pid])
            .output()
            .unwrap()
            .status
            .success()
        {
            assert!(Instant::now() < deadline, "helper was not reaped");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(std::fs::read_dir(&root)
            .unwrap()
            .all(|e| matches!(e.unwrap().file_name().to_str(), Some("helper" | "pids"))));
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn worker_ready_status_stop_and_crash_recovery() {
        let (recorder, root) = fixture(
            r#"read line
printf '{"state":"READY","generation":1}\n'
while read line; do
 case "$line" in
 *commit*) printf '{"state":"RECORDING","generation":1,"status":{"transport":"recording"}}\n';;
 *status*) printf '{"state":"STATUS","generation":1,"status":{"transport":"recording"}}\n';;
 *stop*) printf '{"state":"STOPPED","generation":1,"status":{}}\n'; exit 0;;
 esac
done"#,
        )
        .await;
        assert_eq!(
            recorder.request(command("start", "start_recording")).await["ok"],
            true
        );
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["transport"],
            "recording"
        );
        assert_eq!(recorder.request(command("stop", "stop")).await["ok"], true);
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
        let (recorder, root) = fixture("exit 1").await;
        let failed = recorder.request(command("start", "start_recording")).await;
        assert_eq!(failed["ok"], false);
        assert!(recorder.request(json!({"op":"state"})).await["capture_error"].is_string());
        assert!(failed["error"].as_str().unwrap().contains("capture failed"));
        assert_eq!(recorder.request(command("stop", "stop")).await["ok"], true);
        assert!(recorder.request(json!({"op":"state"})).await["available"]
            .as_bool()
            .unwrap());
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn state_reports_playback_telemetry() {
        struct TelemetryBackend {
            count: u64,
        }
        impl soundcheck_backend::SoundcheckBackend for TelemetryBackend {
            fn state(&self) -> soundcheck_backend::SoundcheckState {
                soundcheck_backend::SoundcheckState {
                    input_mode: "soundcheck".into(),
                    transport: "playing".into(),
                }
            }
            fn telemetry(
                &mut self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Value>> + Send + '_>>
            {
                Box::pin(async move {
                    self.count += 1;
                    let transport = match self.count {
                        1 | 2 => "playing",
                        3 => "paused",
                        _ => "stopped",
                    };
                    Some(
                        json!({"position_frames": self.count * 48000, "sample_rate": 48000, "transport": transport}),
                    )
                })
            }
            fn command<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>
            {
                Box::pin(async { Ok(()) })
            }
        }
        let (recorder, root) =
            fixture_with_backend("exit 1", Some(Box::new(TelemetryBackend { count: 0 }))).await;
        for (frame, transport) in [
            (48000, "playing"),
            (96000, "playing"),
            (144000, "paused"),
            (192000, "stopped"),
        ] {
            let state = recorder.request(json!({"op":"state"})).await;
            assert_eq!(state["position_frames"], frame);
            assert_eq!(state["sample_rate"], 48000);
            assert_eq!(state["transport"], transport);
            assert_eq!(state["input_mode"], "soundcheck");
        }
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    struct FakeBackend {
        calls: std::sync::Arc<std::sync::Mutex<Vec<(String, Value)>>>,
        mode: String,
        transport: String,
    }
    impl soundcheck_backend::SoundcheckBackend for FakeBackend {
        fn state(&self) -> soundcheck_backend::SoundcheckState {
            soundcheck_backend::SoundcheckState {
                input_mode: self.mode.clone(),
                transport: self.transport.clone(),
            }
        }
        fn command<'a>(
            &'a mut self,
            action: &'a str,
            args: &'a Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push((action.to_owned(), args.clone()));
                match action {
                    "enter_soundcheck" => {
                        self.mode = "soundcheck".into();
                        self.transport = "playing".into();
                    }
                    "stop" => self.transport = "stopped".into(),
                    "return_live" => self.mode = "live".into(),
                    _ => {}
                }
                Ok(())
            })
        }
    }
    #[cfg(unix)]
    struct BlockingBackend {
        cancellation: soundcheck_backend::CancellationHandle,
        entered: std::sync::Arc<tokio::sync::Notify>,
        release: std::sync::Arc<tokio::sync::Notify>,
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        mode: String,
    }
    #[cfg(unix)]
    impl soundcheck_backend::SoundcheckBackend for BlockingBackend {
        fn cancellation_handle(&self) -> soundcheck_backend::CancellationHandle {
            if self.cancellation.output_terminated() {
                soundcheck_backend::CancellationHandle::default()
            } else {
                self.cancellation.clone()
            }
        }
        fn state(&self) -> soundcheck_backend::SoundcheckState {
            soundcheck_backend::SoundcheckState {
                input_mode: self.mode.clone(),
                transport: if self.mode == "soundcheck" {
                    "playing"
                } else {
                    "stopped"
                }
                .into(),
            }
        }
        fn command<'a>(
            &'a mut self,
            action: &'a str,
            _: &'a Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls.lock().unwrap().push(action.to_owned());
                if action == "enter_soundcheck" {
                    self.entered.notify_one();
                    self.release.notified().await;
                    self.mode = "soundcheck".into();
                } else if action == "return_live" {
                    self.mode = "live".into();
                }
                Ok(())
            })
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn backend_transition_keeps_actor_responsive_and_queues_stop() {
        let entered = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let cancellation = soundcheck_backend::CancellationHandle::default();
        let (recorder, root) = fixture_with_backend(
            r#"read line
printf '{"state":"READY","generation":1}\n'
while read line; do
 case "$line" in
 *commit*) printf '{"state":"RECORDING","generation":1,"status":{"transport":"recording"}}\n';;
 *status*) printf '{"state":"STATUS","generation":1,"status":{"transport":"recording"}}\n';;
 *stop*) printf '{"state":"STOPPED","generation":1,"status":{}}\n'; exit 0;;
 esac
done"#,
            Some(Box::new(BlockingBackend {
                cancellation: cancellation.clone(),
                entered: entered.clone(),
                release: release.clone(),
                calls: calls.clone(),
                mode: "live".into(),
            })),
        )
        .await;
        let first = tokio::spawn({
            let r = recorder.clone();
            async move { r.request(command("enter", "enter_soundcheck")).await }
        });
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        let duplicate = tokio::spawn({
            let r = recorder.clone();
            async move { r.request(command("enter", "enter_soundcheck")).await }
        });
        let state = tokio::time::timeout(
            Duration::from_secs(2),
            recorder.request(json!({"op":"state"})),
        )
        .await
        .unwrap();
        assert_eq!(state["soundcheck_pending"], true);
        assert_eq!(
            recorder
                .request(command("capture", "start_recording"))
                .await["ok"],
            false
        );
        let stop = tokio::spawn({
            let r = recorder.clone();
            async move { r.request(command("stop", "stop")).await }
        });
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), stop)
                .await
                .unwrap()
                .unwrap()["ok"],
            false
        );
        assert!(cancellation.cancellation_requested());
        release.notify_one();
        let first_result = first.await.unwrap();
        assert_eq!(first_result, duplicate.await.unwrap());
        assert_eq!(first_result["ok"], false);
        assert_eq!(*calls.lock().unwrap(), vec!["enter_soundcheck"]);
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["input_mode"],
            "recovery_required"
        );
        assert_eq!(
            recorder.request(command("again", "enter_soundcheck")).await["ok"],
            false
        );
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["transport"],
            "unknown"
        );
        assert_eq!(
            recorder
                .request(command("blocked-rebind", "rebind_recovery"))
                .await["ok"],
            false
        );
        assert_eq!(
            recorder
                .request(command("blocked-capture", "start_recording"))
                .await["ok"],
            false
        );
        cancellation
            .terminated
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            recorder.request(command("rebind", "rebind_recovery")).await["ok"],
            true
        );
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["input_mode"],
            "recovery_required"
        );
        assert_eq!(
            recorder.request(command("live", "return_live")).await["ok"],
            true
        );
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["input_mode"],
            "live"
        );
        assert!(!root.join("routing-journal.json").exists());
        assert_eq!(
            recorder
                .request(command("record-after-recovery", "start_recording"))
                .await["ok"],
            true
        );
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["transport"],
            "recording"
        );
        assert_eq!(
            recorder.request(command("stop-recording", "stop")).await["ok"],
            true
        );
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    #[tokio::test]
    async fn injected_backend_receives_recorder_commands() {
        let root = std::env::temp_dir().join(format!("openfader-backend-{}", std::process::id()));
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let backend = FakeBackend {
            calls: calls.clone(),
            mode: "live".into(),
            transport: "stopped".into(),
        };
        let engine = openfader_engine::Engine::new(openfader_engine::EngineConfig::default());
        let recorder = Recorder::start_with_backend(root.clone(), engine, Some(Box::new(backend)));
        for (id, action, args) in [
            ("1", "enter_soundcheck", json!({"take_id":"take"})),
            ("2", "pause", json!({})),
            ("3", "seek", json!({"position_frames":42})),
            (
                "4",
                "set_loop",
                json!({"start_frame":1,"end_frame":9,"enabled":true}),
            ),
            ("5", "stop", json!({})),
        ] {
            assert_eq!(recorder.request(json!({"op":"command","command":{"request_id":id,"action":action,"args":args}})).await["ok"], true);
        }
        let state = recorder.request(json!({"op":"state"})).await;
        assert_eq!(state["input_mode"], "soundcheck");
        assert_eq!(state["transport"], "stopped");
        assert_eq!(recorder.request(json!({"op":"command","command":{"request_id":"6","action":"return_live","args":{}}})).await["ok"], true);
        assert_eq!(
            recorder.request(json!({"op":"state"})).await["input_mode"],
            "live"
        );
        let log = calls.lock().unwrap();
        assert_eq!(
            log.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            [
                "enter_soundcheck",
                "pause",
                "seek",
                "set_loop",
                "stop",
                "return_live"
            ]
        );
        assert_eq!(log[2].1["position_frames"], 42);
        assert_eq!(
            log[3].1,
            json!({"start_frame":1,"end_frame":9,"enabled":true})
        );
        drop(log);
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
    #[tokio::test]
    async fn serialized_commands_replay_and_reject_reused_ids() {
        let root = std::env::temp_dir().join(format!(
            "openfader-recorder-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(worker_with_config(root.clone(), rx, None, None, None));
        let recorder = Recorder { tx };
        let cmd =
            json!({"op":"command","command":{"request_id":"test-1","action":"stop","args":{}}});
        let first = recorder.request(cmd.clone()).await;
        assert_eq!(first["ok"], true);
        assert_eq!(recorder.request(cmd).await, first);
        let duplicate = recorder.request(json!({"op":"command","command":{"request_id":"test-1","action":"enter_soundcheck","args":{}}})).await;
        assert_eq!(duplicate["ok"], false);
        assert!(duplicate["error"].as_str().unwrap().contains("different"));
        let blocked = recorder.request(json!({"op":"command","command":{"request_id":"test-2","action":"enter_soundcheck","args":{}}})).await;
        assert_eq!(blocked["ok"], false);
        drop(recorder);
        let _ = std::fs::remove_dir_all(root);
    }
}
