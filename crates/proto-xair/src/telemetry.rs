//! Meter subscriptions, renewal, caches, and spectrum access.
use super::*;

impl XAirDriver {
    pub async fn last_dyn(&self) -> Option<dyn_gr::DynMeters> {
        self.dyn_cache
            .lock()
            .await
            .as_ref()
            .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500))
            .map(|(_, meters)| meters.clone())
    }

    /// Meter subscribe with an explicit renewal interval (tests pass a short
    /// interval; production uses [`addrs::METER_RENEW_INTERVAL`]).
    pub async fn subscribe_meters_with_interval(
        &self,
        interval: std::time::Duration,
    ) -> anyhow::Result<broadcast::Receiver<openfader_core::MeterFrame>> {
        let rx = self.session().await?.as_ref().unwrap().subscribe_meters();
        self.spawn_meter_renewal(interval).await?;
        // Keep the receiver/drain and watchdog installed even after an
        // initial send failure; its same-session retry can then recover.
        if let Err(error) = self.subscribe_meters_once().await {
            eprintln!("xair: initial stream subscribe failed: {error}; watchdog will retry");
        }
        Ok(rx)
    }

    /// One-shot subscribe (no renewal loop). Shared by
    /// [`MixerDriver::subscribe_meters`] and
    /// [`Self::subscribe_meters_with_interval`].
    pub(super) async fn subscribe_meters_once(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openfader_core::MeterFrame>> {
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        // Documented working form (xr-meters against real XR hardware, x-Air
        // Remote Protocol doc `/meters ,si <id> <n>`): subscribe by sending
        // the stream id AS A STRING ARG to bare `/meters` — NOT an empty
        // query to `/meters/1` (desk is silent there, baseline T4).
        //
        // INFERRED, not owner-rig verified: the subscription reportedly
        // expires after ~10 s (xr-meters re-issues every 9 s) and needs the
        // `/xremote` keepalive (every ~5 s, already running via `connect`)
        // alongside. Rig-verified 2026-10-20: every `/meters/N` blob is a
        // 4-byte LE i32 sample count + that many LE i16 dBFSx256 samples
        // (`/meters/1` = 84 B = 40 samples); transport's
        // `parse_xair_meter_blob` strips the count word and decodes
        // dB→linear. The xr-meters 4-header-byte note describes this same
        // count word, not a second header.
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("meter subscribe: {e}"))?;
        // ALL DYN (gate/comp GR) stream, same subscribe form with the
        // `/meters/6` id (Behringer X AIR OSC doc fw 1.11 — NOT owner-rig
        // verified). A send failure here is local socket I/O (UDP sends
        // cannot fail remotely), so it propagates like the levels
        // subscribe above. The DYN blobs the desk then streams decode via
        // `ingest_dyn_message` into the `gate_gr` / `comp_gr` param reads;
        // the shared hub only carries levels, so nothing else changes here.
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("dyn meter subscribe: {e}"))?;
        Ok(session.subscribe_meters())
    }

    /// Spawn the meter renewal loop: re-send `/xremote` + both `/meters`
    /// subscribes every `interval` so the desk (suspected ~10 s expiry,
    /// INFERRED) keeps streaming. Fire-and-forget detached task over a
    /// cloned [`std::sync::Arc`] socket — no driver/session lock held, so
    /// renewals never block queries and tolerate resubscribes.
    pub(super) async fn spawn_meter_renewal(
        &self,
        interval: std::time::Duration,
    ) -> anyhow::Result<()> {
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap().clone();
        let sock = std::sync::Arc::clone(session.socket());
        let dyn_cache = self.dyn_cache.clone();
        let generation = self.connection_generation.clone();
        let connection = generation.load(std::sync::atomic::Ordering::Acquire);
        drop(guard);
        let xremote = osc::query_message(addrs::XREMOTE)?;
        let sub_levels = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())],
        )?;
        let sub_dyn = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())],
        )?;
        let task = tokio::spawn(async move {
            let mut ticker = openfader_transport::session::keepalive_ticker(
                interval.min(std::time::Duration::from_secs(1)),
            );
            let mut renewed = std::time::Instant::now();
            // `keepalive_ticker` fires immediately; consume it so the first
            // renewal lands one full `interval` after the initial subscribe.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                    break;
                }
                let stale_levels = session
                    .latest_meter_sample()
                    .is_none_or(|(at, _)| at.elapsed() > std::time::Duration::from_millis(500));
                let stale_dyn = dyn_cache
                    .lock()
                    .await
                    .as_ref()
                    .is_none_or(|(at, _)| at.elapsed() > std::time::Duration::from_millis(500));
                if renewed.elapsed() < interval && !stale_levels && !stale_dyn {
                    continue;
                }
                renewed = std::time::Instant::now();
                for bytes in [&xremote, &sub_levels, &sub_dyn] {
                    if let Err(error) = sock.send(bytes).await {
                        eprintln!("xair: subscription renewal failed: {error}");
                    }
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        Ok(())
    }

    /// Decode one raw `/meters/6` OSC message into [`DynMeters`]
    /// (shared by [`Self::ingest_dyn_message`] and the per-connection
    /// ingest task in `connect`).
    pub(super) fn decode_dyn_message(msg: &OscMessage) -> anyhow::Result<dyn_gr::DynMeters> {
        if msg.addr != addrs::METER_DYN_SUBSCRIBE {
            anyhow::bail!("ingest_dyn_message: unsupported meter {}", msg.addr);
        }
        let blob = msg.args.iter().find_map(|a| match a {
            OscType::Blob(b) => Some(b.as_slice()),
            _ => None,
        });
        let Some(blob) = blob else {
            anyhow::bail!("ingest_dyn_message: /meters/6 without blob payload");
        };
        dyn_gr::parse_dyn_blob(blob)
    }

    /// Ingest one raw `/meters/6` OSC message into the GR cache (the
    /// documented ALL DYN shape — see [`crate::dyn_gr`]). Any other address
    /// (or a non-blob payload) bails: no guessing at unknown meter layouts.
    ///
    /// Ingestion also runs live: the per-connection task in `connect`
    /// routes every streamed DYN blob through [`Self::decode_dyn_message`]
    /// into the same cache, so the `gate_gr` / `comp_gr` param reads serve
    /// live reduction once the desk streams (see `subscribe_meters_once`).
    pub async fn ingest_dyn_message(&self, msg: &OscMessage) -> anyhow::Result<()> {
        let dyn_meters = Self::decode_dyn_message(msg)?;
        *self.dyn_cache.lock().await = Some((std::time::Instant::now(), dyn_meters));
        Ok(())
    }

    /// GR read for one strip from the DYN cache. Errors when no blob has
    /// been ingested yet (caller keeps the static scale) or the strip is
    /// out of range — never a fabricated zero.
    pub(super) async fn cached_gr(&self, strip: StripId, gate: bool) -> anyhow::Result<f32> {
        let cache = self.dyn_cache.lock().await;
        let Some((_, dyn_meters)) = cache
            .as_ref()
            .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500))
        else {
            anyhow::bail!("xair: no DYN meter data yet (desk not streaming /meters/6)");
        };
        if gate {
            dyn_meters.gate_gr(strip.0)
        } else {
            // Resolve family-local slots from topology; returns, FX sends,
            // aux and DCAs have no slot in the ALL DYN payload.
            let family = XAIR_TOPOLOGY.iter().find(|f| {
                usize::from(strip.0) >= f.first && usize::from(strip.0) < f.first + f.count
            });
            match family.map(|f| (f.key, usize::from(strip.0) - f.first)) {
                Some(("ch", i)) => Ok(dyn_meters.comp[i]),
                Some(("bus", i)) => Ok(dyn_meters.bus[i]),
                Some(("main", 0)) => Ok(dyn_meters.lr),
                _ => anyhow::bail!("xair: no compressor GR slot for strip {}", strip.0),
            }
        }
    }

    pub(super) fn driver_telemetry_timestamps(
        &self,
    ) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> {
        let levels = self
            .meter_latest
            .lock()
            .unwrap()
            .as_ref()
            .map(|(at, _)| *at);
        let gr = self
            .dyn_cache
            .try_lock()
            .ok()
            .and_then(|cache| cache.as_ref().map(|(at, _)| *at));
        Some((levels, gr))
    }

    pub(super) async fn driver_refresh_streams(&self) -> anyhow::Result<()> {
        let session = self.session().await?.as_ref().unwrap().clone();
        // Drain pre-suspend socket traffic through a lightweight reply
        // barrier, then require new samples. No cached replay becomes fresh.
        let _ = session
            .query_timeout(
                addrs::INFO,
                openfader_transport::session::CONNECT_BURST_TIMEOUT,
            )
            .await;
        *self.receive_since.lock().unwrap() = std::time::Instant::now();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        session.send(addrs::XREMOTE, Vec::new()).await?;
        self.subscribe_meters_once().await?;
        Ok(())
    }

    /// Strip-ordered meter levels (one entry per strip, [`XAIR_STRIPS`]).
    /// The `/meters/1` blob (the only level stream subscribed) decodes in
    /// transport to 40 linear levels: the input bank `0..16` feeds ch 1..16
    /// (pre-mute taps — live-verified pattern 2026-10-20: hot while the
    /// channels sit muted, ch15-anchored at idx 14, aux pre pair at 16/17),
    /// and idx 26 feeds the aux-return strip 35 (`/rtn/aux` mute-gated,
    /// live-verified 2026-10-20). Strips past the channels (FX/bus/main/
    /// DCA) and the remaining blob slots have no rig-verified strip
    /// mapping, so they read honest silence here — never a neighbor's
    /// level. `None` (no blob yet) reads back full-map silence, like the
    /// core default.
    pub(super) fn driver_poll_meters(&self) -> openfader_core::MeterFrame {
        let sample = self.meter_latest.lock().unwrap();
        let Some((_, latest)) = sample
            .as_ref()
            .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500))
        else {
            return openfader_core::MeterFrame { levels: Vec::new() };
        };
        let mut levels = vec![0.0f32; XAIR_STRIPS];
        for (i, v) in latest.iter().take(16).enumerate() {
            levels[i] = *v;
        }
        if let Some(v) = latest.get(26) {
            levels[35] = *v;
        }
        openfader_core::MeterFrame { levels }
    }

    pub(super) fn driver_poll_spectrum(&self) -> Vec<openfader_core::AudioSpectrum> {
        self.rta
            .lock()
            .unwrap()
            .as_ref()
            .map_or_else(Vec::new, |rta| rta.poll())
    }

    pub(super) async fn driver_set_spectrum_target(
        &self,
        owner: &str,
        strip: Option<StripId>,
    ) -> anyhow::Result<()> {
        let stream = self.rta.lock().unwrap().clone();
        if let Some(stream) = stream {
            stream.target(owner, strip, false).await?;
        }
        Ok(())
    }

    pub(super) async fn driver_subscribe_meters(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openfader_core::MeterFrame>> {
        let rx = self
            .subscribe_meters_with_interval(addrs::METER_RENEW_INTERVAL)
            .await?;
        // Blob-order cache drain feeding `poll_meters`: a resubscribed
        // clone sees every frame alongside the caller's receiver.
        let mut drain = rx.resubscribe();
        let cache = self.meter_latest.clone();
        let receive_since = self.receive_since.clone();
        let session = self.session().await?.as_ref().unwrap().clone();
        let generation = self.connection_generation.clone();
        let connection = generation.load(std::sync::atomic::Ordering::Acquire);
        let task = tokio::spawn(async move {
            loop {
                match drain.recv().await {
                    Ok(_frame) => {
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                            break;
                        }
                        if let Ok(mut guard) = cache.lock() {
                            *guard = session
                                .latest_meter_sample()
                                .filter(|(at, _)| *at >= *receive_since.lock().unwrap())
                                .map(|(at, frame)| (at, frame.levels));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        Ok(rx)
    }
}
