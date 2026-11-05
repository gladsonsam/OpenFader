//! Session lifecycle: connect, correlated reads, identity validation,
//! push ingest and control health.
use crate::{addrs, inbound, mapping::Mapping, X32Driver, QUERY_TIMEOUT};
use anyhow::{ensure, Context};
use openfader_core::{MixerInfo, ParamUpdate, ParamValue, StripId};
use openfader_transport::{osc, KeepaliveConfig, OscSession};
use rosc::OscType;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

pub(crate) struct Link {
    pub(crate) session: Arc<OscSession>,
    pub(crate) info: MixerInfo,
    pub(crate) updates: tokio::task::JoinHandle<()>,
    /// Meter subscribe/renew + drain tasks; aborted with the link so no
    /// stream task outlives its connection.
    pub(crate) meter_tasks: Vec<tokio::task::JoinHandle<()>>,
    /// Route hydration/renewal worker; aborted with the link so no query
    /// task outlives its connection (its fanout children drop with it).
    pub(crate) route_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.updates.abort();
        for task in self.meter_tasks.drain(..) {
            task.abort();
        }
        for task in self.route_tasks.drain(..) {
            task.abort();
        }
    }
}

impl X32Driver {
    pub(crate) fn session(&self) -> anyhow::Result<Arc<OscSession>> {
        self.link
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.session.clone())
            .context("x32: not connected")
    }

    pub(crate) fn ensure_current(&self, session: &Arc<OscSession>) -> anyhow::Result<()> {
        ensure!(
            Arc::ptr_eq(session, &self.session()?),
            "x32: connection changed during operation"
        );
        Ok(())
    }

    pub(crate) async fn read(&self, m: &Mapping) -> anyhow::Result<ParamValue> {
        let session = self.session()?;
        let msg = session
            .query_with_args_timeout(&m.path, vec![], QUERY_TIMEOUT, |msg| {
                // Meter blobs and node text are never param values.
                !matches!(msg.args.as_slice(), [OscType::Blob(_)])
            })
            .await?;
        self.ensure_current(&session)?;
        let value = m.decode(&msg)?;
        *self.last_control.lock().unwrap() = Some(Instant::now());
        Ok(value)
    }

    pub(crate) async fn read_int(&self, path: &str) -> anyhow::Result<i32> {
        let session = self.session()?;
        let msg = session.query_timeout(path, QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        *self.last_control.lock().unwrap() = Some(Instant::now());
        crate::mapping::strict_int(&msg).map_err(|e| anyhow::anyhow!("x32: {e}"))
    }

    pub(crate) async fn send(&self, path: &str, arg: OscType) -> anyhow::Result<()> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session.send(path, vec![arg]).await?;
        *self.last_control.lock().unwrap() = Some(Instant::now());
        Ok(())
    }

    /// Connect with provider-owned identity validation: `/info` must
    /// report an X32/M32 model before any X32 map is applied. An X-Air
    /// desk (same `/info` shape, different port/model) is refused here
    /// rather than mis-driven.
    pub(crate) async fn connect_session(&self, addr: &str) -> anyhow::Result<()> {
        let _guard = self.lifecycle.lock().await;
        self.link.write().unwrap().take();
        *self.last_control.lock().unwrap() = None;
        let peer = openfader_transport::session::resolve_peer(addr, addrs::PORT)?;
        let session = Arc::new(OscSession::connect(peer, Some(KeepaliveConfig::xremote())).await?);
        // Meter blobs need raw forwarding to the ordered receive plane
        // (the transport level hub cannot carry the X32 BE/LE blob form;
        // the telemetry drain decodes strictly there). Opt-in per
        // session, default off for every other provider.
        session.forward_received_addresses(crate::telemetry::received_meter_addresses())?;
        // Authoritative ping: UDP "connect" always succeeds, so a silent
        // host must fail fast instead of linking phantom.
        let mut verified: Option<Vec<String>> = None;
        for _ in 0..2 {
            if let Ok(reply) = session
                .query_timeout(
                    addrs::INFO,
                    openfader_transport::session::CONNECT_BURST_TIMEOUT,
                )
                .await
            {
                verified = Some(osc::string_args(&reply));
                break;
            }
        }
        let Some(strings) = verified else {
            anyhow::bail!(
                "no reply from {peer} — check the desk is powered, on this network, and the IP is correct"
            );
        };
        let (model, firmware) = addrs::info_model_firmware(&strings);
        anyhow::ensure!(
            addrs::is_x32_model(&model),
            "not an X32/M32 mixer (model {model:?} at {peer}) — refusing to apply X32 maps"
        );
        let info = MixerInfo {
            name: strings.get(1).cloned().unwrap_or_default(),
            model,
            firmware,
            addr: peer.to_string(),
        };
        // Push ingest: /xremote echoes every desk-side change addressed
        // as the param itself. Only unambiguous mix-surface leaves enter
        // the push plane, decoded to engineering units.
        let mut received = session.subscribe_received_updates();
        // Query replies belong in the ordered value lane, but must not
        // invalidate the very source values they are hydrating.
        let mut source_updates = session.subscribe_updates();
        let tx = self.updates.clone();
        let last_control = self.last_control.clone();
        // Session-scoped route hydration: pin one session Arc and its
        // route generation across every read. The maintenance worker owns
        // both; the push task below only sends refresh demand. Both task
        // handles join the link, so disconnect aborts them with it.
        // Solo observations reuse the same session generation (one session
        // identity, never mixed across reconnects).
        let route_generation = self.route_generation_for(&session)?;
        let (route_dirty_tx, route_dirty_rx) =
            tokio::sync::mpsc::unbounded_channel::<crate::routing::RouteRefresh>();
        let route_push_tx = route_dirty_tx.clone();
        let (solo_dirty_tx, solo_dirty_rx) =
            tokio::sync::mpsc::unbounded_channel::<crate::solo::SoloRefresh>();
        let solo_push_tx = solo_dirty_tx.clone();
        let solo_push_tx2 = solo_dirty_tx.clone();
        let updates = tokio::spawn(async move {
            loop {
                let (at, msg, unsolicited) = tokio::select! {
                    value = received.recv() => match value {
                        Ok((at, msg)) => (at, msg, false),
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Missed desk broadcasts make the route view
                            // suspect: drop it and re-read honestly rather
                            // than serving stale rows.
                            crate::routing::invalidate_all_routes(route_generation);
                            crate::solo::invalidate_all_solos(route_generation);
                            let _ = route_push_tx.send(crate::routing::RouteRefresh::All);
                            let _ = solo_push_tx.send(crate::solo::SoloRefresh::All);
                            continue;
                        }
                        Err(_) => break,
                    },
                    value = source_updates.recv() => match value {
                        Ok(msg) => (Instant::now(), msg, true),
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let _ = tx.send(ParamUpdate {
                                received_at: Instant::now(),
                                strip: StripId(u16::MAX),
                                param: "_preamp_routes_dirty".into(),
                                value: ParamValue::Bool(true),
                            });
                            crate::routing::invalidate_all_routes(route_generation);
                            crate::solo::invalidate_all_solos(route_generation);
                            let _ = route_push_tx.send(crate::routing::RouteRefresh::All);
                            let _ = solo_push_tx.send(crate::solo::SoloRefresh::All);
                            continue;
                        }
                        Err(_) => break,
                    },
                };
                if unsolicited {
                    let source_changed = msg.addr.starts_with("/-ha/")
                        || msg.addr.ends_with("/config/source")
                        || msg.addr.starts_with("/config/routing/")
                        || msg.addr.starts_with("/config/userrout/");
                    if source_changed || msg.addr.starts_with("/headamp/") {
                        let _ = tx.send(ParamUpdate {
                            received_at: at,
                            strip: StripId(u16::MAX),
                            param: if source_changed {
                                "_preamp_routes_dirty"
                            } else {
                                "_preamp_dirty"
                            }
                            .into(),
                            value: ParamValue::Bool(true),
                        });
                    }
                    // Desk-originated route traffic converges the sync
                    // route cache without polling: bank/user/mode leaves
                    // record directly, output leaves invalidate and
                    // re-read their full triple. Own hydration queries
                    // never arrive here (replies are not desk-originated),
                    // so refresh work cannot invalidate itself. Solo leaves
                    // converge the same way (strict 0/1 only).
                    if crate::routing::is_shared_route_addr(&msg.addr) {
                        crate::routing::note_route_push(&msg, route_generation, &route_push_tx);
                    }
                    if msg.addr.starts_with("/-stat/solosw/") {
                        crate::solo::note_solo_push(&msg, route_generation, &solo_push_tx2);
                    }
                    // Other unsolicited leaves already entered `received`.
                    continue;
                }
                if msg.addr.starts_with("/-stat/solosw/") {
                    crate::solo::note_solo_push(&msg, route_generation, &solo_push_tx2);
                }
                for (strip, param, value) in inbound::parse_inbound(&msg) {
                    *last_control.lock().unwrap() = Some(at);
                    let _ = tx.send(ParamUpdate {
                        received_at: at,
                        strip,
                        param,
                        value,
                    });
                }
            }
        });
        *self.link.write().unwrap() = Some(Link {
            session: session.clone(),
            info,
            updates,
            meter_tasks: Vec::new(),
            route_tasks: vec![
                tokio::spawn(crate::routing::route_maintenance_task(
                    session.clone(),
                    route_generation,
                    route_dirty_rx,
                )),
                tokio::spawn(crate::solo::solo_maintenance_task(
                    session,
                    route_generation,
                    solo_dirty_rx,
                )),
            ],
        });
        // Fresh epoch before any subscriber task can store: no sample
        // ever survives a session change.
        self.telemetry_reset();
        *self.last_control.lock().unwrap() = Some(Instant::now());
        // Control pushes ride the /xremote keepalive running in the
        // session. Meter streams are NOT renewed by /xremote (nor by
        // /renew, a desk-side no-op): telemetry re-sends its documented
        // /meters subscribes on demand from subscribe_meters/refresh.
        Ok(())
    }

    pub(crate) async fn disconnect_session(&self) -> anyhow::Result<()> {
        let _guard = self.lifecycle.lock().await;
        self.link.write().unwrap().take();
        *self.last_control.lock().unwrap() = None;
        // New epoch + cleared caches: late in-flight blobs cannot
        // repopulate the next session, and polls read unavailable.
        self.telemetry_reset();
        Ok(())
    }

    /// Control-path liveness independent of cached display values.
    pub async fn check_control_health(&self) -> anyhow::Result<()> {
        let session = self.session()?;
        session
            .query_timeout(addrs::INFO, Duration::from_secs(2))
            .await?;
        self.ensure_current(&session)?;
        Ok(())
    }
}
