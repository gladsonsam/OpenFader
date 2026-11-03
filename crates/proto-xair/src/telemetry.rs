//! Meter subscriptions, renewal, caches, and spectrum access.
use super::*;

const AUTOMIX_METER_STREAM: &str = "/meters/7";
const INPUT_METER_STREAM: &str = "/meters/2";
const DCA_METER_STREAM: &str = "/meters/8";
const OUTPUT_METER_STREAM: &str = "/meters/5";

impl XAirDriver {
    // The two anchored slots are 1 and 14; the other positions follow the
    // documented contiguous bank, but have not been individually correlated.
    pub(super) fn decode_automix_gain(msg: &OscMessage) -> Option<[f32; 16]> {
        if msg.addr != AUTOMIX_METER_STREAM {
            return None;
        }
        let [OscType::Blob(blob)] = msg.args.as_slice() else {
            return None;
        };
        if blob.len() != 36 || i32::from_le_bytes(blob[..4].try_into().ok()?) != 16 {
            return None;
        }
        let mut values = [0.0; 16];
        for (i, value) in values.iter_mut().enumerate() {
            *value = f32::from(i16::from_le_bytes([blob[4 + i * 2], blob[5 + i * 2]])) / 256.0;
        }
        Some(values)
    }

    pub(super) async fn cached_automix_gain(&self, strip: StripId) -> anyhow::Result<f32> {
        let cache = self.automix_gain_cache.lock().await;
        let (_, values) = cache
            .as_ref()
            .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500))
            .ok_or_else(|| anyhow::anyhow!("xair: automix gain unavailable or stale"))?;
        values
            .get(usize::from(strip.0))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("xair: no automix gain slot"))
    }

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
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        session.enable_xair_secondary_meters();
        let rx = session.subscribe_meters();
        drop(guard);
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
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(DCA_METER_STREAM.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("DCA meter subscribe: {e}"))?;
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(OUTPUT_METER_STREAM.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("output meter subscribe: {e}"))?;
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(INPUT_METER_STREAM.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("input meter subscribe: {e}"))?;
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(AUTOMIX_METER_STREAM.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("automix meter subscribe: {e}"))?;
        Ok(session.subscribe_meters())
    }

    /// Spawn the meter renewal loop: re-send `/xremote` and all `/meters`
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
        let sub_dca = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(DCA_METER_STREAM.to_string())],
        )?;
        let sub_outputs = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(OUTPUT_METER_STREAM.to_string())],
        )?;
        let sub_inputs = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(INPUT_METER_STREAM.to_string())],
        )?;
        let sub_automix = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(AUTOMIX_METER_STREAM.to_string())],
        )?;
        let link_queries = ["1-2", "3-4", "5-6"]
            .map(|pair| osc::query_message(&format!("/config/buslink/{pair}")))
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;
        let policy_queries = super::link_policy::IDS
            .map(|id| osc::query_message(&format!("/config/linkcfg/{id}")))
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;
        let clock_query = osc::query_message("/-prefs/clockrate")?;
        let mute_queries = [
            osc::query_message("/-prefs/hardmute")?,
            osc::query_message("/-prefs/dcamute")?,
        ];
        let safe_query = osc::query_message("/-prefs/ponmute")?;
        let usb_query = osc::query_message("/-prefs/usbifcmode")?;
        let solo_query = osc::query_message(addrs::SOLO_SW)?;
        let midi_query = osc::query_message("/-prefs/midiconfig")?;
        let is_xr18 = self.model == addrs::XAirModel::Xr18;
        let task = tokio::spawn(async move {
            let mut ticker = openfader_transport::session::keepalive_ticker(
                interval.min(std::time::Duration::from_secs(1)),
            );
            if is_xr18 {
                let _ = sock.send(&usb_query).await;
                let _ = sock.send(&safe_query).await;
                for query in &mute_queries {
                    let _ = sock.send(query).await;
                }
                let _ = sock.send(&clock_query).await;
                let _ = sock.send(&midi_query).await;
                let _ = sock.send(&solo_query).await;
            }
            let mut renewed = std::time::Instant::now();
            let mut links_renewed = std::time::Instant::now();
            // `keepalive_ticker` fires immediately; consume it so the first
            // renewal lands one full `interval` after the initial subscribe.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                    break;
                }
                if links_renewed.elapsed() >= std::time::Duration::from_secs(2) {
                    links_renewed = std::time::Instant::now();
                    if is_xr18 {
                        let _ = sock.send(&usb_query).await;
                        let _ = sock.send(&safe_query).await;
                        for query in &mute_queries {
                            let _ = sock.send(query).await;
                        }
                        let _ = sock.send(&clock_query).await;
                        let _ = sock.send(&midi_query).await;
                        let _ = sock.send(&solo_query).await;
                    }
                    for query in link_queries.iter().chain(policy_queries.iter()) {
                        if let Err(error) = sock.send(query).await {
                            eprintln!("xair: bus link read failed: {error}");
                        }
                    }
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
                for bytes in [
                    &xremote,
                    &sub_levels,
                    &sub_dyn,
                    &sub_dca,
                    &sub_outputs,
                    &sub_inputs,
                    &sub_automix,
                ] {
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
    /// routes every streamed DYN blob through `decode_dyn_message`
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
        *self.automix_gain_cache.lock().await = None;
        session.send(addrs::XREMOTE, Vec::new()).await?;
        self.subscribe_meters_once().await?;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        self.refresh_output_geq(session.clone(), generation);
        self.refresh_automix_settings(session.clone(), generation);
        self.refresh_output_misc(session.clone(), generation);
        self.refresh_bus_links(session.clone(), generation);
        self.refresh_output_routing(session, generation);
        Ok(())
    }

    /// Strip-ordered meter levels (one entry per strip, [`XAIR_STRIPS`]).
    /// The `/meters/1` blob (the only level stream subscribed) decodes in
    /// transport to 40 linear levels: the input bank `0..16` feeds ch 1..16
    /// (pre-mute taps — live-verified pattern 2026-10-20: hot while the
    /// channels sit muted, ch15-anchored at idx 14, aux pre pair at 16/17),
    /// slots 16/17 are the aux pre-fader pair, 18..26 are the four stereo
    /// FX returns, 26..32 are buses, and 32..36 are FX sends. Stereo pairs
    /// feed the single strip meter using the louder side; 36/37 are the
    /// main post-fader pair. The bank layout follows the X AIR OSC protocol;
    /// bus 1 and aux were cross-checked against `/meters/0` on XR18 FW 1.20.
    /// Four DCA levels from `/meters/8` append to the cache only while fresh;
    /// slots 31..35 are silent without a recent DCA frame. Missing or stale
    /// primary data returns an empty frame.
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
        if let (Some(left), Some(right)) = (latest.get(16), latest.get(17)) {
            levels[35] = left.max(*right);
        }
        for i in 0..4 {
            if let (Some(left), Some(right)) = (latest.get(18 + i * 2), latest.get(19 + i * 2)) {
                levels[16 + i] = left.max(*right);
            }
        }
        for i in 0..6 {
            if let Some(level) = latest.get(26 + i) {
                levels[20 + i] = *level;
            }
        }
        for i in 0..4 {
            if let Some(level) = latest.get(32 + i) {
                levels[26 + i] = *level;
            }
        }
        if let (Some(left), Some(right)) = (latest.get(36), latest.get(37)) {
            levels[30] = left.max(*right);
        }
        for i in 0..4 {
            if let Some(level) = latest.get(40 + i) {
                levels[31 + i] = *level;
            }
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

    /// Physical sockets, not bus strip identities: routing can assign any source.
    pub(super) fn physical_output_targets() -> Vec<String> {
        (1..=6)
            .map(|n| format!("physical/aux/{n:02}"))
            .chain(["physical/main".to_string()])
            .chain((1..=16).map(|n| format!("physical/p16/{n:02}")))
            .chain((1..=18).map(|n| format!("physical/usb/{n:02}")))
            .chain(["physical/phones".to_string()])
            .collect()
    }

    pub(super) fn physical_input_targets() -> Vec<String> {
        (1..=16)
            .map(|n| format!("input/mic/{n:02}"))
            .chain((1..=2).map(|n| format!("input/aux/{n:02}")))
            .chain((1..=18).map(|n| format!("input/usb/{n:02}")))
            .collect()
    }

    pub(super) fn driver_poll_output_meters(&self) -> openfader_core::OutputMeterSnapshot {
        use std::sync::atomic::Ordering;
        let generation = self.connection_generation.load(Ordering::Acquire);
        let mut snapshot = openfader_core::OutputMeterSnapshot {
            generation,
            age_ms: 0,
            frames: Vec::new(),
        };
        if !self.connected.load(Ordering::Acquire) {
            return snapshot;
        }
        let since = *self.receive_since.lock().unwrap();
        let cache = self.output_latest.lock().unwrap();
        if let Some((at, values)) = cache.as_ref().filter(|(at, values)| {
            *at >= since
                && at.elapsed() <= std::time::Duration::from_millis(500)
                && values.len() == 44
        }) {
            snapshot.age_ms = at.elapsed().as_millis() as u64;
            let offsets = (0..6)
                .map(|i| (i, 1))
                .chain([(6, 2)])
                .chain((8..24).map(|i| (i, 1)))
                .chain((24..42).map(|i| (i, 1)))
                .chain([(42, 2)]);
            snapshot.frames.extend(
                Self::physical_output_targets()
                    .into_iter()
                    .zip(offsets)
                    .map(
                        |(target, (index, width))| openfader_core::OutputMeterFrame {
                            target,
                            levels: values[index..index + width].to_vec(),
                            inferred: true,
                            compressor_gr: None,
                        },
                    ),
            );
        }
        drop(cache);
        let inputs = self.input_latest.lock().unwrap();
        if let Some((at, values)) = inputs.as_ref().filter(|(at, values)| {
            *at >= since
                && at.elapsed() <= std::time::Duration::from_millis(500)
                && values.len() == 36
        }) {
            snapshot.age_ms = snapshot.age_ms.max(at.elapsed().as_millis() as u64);
            snapshot
                .frames
                .extend(Self::physical_input_targets().into_iter().zip(values).map(
                    |(target, level)| openfader_core::OutputMeterFrame {
                        target,
                        levels: vec![*level],
                        inferred: true,
                        compressor_gr: None,
                    },
                ));
        }
        snapshot
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
                                .map(|(at, frame)| {
                                    let mut levels = frame.levels;
                                    // Only the complete XR18 primary bank can be combined.
                                    if levels.len() == 40 {
                                        if let Some((_, dca)) = session
                                            .latest_xair_dca_sample()
                                            .filter(|(dca_at, _)| {
                                                *dca_at >= *receive_since.lock().unwrap()
                                                    && dca_at.elapsed()
                                                        <= std::time::Duration::from_millis(500)
                                            })
                                        {
                                            levels.extend(dca);
                                        }
                                    }
                                    (at, levels)
                                });
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        // Output bank freshness is independent of primary strip meter delivery.
        let session = self.session().await?.as_ref().unwrap().clone();
        let mut output_rx = session.subscribe_received_updates();
        let output_cache = self.output_latest.clone();
        let input_cache = self.input_latest.clone();
        let receive_since = self.receive_since.clone();
        let generation = self.connection_generation.clone();
        let connection = generation.load(std::sync::atomic::Ordering::Acquire);
        let task = tokio::spawn(async move {
            loop {
                match output_rx.recv().await {
                    Ok((_, msg)) if matches!(msg.addr.as_str(), "/meters/5" | "meters/5") => {
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                            break;
                        }
                        *output_cache.lock().unwrap() = session
                            .latest_xair_output_sample()
                            .filter(|(at, _)| *at >= *receive_since.lock().unwrap());
                    }
                    Ok((at, msg)) if matches!(msg.addr.as_str(), "/meters/2" | "meters/2") => {
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection {
                            break;
                        }
                        if at >= *receive_since.lock().unwrap() {
                            if let [OscType::Blob(blob)] = msg.args.as_slice() {
                                if let Some(values) =
                                    openfader_transport::meters::parse_xair_meter_blob(blob)
                                        .filter(|v| v.len() == 36)
                                {
                                    *input_cache.lock().unwrap() = Some((at, values));
                                }
                            }
                        }
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        Ok(rx)
    }
}
