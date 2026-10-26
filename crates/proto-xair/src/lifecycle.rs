//! Connection lifecycle and mixer identity.
use super::*;

impl XAirDriver {
    /// Connect-time port sweep for a bare host (no `:port`): ping `/info`
    /// on 10024, then the 10023 fallback, each bounded by
    /// [`addrs::CONNECT_PROBE_TIMEOUT`](crate::addrs::CONNECT_PROBE_TIMEOUT);
    /// dial the first responder, preferring 10024. If neither answers,
    /// dial 10024 anyway — venue wifi may drop the ping but pass the
    /// session (same best-effort posture as the old unconditional dial).
    pub(super) async fn sweep_peer(host: &str) -> anyhow::Result<SocketAddr> {
        use openfader_transport::session::resolve_peer;
        if probe_port(host, addrs::PORT).await {
            return resolve_peer(host, addrs::PORT);
        }
        if probe_port(host, addrs::FALLBACK_PORT).await {
            return resolve_peer(host, addrs::FALLBACK_PORT);
        }
        resolve_peer(host, addrs::PORT)
    }

    /// Map a raw `/info` reply to [`MixerInfo`]. Hardware order (baseline
    /// §1, XR18 FW 1.20 `/info` → `[V0.04, Studio, XR18, 1.20]`):
    /// index 1 = name, 2 = model, 3 = firmware (index 0 is the protocol
    /// revision). Short replies yield empty fields rather than guesses.
    pub(super) fn info_from_strings(strings: Vec<String>, peer: String) -> MixerInfo {
        MixerInfo {
            name: strings.get(1).cloned().unwrap_or_default(),
            model: strings.get(2).cloned().unwrap_or_default(),
            firmware: strings.get(3).cloned().unwrap_or_default(),
            addr: peer,
        }
    }

    pub(super) async fn driver_connect(&self, addr: &str) -> anyhow::Result<()> {
        // Connect-time port sweep: bare hosts try 10024 then the 10023
        // fallback (sub-second probes each); an explicit `:port` is dialed
        // as-is with no sweep. Neither path touches the steady-state 2 s
        // query timeout.
        let peer = if has_explicit_port(addr) {
            openfader_transport::session::resolve_peer(addr, addrs::PORT)?
        } else {
            Self::sweep_peer(addr).await?
        };
        let session =
            std::sync::Arc::new(OscSession::connect(peer, Some(KeepaliveConfig::xremote())).await?);
        // Authoritative discovery ping: a UDP "connect" always succeeds, so
        // without this a silent desk would link as connected and then time
        // out every query (phantom link, tap-to-connect looks dead with no
        // message). Two attempts absorb one lost datagram on flaky venue
        // wifi; a desk that answers nothing fails fast with the address in
        // the message so the UI can offer a retry. Attempts use the
        // connect-burst budget (not the steady-state 2 s query timeout).
        let mut verified = false;
        for _ in 0..2 {
            if session
                .query_timeout(
                    addrs::INFO,
                    openfader_transport::session::CONNECT_BURST_TIMEOUT,
                )
                .await
                .is_ok()
            {
                verified = true;
                break;
            }
        }
        if !verified {
            anyhow::bail!(
                "no reply from {peer} — check the desk is powered, on this network, and the IP is correct"
            );
        }
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
        let connection = self
            .connection_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        *self.receive_since.lock().unwrap() = std::time::Instant::now();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        // Desk-broadcast ingest: parse every unmatched inbound message into
        // the stable update channel (one task per connection; it exits when
        // the next connect drops this session). `/meters/6` ALL DYN blobs
        // ride along here too (the transport hub only forwards levels, so
        // the session files DYN blobs as unmatched traffic): route them
        // into the GR cache backing the `gate_gr` / `comp_gr` param reads.
        let mut rx = session.subscribe_received_updates();
        let tx = self.update_tx.clone();
        let dyn_cache = self.dyn_cache.clone();
        let receive_since = self.receive_since.clone();
        let generation = self.connection_generation.clone();
        let task = tokio::spawn(async move {
            let mut fx_types = [None; 4];
            loop {
                match rx.recv().await {
                    Ok((received_at, msg)) => {
                        if received_at < *receive_since.lock().unwrap() {
                            continue;
                        }
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                            break;
                        }
                        if msg.addr == addrs::METER_DYN_SUBSCRIBE {
                            if let Ok(dyn_m) = Self::decode_dyn_message(&msg) {
                                *dyn_cache.lock().await = Some((received_at, dyn_m));
                            }
                            continue;
                        }
                        if let Some(rest) = msg.addr.strip_prefix("/fx/") {
                            if let Some((slot, leaf)) = rest.split_once('/') {
                                if let Ok(slot @ 1..=4) = slot.parse::<u16>() {
                                    let type_args = if leaf == "type" {
                                        Some(msg.args.as_slice())
                                    } else {
                                        None
                                    };
                                    if let Some(args) = type_args {
                                        if let Ok(index) = fx::parse_fx_type(args) {
                                            fx_types[slot as usize - 1] = Some(index);
                                            let _ = tx.send(openfader_core::ParamUpdate {
                                                received_at,
                                                strip: StripId(slot),
                                                param: "_fx_type".into(),
                                                value: ParamValue::Float(index as f32),
                                            });
                                        }
                                    } else if let Some(index) = leaf
                                        .strip_prefix("par/")
                                        .and_then(|n| n.parse::<u8>().ok())
                                        .filter(|n| (1..=64).contains(n))
                                    {
                                        if let Some(effect) = fx_types[slot as usize - 1] {
                                            if let Some(def) = fx::fx_param_def(effect, index) {
                                                if let Ok(value) =
                                                    fx::parse_fx_param(&msg.args, Some(&def))
                                                {
                                                    let _ = tx.send(openfader_core::ParamUpdate {
                                                        received_at,
                                                        strip: StripId(slot),
                                                        param: format!("_fx_par{index:02}"),
                                                        value: ParamValue::Float(value),
                                                    });
                                                }
                                            }
                                        } else {
                                            let _ = tx.send(openfader_core::ParamUpdate {
                                                received_at,
                                                strip: StripId(slot),
                                                param: "_fx_dirty".into(),
                                                value: ParamValue::Bool(true),
                                            });
                                        }
                                    } else if leaf == "par" {
                                        if let Some(effect) = fx_types[slot as usize - 1] {
                                            for (i, arg) in msg.args.iter().take(64).enumerate() {
                                                if let Some(def) =
                                                    fx::fx_param_def(effect, i as u8 + 1)
                                                {
                                                    if let Ok(value) = fx::parse_fx_param(
                                                        std::slice::from_ref(arg),
                                                        Some(&def),
                                                    ) {
                                                        let _ =
                                                            tx.send(openfader_core::ParamUpdate {
                                                                received_at,
                                                                strip: StripId(slot),
                                                                param: format!(
                                                                    "_fx_par{:02}",
                                                                    i + 1
                                                                ),
                                                                value: ParamValue::Float(value),
                                                            });
                                                    }
                                                }
                                            }
                                        }
                                    } else if leaf.is_empty() {
                                        // No positional guessing for unknown FX containers.
                                    }
                                }
                            } else if let Ok(slot @ 1..=4) = rest.parse::<u16>() {
                                if let Ok(index) = fx::parse_fx_type(&msg.args) {
                                    fx_types[slot as usize - 1] = Some(index);
                                    let _ = tx.send(openfader_core::ParamUpdate {
                                        received_at,
                                        strip: StripId(slot),
                                        param: "_fx_type".into(),
                                        value: ParamValue::Float(index as f32),
                                    });
                                }
                            }
                            continue;
                        }
                        if msg.addr == "/node" {
                            // Display-unit node text is never decoded as normalized
                            // OSC. Re-read a captured typed container at normal budget.
                            if let Some(OscType::String(text)) = msg.args.first() {
                                for line in text.lines().take(256) {
                                    let Some(path) = line.split_whitespace().next() else {
                                        continue;
                                    };
                                    let route = XAIR_TOPOLOGY
                                        .iter()
                                        .flat_map(|f| f.first..f.first + f.count)
                                        .find_map(|i| {
                                            let (_, root) = strip_root(i as u16).ok()?;
                                            if path == root {
                                                return Some((i as u16, None));
                                            }
                                            path.strip_prefix(&(root + "/"))
                                                .map(|suffix| (i as u16, Some(suffix.to_string())))
                                        });
                                    let (strip, value) = match route {
                                        Some((strip, Some(suffix))) => {
                                            (strip, ParamValue::Text(suffix))
                                        }
                                        Some((strip, None)) => (strip, ParamValue::Bool(true)),
                                        None if path.starts_with("/fx/") => {
                                            let _ = tx.send(openfader_core::ParamUpdate {
                                                received_at,
                                                strip: StripId(0),
                                                param: "_fx_dirty".into(),
                                                value: ParamValue::Bool(true),
                                            });
                                            continue;
                                        }
                                        None => (u16::MAX, ParamValue::Bool(true)),
                                    };
                                    let _ = tx.send(openfader_core::ParamUpdate {
                                        received_at,
                                        strip: StripId(strip),
                                        param: "_sync_dirty".into(),
                                        value,
                                    });
                                }
                            }
                            continue;
                        }
                        for (strip, param, value) in parse_inbound(&msg) {
                            let _ = tx.send(openfader_core::ParamUpdate {
                                received_at,
                                strip: StripId(strip),
                                param: param.to_string(),
                                value,
                            });
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!(
                            "xair: control receive lagged {n}; requesting bounded reconciliation"
                        );
                        let _ = tx.send(openfader_core::ParamUpdate {
                            received_at: std::time::Instant::now(),
                            strip: StripId(u16::MAX),
                            param: "_sync_dirty".into(),
                            value: ParamValue::Bool(true),
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        let old_rta = self.rta.lock().unwrap().take();
        if let Some(old) = old_rta {
            let _ = old.target("", None, true).await;
        }
        // Same OSC session as the selector: XR18 meter provenance is session-bound.
        // The actor runs concurrently; no RTA round trip blocks hydration.
        let stream = std::sync::Arc::new(rta::RtaStream::start(session.clone(), connection));
        *self.rta.lock().unwrap() = Some(stream);
        *self.session.lock().await = Some(session);
        *self.peer.lock().await = peer.to_string();
        Ok(())
    }

    pub(super) async fn driver_disconnect(&self) -> anyhow::Result<()> {
        self.connection_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let old_rta = self.rta.lock().unwrap().take();
        if let Some(old) = old_rta {
            let _ = old.target("", None, true).await;
        }
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
        self.session.lock().await.take();
        self.peer.lock().await.clear();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        Ok(())
    }

    pub(super) async fn driver_query_info(&self) -> anyhow::Result<MixerInfo> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::INFO).await?;
        let peer = self.peer.lock().await.clone();
        Ok(Self::info_from_strings(osc::string_args(&reply), peer))
    }
}
