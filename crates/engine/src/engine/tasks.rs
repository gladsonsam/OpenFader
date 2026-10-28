//! Background control ingest, telemetry fan-out, polling, and lock-sweep task wiring.

use super::*;

impl Engine {
    pub(super) fn ensure_tasks(self: &Arc<Self>) {
        if self.tasks_started.swap(true, Ordering::SeqCst) {
            return;
        }
        // R2 desk-broadcast ingest: drivers that publish desk-originated
        // mix-surface updates (another client's moves, echoed to every
        // subscriber) get one task each, apply them to the param cache
        // live as `params` diffs, no poll-sweep wait. While another driver
        // is active, a driver's updates never touch the cache.
        for name in self.driver_order.clone() {
            let rx = match self.drv(&name).subscribe_updates() {
                Some(rx) => rx,
                None => continue,
            };
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut rx = rx;
                loop {
                    let update = match rx.recv().await {
                        Ok(u) => u,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            if let Some(engine) = weak.upgrade() {
                                eprintln!(
                                    "engine: {name} control update lagged {n}; repair queued"
                                );
                                engine.repair_dirty.store(true, Ordering::Release);
                            }
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    if engine.active_driver() != name
                        || engine.reads_suspended.load(Ordering::Acquire)
                    {
                        continue;
                    }
                    engine.apply_received(
                        update.strip.0,
                        &update.param,
                        &update.value,
                        update.received_at,
                        None,
                    );
                }
            });
        }
        // Param-plane poller: pipelined batch per tick (R2; one query per
        // tick at default inflight 1, more when configured), diff
        // broadcasts on change, caps updates on unsupported-dropout.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            let interval = self.poll_interval;
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(interval);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tick.tick().await;
                    match weak.upgrade() {
                        Some(engine) => engine.poll_tick().await,
                        None => break,
                    }
                }
            });
        }
        // Expiry sweeper: releases + broadcasts expired pins so an idle
        // session still notifies siblings, and prunes detached mailboxes.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            let interval =
                (self.lock_ttl / 2).clamp(Duration::from_millis(50), Duration::from_secs(5));
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(interval);
                loop {
                    tick.tick().await;
                    match weak.upgrade() {
                        Some(engine) => {
                            engine.prune_closed();
                            if engine.prune_expired_locks() {
                                engine.broadcast(engine.locks_message_inner());
                            }
                        }
                        None => break,
                    }
                }
            });
        }
        // Meter/spectrum/GR ticker at the SOURCE rate
        // ([`METER_INTERVAL`](openfader_transport::METER_INTERVAL) = 50 ms =
        // 20 Hz, the desk rate): no adapter-side downsampling. GR rides the
        // same tick (cache-only reads, no extra wire traffic). Slow clients
        // get latest-only delivery in the adapter (`crates/serve`), so the
        // engine always fans out the freshest frame. R2 taps ride here too,
        // but only while at least one client negotiated them (old clients
        // see no new traffic).
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut had_audio_spectrum = false;
                let mut audio_spectrum_generation = 0;
                let mut last_audio_spectrum_epoch = 0;
                let mut meters_live = false;
                let mut dynamics_live = false;
                let mut outputs_live = false;
                let mut output_source_generation = 0;
                let mut parameter_labels_last = None;
                let mut destination_controls_last = None;
                let mut output_routes_last = None;
                let mut last_level_sample = None;
                let mut last_gr_sample = None;
                let mut tick = tokio::time::interval(openfader_transport::METER_INTERVAL);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // Skip the immediate first tick so a fresh client gets
                // state first.
                tick.tick().await;
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    let stamps = engine.active_drv().telemetry_timestamps();
                    let level_updated =
                        stamps.is_none_or(|(at, _)| at.is_some() && at != last_level_sample);
                    let gr_updated =
                        stamps.is_none_or(|(_, at)| at.is_some() && at != last_gr_sample);
                    if let Some((levels, gr)) = stamps {
                        last_level_sample = levels;
                        last_gr_sample = gr;
                    }
                    let levels = engine.meter_levels().await;
                    let frame = MeterFrame { levels };
                    if !frame.levels.is_empty() {
                        if level_updated {
                            engine.broadcast(Event::Meters {
                                levels: frame.levels.clone(),
                            });
                        }
                        meters_live = true;
                    } else if meters_live {
                        // Fresh -> unavailable transition (DL32S stale >1 s,
                        // disconnect, or suspended switch): clear the UI once
                        // instead of leaving the last levels frozen (stuck
                        // clip). One-shot; stays silent until live again.
                        // Mirrors the `had_audio_spectrum` clear below, on
                        // the same tick, no new timers.
                        meters_live = false;
                        engine.broadcast(Event::Meters { levels: Vec::new() });
                        if engine.any_taps_client() {
                            let tap_drv = engine.active_drv();
                            // Fallback taps only: `poll_taps` drivers keep
                            // serving their own tap frames, so only clear the
                            // meter-derived tap here, mirroring the
                            // `tap_frames` fallback name (`in` for DL32S,
                            // else `post`). Tap-less drivers emit nothing.
                            if tap_drv.poll_taps().is_none() && !tap_drv.meter_taps().is_empty() {
                                let tap = if tap_drv.meter_taps() == ["in"] {
                                    "in"
                                } else {
                                    "post"
                                };
                                engine.broadcast(Event::Taps {
                                    tap: tap.to_string(),
                                    levels: Vec::new(),
                                });
                            }
                        }
                    }
                    if engine.any_taps_client() && (level_updated || gr_updated) {
                        for (tap, tap_levels) in engine.tap_frames().await {
                            if (tap == "post" && !level_updated)
                                || (tap.starts_with("gr_") && !gr_updated)
                            {
                                continue;
                            }
                            engine.broadcast(Event::Taps {
                                tap,
                                levels: tap_levels,
                            });
                        }
                    }
                    let spectrum_generation = engine.connection_gen.load(Ordering::Acquire);
                    let drv = engine.active_drv();
                    if spectrum_generation != audio_spectrum_generation {
                        audio_spectrum_generation = spectrum_generation;
                        had_audio_spectrum = false;
                        last_audio_spectrum_epoch = 0;
                    }
                    let mut supplied = std::collections::HashSet::new();
                    let frames = if engine.reads_suspended.load(Ordering::Acquire) {
                        Vec::new()
                    } else {
                        drv.poll_spectrum()
                    };
                    if frames.is_empty() && had_audio_spectrum {
                        engine.broadcast(Event::AudioSpectrum {
                            ch: None,
                            bins_db: Vec::new(),
                            band_edges_hz: Vec::new(),
                            source: "unavailable".into(),
                            position: String::new(),
                            epoch: last_audio_spectrum_epoch,
                        });
                    }
                    had_audio_spectrum = !frames.is_empty();
                    {
                        for frame in frames {
                            let n = frame.bins_db.len();
                            if n == 0
                                || n > 1024
                                || frame.band_edges_hz.len() != n + 1
                                || frame
                                    .strip
                                    .is_some_and(|s| usize::from(s.0) >= drv.strip_count())
                                || frame.bins_db.iter().any(|v| !v.is_finite())
                                || frame
                                    .band_edges_hz
                                    .iter()
                                    .any(|v| !v.is_finite() || *v <= 0.0)
                                || frame.band_edges_hz.windows(2).any(|w| w[0] >= w[1])
                            {
                                continue;
                            }
                            if let Some(strip) = frame.strip {
                                supplied.insert(usize::from(strip.0));
                            }
                            last_audio_spectrum_epoch = frame.epoch;
                            engine.broadcast(Event::AudioSpectrum {
                                ch: frame.strip.map(|s| s.0),
                                bins_db: frame.bins_db,
                                band_edges_hz: frame.band_edges_hz,
                                source: frame.source,
                                position: frame.position,
                                epoch: frame.epoch,
                            });
                        }
                    }
                    // Demo spectra come from the simulator's stable frequency
                    // profile, never an FFT of control-rate meter envelopes.
                    for ch in 0..frame.levels.len().min(SPECTRUM_CHANNELS) {
                        if supplied.contains(&ch) {
                            continue;
                        }
                        if let Some(bins) = drv.demo_spectrum(ch) {
                            engine.broadcast(Event::Spectrum {
                                ch: ch as u16,
                                bins,
                            });
                        }
                    }
                    let outputs = if engine.reads_suspended.load(Ordering::Acquire) {
                        None
                    } else {
                        drv.poll_output_meters()
                    };
                    if engine.connection_gen.load(Ordering::Acquire) == spectrum_generation
                        && (outputs.as_ref().is_some_and(|s| !s.frames.is_empty()) || outputs_live)
                    {
                        let snapshot = outputs.unwrap_or(openfader_core::OutputMeterSnapshot {
                            generation: output_source_generation,
                            age_ms: 0,
                            frames: Vec::new(),
                        });
                        output_source_generation = snapshot.generation;
                        outputs_live = !snapshot.frames.is_empty();
                        engine.broadcast(Event::OutputMeters {
                            epoch: spectrum_generation,
                            source_generation: snapshot.generation,
                            age_ms: snapshot.age_ms,
                            frames: snapshot
                                .frames
                                .into_iter()
                                .map(|m| {
                                    let mut row = json!({"target": m.target, "levels": m.levels});
                                    if m.inferred {
                                        row["inferred"] = json!(true);
                                    }
                                    if let Some(gr) = m.compressor_gr {
                                        row["compressor_gr"] = json!({
                                            "reduction_db": gr.reduction_db,
                                            "inferred": gr.inferred,
                                        });
                                    }
                                    row
                                })
                                .collect(),
                        });
                    }
                    let dynamics = if engine.reads_suspended.load(Ordering::Acquire) {
                        Vec::new()
                    } else {
                        drv.poll_dynamics_meters().unwrap_or_default()
                    };
                    if !dynamics.is_empty() || dynamics_live {
                        dynamics_live = !dynamics.is_empty();
                        engine.broadcast(Event::DynamicsMeters { frames: dynamics.into_iter().map(|m| json!({
                            "ch": m.ch, "gate_in": m.gate_in, "gate_reduction": m.gate_reduction,
                            "gate_out": m.gate_out, "comp_in": m.comp_in,
                            "comp_reduction": m.comp_reduction, "comp_out": m.comp_out,
                        })).collect() });
                    }
                    let gr_count = drv.gr_channel_count();
                    for ch in 0..if gr_updated { gr_count } else { 0 } {
                        if let Some((gate, comp)) = engine.gr_db(ch).await {
                            engine.broadcast(Event::Gr {
                                ch: ch as u16,
                                gate,
                                comp,
                            });
                        }
                    }
                    if drv.name() == "xair" && !engine.reads_suspended.load(Ordering::Acquire) {
                        let generation = spectrum_generation;
                        for ch in 0..drv.channel_count() {
                            let db = match drv
                                .get_param(StripId(ch as u16), "xair:automix_gain")
                                .await
                            {
                                Ok(ParamValue::Float(value)) if value.is_finite() => Some(value),
                                _ => None,
                            };
                            let _sync = engine.sync_state.lock().unwrap();
                            if generation != engine.connection_gen.load(Ordering::Acquire)
                                || engine.active_driver() != drv.name()
                                || engine.reads_suspended.load(Ordering::Acquire)
                            {
                                break;
                            }
                            engine.broadcast(Event::AutomixGain {
                                epoch: generation,
                                ch: ch as u16,
                                db,
                            });
                        }
                    }
                    // Read-only native desk snapshots: re-sample the driver
                    // hook on this same tick (lock-local session-cache
                    // reads, no I/O, no new timer) and broadcast only on
                    // change or session-generation change. The generation
                    // is captured BEFORE the driver/view sampling, and the
                    // dedup + broadcast run under the `sync_state` barrier
                    // with a final generation + `reads_suspended` recheck
                    // so an old sample can never land after a newer
                    // `SyncEpoch` (the frontend rejects a mismatched
                    // `epoch`). No audio-param fanout: the frame carries
                    // slot ids, names, the current marker and the engine
                    // `epoch` only. No guard is held across an await (no
                    // await below); `invalidate_connection` shares the same
                    // barrier.
                    {
                        let generation = engine.connection_gen.load(Ordering::Acquire);
                        let driver = engine.active_driver();
                        let view = engine.drv(&driver).native_snapshots();
                        let labels = engine.drv(&driver).param_label_overrides();
                        let destination_controls = engine.drv(&driver).destination_controls();
                        let output_routes = engine.output_routing_fingerprint(&driver);
                        // Held to the end of the block: the barrier shared
                        // with `invalidate_connection`.
                        let _sync = engine.sync_state.lock().unwrap();
                        if engine.connection_gen.load(Ordering::Acquire) == generation
                            && !engine.reads_suspended.load(Ordering::Acquire)
                        {
                            let output_key = (generation, driver.clone(), output_routes);
                            if output_routes_last.as_ref() != Some(&output_key) {
                                engine.routing_dirty.store(true, Ordering::Release);
                                output_routes_last = Some(output_key);
                            }
                            let destination_key = (generation, destination_controls);
                            if destination_controls_last.as_ref() != Some(&destination_key) {
                                let emit = !destination_key.1.is_empty()
                                    || destination_controls_last
                                        .as_ref()
                                        .is_some_and(|last| !last.1.is_empty());
                                if emit {
                                    engine.broadcast(Event::DestinationControls {
                                        epoch: generation,
                                        controls: Self::destination_rows(&destination_key.1),
                                    });
                                }
                                destination_controls_last = Some(destination_key);
                            }
                            // Existing metadata frames carry dynamic provider
                            // names; no separate UI timer or parameter writes.
                            let labels_key = (generation, driver.clone(), labels);
                            if parameter_labels_last.as_ref() != Some(&labels_key) {
                                let has_labels = !labels_key.2.is_empty()
                                    || parameter_labels_last
                                        .as_ref()
                                        .is_some_and(|last| !last.2.is_empty());
                                parameter_labels_last = Some(labels_key);
                                if has_labels {
                                    engine.broadcast(engine.caps_message());
                                }
                            }
                            let key = (generation, view);
                            let changed = *engine.native_last.read().unwrap() != key;
                            if changed {
                                *engine.native_last.write().unwrap() = key.clone();
                                engine.broadcast(Self::native_message_for(
                                    &driver, &key.1, generation,
                                ));
                            }
                        }
                    }
                }
            });
        }
        // R2 link ticker: re-emit `link` every ~2 s with a fresh
        // round-trip measurement (spec: on change + periodic).
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(LINK_INTERVAL);
                // Skip the immediate first tick (connect paths emit link).
                tick.tick().await;
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    let event = engine.link_message().await;
                    engine.broadcast(event);
                }
            });
        }
        // Type pushes invalidate immediately; refill missing FX values
        // separately from snapshots, yielding to control writes. No timer
        // reads happen unless a type actually changed.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(50));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    if engine.reads_suspended.load(Ordering::Acquire)
                        || engine.hydrating.load(Ordering::Acquire) != 0
                    {
                        continue;
                    }
                    if engine.active_drv().fx_flavor() == FxFlavor::Numbered
                        && engine.fx_refill_requested.swap(false, Ordering::AcqRel)
                    {
                        let changed = engine.connection_changed.notified();
                        tokio::pin!(changed);
                        changed.as_mut().enable();
                        tokio::select! {
                            _ = &mut changed => {},
                            _ = engine.refresh_fx_cache() => {},
                        }
                    }
                }
            });
        }
        // R2 connect-fill worker: completes the deferred remainder of
        // `post_switch` (state + info refresh, full FX fill, routing,
        // masters) off the command path, one broadcast per frame as it
        // lands. Woken per switch; coalesced by generation so a second
        // switch while a fill is in flight restarts it once instead of
        // piling fills up.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                loop {
                    let notified = match weak.upgrade() {
                        Some(engine) => Arc::clone(&engine.fill_notify),
                        None => break,
                    };
                    notified.notified().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    loop {
                        let gen = engine.fill_gen.load(Ordering::Acquire);
                        let changed = engine.connection_changed.notified();
                        tokio::pin!(changed);
                        changed.as_mut().enable();
                        let connection = engine.fill_connection_gen.load(Ordering::Acquire);
                        tokio::select! {
                            _ = &mut changed => {},
                            _ = engine.deferred_fill(connection) => {},
                        }
                        if engine.fill_gen.load(Ordering::Acquire) == gen {
                            break;
                        }
                    }
                }
            });
        }
    }
}
