//! Session readiness, generation changes and broadcast ingest lifecycle.
use crate::addrs::dl32s_inputs;
use crate::input::decode_push;
use crate::native::NativeCache;
use crate::preamp::{affects_preamp_binding, routed_preamp_updates};
use crate::telemetry::{
    decode_dynamics_meters, decode_input_meters, decode_rta_bins, rta_selector_from_push,
    RTA_SELECTOR_ADDR,
};
use crate::{addrs, Dl32sDriver};
use openfader_transport::mackie::MackieSession;
use std::sync::{atomic::Ordering, Arc};
use tokio::sync::{broadcast, MutexGuard};

/// Bulk-state readiness bound: prior hardware runs saw the whole-space
/// push settle tens of seconds after init on a slow path, so 60 s keeps
/// headroom without hanging `Cmd::Connect` forever.
const INITIAL_STATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Wait until the mixer's initial bulk push has populated the session
/// cache. Sentinel choice is deliberate: CH1 and CH32 mute+fader are
/// verified input leaves at opposite ends of the bulk, so their presence
/// proves both chunks landed; the main fader word proves the master
/// block did (its field domain is unverified, but the word itself is
/// expected in the whole-space push). Returns an `awaiting initial mixer
/// state` error on timeout — never TCP-success with zero audio data.
async fn wait_for_initial_state(session: &MackieSession) -> anyhow::Result<()> {
    let table = dl32s_inputs();
    let sentinels: [(&str, u32); 5] = [
        ("ch1 mute", table.address(1, "mute").expect("table static")),
        (
            "ch1 fader",
            table.address(1, "fader").expect("table static"),
        ),
        (
            "ch32 mute",
            table.address(32, "mute").expect("table static"),
        ),
        (
            "ch32 fader",
            table.address(32, "fader").expect("table static"),
        ),
        (
            "main fader",
            addrs::dl32s_main()
                .address(1, "fader")
                .expect("table static"),
        ),
    ];
    let deadline = std::time::Instant::now() + INITIAL_STATE_TIMEOUT;
    loop {
        // Liveness gate (r50): check BEFORE the sentinel-success test so a
        // closed transport can never report ready from stale cache words.
        if !session.is_alive() {
            anyhow::bail!(
                "dl32s: transport closed before initial mixer state arrived; reconnect once the mixer is reachable"
            );
        }
        if sentinels
            .iter()
            .all(|(_, addr)| session.cached(*addr).is_some())
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let missing: Vec<String> = sentinels
                .iter()
                .filter(|(_, addr)| session.cached(*addr).is_none())
                .map(|(name, addr)| format!("{name} ({addr})"))
                .collect();
            anyhow::bail!(
                "dl32s: awaiting initial mixer state: no bulk push for {} after {:?}; reconnect once the mixer is broadcasting",
                missing.join(", "),
                INITIAL_STATE_TIMEOUT
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

impl Dl32sDriver {
    /// A correlated, read-only device-name query on the existing control TCP
    /// session. A cached main-fader word and a writable TCP buffer cannot
    /// detect a silent/half-open Wi-Fi route. Uses the same captured request
    /// as identification; it never initializes or writes mixer state.
    pub async fn check_control_health(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .expect("checked above")
            .request_general_info(3, openfader_transport::mackie::REQUEST_TIMEOUT)
            .await?;
        anyhow::ensure!(
            reply.body.starts_with(&3u32.to_be_bytes()) && reply.body.len() >= 8,
            "dl32s: invalid control health reply"
        );
        Ok(())
    }

    pub(super) async fn session(&self) -> anyhow::Result<MutexGuard<'_, Option<MackieSession>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        // Liveness gate (r50): a stored session whose transport tasks have
        // finished is a closed TCP connection, not a live desk. Report it as
        // a transport closure (wording avoids the engine's drop-class
        // substrings) so callers reconnect instead of serving stale cache.
        if !guard.as_ref().expect("checked above").is_alive() {
            anyhow::bail!(
                "dl32s: transport closed: connection lost; reconnect once the mixer is reachable"
            );
        }
        Ok(guard)
    }
}
impl Dl32sDriver {
    pub(super) async fn lifecycle_connect(&self, addr: &str) -> anyhow::Result<()> {
        addrs::model_status("dl32s")?;
        let session = MackieSession::connect(addr).await?;
        // Readiness gate: the wire init handshake is best-effort and
        // returns once the TCP exchange completes, but the live value
        // space only lands in the push cache as the mixer broadcasts it
        // (bulk chunks arrive seconds later on a slow path). Exposing the
        // session before that serves fabricated snapshot defaults as real
        // state, so wait for the sentinel words before publishing. On
        // timeout the session is closed (transport tasks abort on close /
        // drop) and no ingest task is spawned, so nothing leaks.
        if let Err(e) = wait_for_initial_state(&session).await {
            session.close();
            return Err(e);
        }
        // Desk-broadcast ingest: decode every pushed frame into the stable
        // update channel (one task per connection; it exits when the next
        // connect bumps the generation or the session drops). Mirrors the
        // X-Air session->driver forward path. Type-1 meter blocks ride a
        // separate telemetry branch; `decode_push` never treats them as params.
        // The per-client 301-entry layout preserves the input/RTA prefix
        // (input0..31, RTA62..92), then appends six dynamics words per
        // input and 16 output GR words.
        // A refused override leaves extended telemetry unavailable while
        // the default input stream can continue.
        if let Err(e) = session
            .request_meter_layout(&openfader_transport::mackie::dl32s_output_meter_addresses())
            .await
        {
            eprintln!("dl32s: output GR/dynamics/RTA meter layout override failed (input meters continue, extended telemetry unavailable): {e:#}");
        }
        // Liveness recheck (r50): the layout override above awaited two
        // correlated round trips. Recheck BEFORE publishing the session or
        // spawning ingest so a transport that died mid-await never becomes
        // the stored session. Cleanup mirrors the readiness-gate failure:
        // close the local session (aborts transport tasks on close/drop),
        // spawn nothing, assign nothing.
        if !session.is_alive() {
            session.close();
            anyhow::bail!(
                "dl32s: transport closed before initial mixer state arrived; reconnect once the mixer is reachable"
            );
        }
        // Retire the previous session's ingest/cache only after all
        // fallible readiness checks succeed. A failed replacement must
        // leave the retained session's generation and telemetry intact.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut m) = self.meters.lock() {
            m.generation = generation;
            m.updated_at = None;
            m.levels.clear();
            m.outputs_at = None;
            m.outputs.clear();
            m.dynamics_at = None;
            m.dynamics.clear();
        }
        // Initial shared-source read: the rig-confirmed initial bulk carries
        // the type-2 addr-22 selector (5000 = Return 1–2). Treated as a
        // source change (fresh settle window) so early frames are never
        // misattributed.
        // Subscribe before sampling the cache so a selector update cannot
        // land between the initial read and the live subscription.
        let mut rx = session.subscribe_pushes();
        let initial_selector = session.cached_type2(RTA_SELECTOR_ADDR);
        if let Ok(mut s) = self.spectrum.lock() {
            s.generation = generation;
            s.selector = initial_selector;
            s.epoch = generation << 32;
            s.switched_at = Some(std::time::Instant::now());
            s.updated_at = None;
            s.bins_db.clear();
        }
        let tx = self.update_tx.clone();
        let routed_session = Arc::downgrade(&self.session);
        let gen = self.generation.clone();
        let meters = self.meters.clone();
        let spectrum = self.spectrum.clone();
        // Seed the read-only native snapshot view from the session's
        // push-only verified stores (covers snapshot pushes that arrived
        // before this subscription; the broadcast channel buffers anything
        // newer until the task below drains it). Verified stores only —
        // own-save optimism never seeds this view.
        let native = self.native.clone();
        if let Ok(mut n) = native.lock() {
            n.seed(generation, &session);
        }
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(push) => {
                        if gen.load(Ordering::Acquire) != generation {
                            break;
                        }
                        // Passive shared-source tracking (no writes): a
                        // type-2 push covering addr 22 reselects the source —
                        // clear the cache, bump the epoch, restart the
                        // settle window.
                        if let Some(sel) = rta_selector_from_push(&push) {
                            if let Ok(mut s) = spectrum.lock() {
                                if s.generation == generation && s.selector != Some(sel) {
                                    s.selector = Some(sel);
                                    s.epoch += 1;
                                    s.bins_db.clear();
                                    s.updated_at = None;
                                    s.switched_at = Some(std::time::Instant::now());
                                }
                            }
                        }
                        if let Some(levels) = decode_input_meters(&push) {
                            if let Ok(mut m) = meters.lock() {
                                m.generation = generation;
                                m.updated_at = Some(std::time::Instant::now());
                                m.levels = levels;
                                if let Some(outputs) = crate::output_meters::decode(&push) {
                                    m.outputs_at = Some(std::time::Instant::now());
                                    m.outputs = outputs;
                                } else {
                                    // A legacy input-only shape cannot retain output GR.
                                    m.outputs.clear();
                                    m.outputs_at = None;
                                }
                                if let Some(frames) = decode_dynamics_meters(&push) {
                                    m.dynamics_at = Some(std::time::Instant::now());
                                    m.dynamics = frames;
                                }
                            }
                            // The explicit frame carries RTA after the 62-word strip
                            // prefix. Legacy 60-word frames supply input IN only;
                            // their output aliases must never be labelled DL32S.
                            if let Some(bins) = decode_rta_bins(&push) {
                                if let Ok(mut s) = spectrum.lock() {
                                    if s.generation == generation {
                                        s.bins_db = bins;
                                        s.updated_at = Some(std::time::Instant::now());
                                    }
                                }
                            }
                            continue;
                        }
                        // Read-only native snapshot tracking: genuine desk
                        // recall/rename pushes update the view (received
                        // marks PARTIAL observed metadata — even a bare
                        // recall with no names proves only the last
                        // reported recall, never a full inventory). No
                        // param fanout: `decode_push` already maps these to
                        // nothing, and this view is sampled by the engine
                        // ticker, never fanned out as strip updates. The
                        // inside-lock generation check closes the
                        // seed race: a stale task that passed the outer
                        // check above must not write into the newer
                        // session's view.
                        if !crate::native::ingest_native(&native, generation, &push) {
                            break;
                        }
                        let refresh_preamp = affects_preamp_binding(&push);
                        let mut updates = Vec::new();
                        if refresh_preamp {
                            if let Some(sessions) = routed_session.upgrade() {
                                let guard = sessions.lock().await;
                                if gen.load(Ordering::Acquire) != generation {
                                    break;
                                }
                                if let Some(session) = guard.as_ref() {
                                    updates.extend(routed_preamp_updates(session, &push));
                                }
                            }
                        }
                        for update in decode_push(push).into_iter().chain(updates) {
                            let _ = tx.send(update);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        *self.session.lock().await = Some(session);
        self.start_input_refresh(generation);
        *self.addr.lock().await = Some(addr.to_string());
        Ok(())
    }

    pub(super) async fn lifecycle_disconnect(&self) -> anyhow::Result<()> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.input_refresh.lock().unwrap().take();
        if let Some(session) = self.session.lock().await.take() {
            session.close();
        }
        *self.addr.lock().await = None;
        if let Ok(mut m) = self.meters.lock() {
            m.updated_at = None;
            m.levels.clear();
            m.outputs_at = None;
            m.outputs.clear();
            m.dynamics_at = None;
            m.dynamics.clear();
        }
        if let Ok(mut s) = self.spectrum.lock() {
            s.updated_at = None;
            s.bins_db.clear();
        }
        // The last desk must not survive disconnect: the private
        // generation tag and the view reset together (the engine emits
        // `unavailable`, never stale slots; the getter would withhold a
        // mismatched generation either way).
        if let Ok(mut n) = self.native.lock() {
            *n = NativeCache::default();
        }
        Ok(())
    }
}
