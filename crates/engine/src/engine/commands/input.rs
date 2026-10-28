//! Input command arms.

use super::*;

impl Engine {
    async fn checked_bus_strip_write(
        &self,
        driver: &str,
        epoch: u64,
        ch: u16,
        field: &str,
        value: ParamValue,
        writer: &str,
    ) -> anyhow::Result<bool> {
        if driver != "xair"
            || !self.driver_topology(driver).iter().any(|family| {
                family.kind == openfader_core::StripKind::Bus
                    && usize::from(ch) >= family.first
                    && usize::from(ch) < family.first + family.count
            })
        {
            return Ok(false);
        }
        let check = |members: &[StripId]| {
            anyhow::ensure!(
                driver == self.active_driver()
                    && epoch == self.connection_gen.load(Ordering::Acquire)
                    && !self.reads_suspended.load(Ordering::Acquire),
                "mixer session changed"
            );
            let policy = self.policy_of(writer);
            for member in members {
                match (&value, field) {
                    (ParamValue::Float(v), "fader") => policy.allow_fader(*member, *v)?,
                    (_, "mute") => policy.allow_mute(*member)?,
                    _ => anyhow::bail!("unsupported bus strip write"),
                }
                anyhow::ensure!(
                    self.lock_holder(member.0)
                        .is_none_or(|holder| holder == writer),
                    "strip {} locked",
                    member.0
                );
            }
            Ok(())
        };
        self.drv(driver)
            .set_param_guarded_checked(StripId(ch), field, value.clone(), &BTreeMap::new(), &check)
            .await?;
        Ok(true)
    }

    /// Resolve only canonical send fields through the provider's explicit destination.
    pub(in crate::engine) async fn send_bus_members(
        &self,
        driver: &str,
        field: &str,
    ) -> anyhow::Result<Vec<StripId>> {
        let field = super::super::models::canonical_send_field(driver, field);
        let Some(rest) = field.strip_prefix("send") else {
            return Ok(Vec::new());
        };
        let Some((slot, suffix)) = rest.split_once('_') else {
            return Ok(Vec::new());
        };
        if !matches!(suffix, "level" | "tap" | "pan" | "grpon") {
            return Ok(Vec::new());
        }
        let Ok(slot) = slot.parse::<u16>() else {
            return Ok(Vec::new());
        };
        let drv = self.drv(driver);
        let Some(destination) = drv.send_destination(slot) else {
            return Ok(Vec::new());
        };
        if !destination.key.starts_with("bus/") {
            return Ok(Vec::new());
        }
        let topology = self.driver_topology(driver);
        if !topology
            .iter()
            .any(|family| family.kind == openfader_core::StripKind::Bus)
        {
            return Ok(Vec::new());
        }
        let index = openfader_core::strip_index_for_key(&topology, &destination.key)
            .ok_or_else(|| anyhow::anyhow!("missing send destination {}", destination.key))?;
        let index = u16::try_from(index)?;
        drv.control_members(StripId(index), "fader").await
    }

    pub(super) async fn apply_input(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
        policy: &Policy,
        expected: Option<BTreeMap<String, ParamValue>>,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            Cmd::InputProcessor {
                request_id,
                epoch,
                ch,
                key,
                processor,
                field,
                value,
                guard,
            } => Ok(Some(
                self.apply_input_processor(
                    request_id, epoch, ch, key, processor, field, value, guard, writer, out,
                )
                .await,
            )),
            Cmd::Fader { ch, value, key } => {
                let driver = self.active_driver();
                let epoch = self.connection_gen.load(Ordering::Acquire);
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_fader(StripId(ch), value)?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                for member in self
                    .active_drv()
                    .control_members(StripId(ch), "fader")
                    .await?
                {
                    policy.allow_fader(member, value)?;
                    if let Some(holder) = self.lock_holder(member.0) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(member.0, &holder)));
                        }
                    }
                }
                let from = self.offline_from(StripId(ch), "fader").await;
                if !self
                    .checked_bus_strip_write(
                        &driver,
                        epoch,
                        ch,
                        "fader",
                        ParamValue::Float(value),
                        writer,
                    )
                    .await?
                {
                    self.drv_set_fader(StripId(ch), value).await?;
                }
                if let Some(from) = from {
                    self.offline_note(ch, "fader", from, json!(value)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo (same as the `param` arm): the state
                // frame alone cannot confirm the write on drivers whose
                // snapshot is empty (xair), so re-read and broadcast what
                // the desk reports, falling back to the written value
                // (including when the re-read races the desk's apply lag
                // see `refresh_param`). The echo carries readback
                // provenance: `verified` when the desk confirmed it,
                // `accepted` when the write landed without readback proof
                // (DL32S always: no echo, no independent query).
                let (value, authority) = match self
                    .refresh_param(StripId(ch), "fader", &json!(value))
                    .await
                {
                    Some(v) => (v, Some("verified".to_string())),
                    None => (json!(value), Some("accepted".to_string())),
                };
                out.push(Event::Param {
                    ch,
                    field: "fader".to_string(),
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Mute { ch, muted, key } => {
                let driver = self.active_driver();
                let epoch = self.connection_gen.load(Ordering::Acquire);
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_mute(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                for member in self
                    .active_drv()
                    .control_members(StripId(ch), "mute")
                    .await?
                {
                    policy.allow_mute(member)?;
                    if let Some(holder) = self.lock_holder(member.0) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(member.0, &holder)));
                        }
                    }
                }
                let from = self.offline_from(StripId(ch), "mute").await;
                if !self
                    .checked_bus_strip_write(
                        &driver,
                        epoch,
                        ch,
                        "mute",
                        ParamValue::Bool(muted),
                        writer,
                    )
                    .await?
                {
                    self.drv_set_mute(StripId(ch), muted).await?;
                }
                if let Some(from) = from {
                    self.offline_note(ch, "mute", from, json!(muted)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo (same rationale as `fader` above, with
                // the same `verified`/`accepted` provenance on the echo).
                let (value, authority) =
                    match self.refresh_param(StripId(ch), "mute", &json!(muted)).await {
                        Some(v) => (v, Some("verified".to_string())),
                        None => (json!(muted), Some("accepted".to_string())),
                    };
                out.push(Event::Param {
                    ch,
                    field: "mute".to_string(),
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // LR main fader (all drivers via `get_main_fader`; `None` in
            // state when the driver has none). Not strip-locked: locks pin
            // per-channel strips.
            Cmd::MainFader { value } => {
                let value = openfader_core::validate_fader(value)?;
                policy.allow_main(value)?;
                let from = if self.offline.read().unwrap().active {
                    Some(
                        self.active_main_fader()
                            .await
                            .map(|v| json!(v))
                            .unwrap_or(Value::Null),
                    )
                } else {
                    None
                };
                self.drv_set_main(value).await?;
                if let Some(from) = from {
                    self.offline_note(u16::MAX, "main_fader", from, json!(value))
                        .await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // `main` echo (R2 display continuity): the inclusive getters
                // already carry own writes, so `state` + this frame keep a
                // new attach continuous. No fabrication: the fader IS the
                // submitted value, and `main` frames carry no `authority`
                // field, see the `param` event for verified values.
                self.push_state_with_by(writer, out).await;
                let muted = self.active_drv().get_main_mute().await.ok();
                out.push(Event::Main {
                    fader: Some(value),
                    muted,
                });
                Ok(None)
            }
            // R2 Main LR master mute (all drivers via `get_main_mute`;
            // unsupported drivers bail honestly). Master gate: the main
            // master is a shared mix like the bus masters.
            Cmd::MainMute { muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_main_mute(muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // `main` echo (same display-continuity rationale as above).
                let fader = self.active_drv().get_main_fader().await.ok();
                out.push(Event::Main {
                    fader,
                    muted: Some(muted),
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // R2 per-strip solo (monitor bus only). Channel-scoped lock +
            // strip gate (no FOH impact, but a locked strip is someone's).
            Cmd::Solo { ch, soloed, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_strip(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                for member in self
                    .active_drv()
                    .control_members(StripId(ch), "solo")
                    .await?
                {
                    policy.allow_strip(member)?;
                    if let Some(holder) = self.lock_holder(member.0) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(member.0, &holder)));
                        }
                    }
                }
                self.active_drv().set_solo(StripId(ch), soloed).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Echo the commanded state (house pattern, same as mute:
                // the re-read races the desk's apply lag, so a disagreeing
                // read falls back to the written value instead of flashing
                // a phantom unsoloed. The write path itself is rig-verified:
                // `/-stat/solosw/NN` 1-based sets the bank bit, `/-action/
                // clearsolo` clears it, XR18 FW 1.20, 2026-10-18.)
                //
                // Cache alongside the echo (see `apply_inbound`): the poll
                // diff then stays quiet instead of re-emitting this state.
                if !self.active_drv().is_hardware() {
                    self.apply_inbound(ch, "solo", &ParamValue::Bool(soloed));
                }
                out.push(Event::Solo {
                    ch,
                    soloed,
                    key: self.active_strip_key(ch),
                });
                Ok(None)
            }
            Cmd::Param {
                ch,
                field,
                value,
                key,
            } => {
                let driver = self.active_driver();
                let generation = self.connection_gen.load(Ordering::Acquire);
                let ch = self.resolve_strip(ch, key.as_deref())?;
                let field =
                    super::super::models::canonical_send_field(&driver, &field).into_owned();
                let pv = match &value {
                    Value::Number(n) => ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32),
                    Value::Bool(b) => ParamValue::Bool(*b),
                    Value::String(s) => ParamValue::Text(s.clone()),
                    _ => anyhow::bail!("param value must be number|bool|string"),
                };
                match (&field as &str, &pv) {
                    ("fader", ParamValue::Float(v)) => {
                        policy.allow_fader(StripId(ch), *v)?;
                    }
                    ("name", _) => {
                        policy.allow_rename(StripId(ch))?;
                    }
                    (field, _) if field == "lr_assign" || field.starts_with("dca") => {
                        policy.allow_assign(StripId(ch))?;
                    }
                    _ => {
                        policy.allow_strip(StripId(ch))?;
                    }
                }
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                if field == "link" {
                    let linked = match pv {
                        ParamValue::Bool(value) => value,
                        _ => anyhow::bail!("stereo link requires a boolean"),
                    };
                    let pair = self
                        .active_drv()
                        .stereo_pairs()
                        .into_iter()
                        .find(|pair| pair.contains(&ch))
                        .ok_or_else(|| {
                            anyhow::anyhow!("stereo pairing unavailable for this channel")
                        })?;
                    for member in pair {
                        // Linking can replace names, levels and assignments,
                        // so a strip-only grant is insufficient.
                        policy.allow_assign(StripId(member))?;
                        if linked {
                            policy.allow_rename(StripId(member))?;
                        }
                        if let Some(holder) = self.lock_holder(member) {
                            if holder != writer {
                                return Ok(Some(Self::locked_error(member, &holder)));
                            }
                        }
                    }
                    if linked && driver != "xair" {
                        let level = self.active_drv().get_fader(StripId(ch)).await?;
                        for member in pair {
                            policy.allow_fader(StripId(member), level)?;
                        }
                    }
                    let changes = if driver == "xair" {
                        let check = |members: &[StripId]| -> anyhow::Result<()> {
                            anyhow::ensure!(
                                self.active_driver() == driver
                                    && self.connection_gen.load(Ordering::Acquire) == generation
                                    && !self.reads_suspended.load(Ordering::Acquire),
                                "mixer session changed"
                            );
                            anyhow::ensure!(
                                members == [StripId(pair[0]), StripId(pair[1])],
                                "stereo link topology changed"
                            );
                            let current_policy = self.policy_of(writer);
                            for member in members {
                                current_policy.allow_strip(*member)?;
                                current_policy.allow_assign(*member)?;
                                if linked {
                                    current_policy.allow_rename(*member)?;
                                    current_policy.allow_fader(*member, 1.0)?;
                                }
                                anyhow::ensure!(
                                    self.lock_holder(member.0)
                                        .is_none_or(|holder| holder == writer),
                                    "strip {} locked",
                                    member.0
                                );
                            }
                            Ok(())
                        };
                        check(&[StripId(pair[0]), StripId(pair[1])])?;
                        self.drv(&driver)
                            .set_param_guarded_checked(
                                StripId(ch),
                                "link",
                                ParamValue::Bool(linked),
                                &BTreeMap::new(),
                                &check,
                            )
                            .await?;
                        pair.into_iter()
                            .map(|member| {
                                (
                                    StripId(member),
                                    "link".to_string(),
                                    ParamValue::Bool(linked),
                                )
                            })
                            .collect()
                    } else {
                        self.active_drv()
                            .set_stereo_link(StripId(ch), linked)
                            .await?
                    };
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    for (strip, field, value) in changes {
                        // As with ordinary writes, superseded desk evidence
                        // must not overwrite the accepted value in snapshots.
                        let _ = self
                            .refresh_param(strip, &field, &param_value_json(&value))
                            .await;
                        out.push(Event::Param {
                            ch: strip.0,
                            field,
                            value: param_value_json(&value),
                            by: Some(writer.to_string()),
                            key: self.active_strip_key(strip.0),
                            authority: Some("accepted".to_string()),
                        });
                    }
                    self.push_state_with_by(writer, out).await;
                    return Ok(None);
                }
                for member in self
                    .active_drv()
                    .control_members(StripId(ch), &field)
                    .await?
                {
                    match (field.as_str(), &pv) {
                        ("fader", ParamValue::Float(v)) => policy.allow_fader(member, *v)?,
                        ("mute", _) => policy.allow_mute(member)?,
                        ("name", _) => policy.allow_rename(member)?,
                        (field, _) if field == "lr_assign" || field.starts_with("dca") => {
                            policy.allow_assign(member)?
                        }
                        _ => policy.allow_strip(member)?,
                    }
                    if let Some(holder) = self.lock_holder(member.0) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(member.0, &holder)));
                        }
                    }
                }
                let send_members = self.send_bus_members(&driver, &field).await?;
                anyhow::ensure!(
                    self.active_driver() == driver
                        && self.connection_gen.load(Ordering::Acquire) == generation
                        && !self.reads_suspended.load(Ordering::Acquire),
                    "mixer session changed"
                );
                for member in &send_members {
                    self.policy_of(writer).allow_strip(*member)?;
                    if let Some(holder) = self.lock_holder(member.0) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(member.0, &holder)));
                        }
                    }
                }
                let is_xair_bus = driver == "xair"
                    && self.driver_topology(&driver).iter().any(|family| {
                        family.kind == openfader_core::StripKind::Bus
                            && usize::from(ch) >= family.first
                            && usize::from(ch) < family.first + family.count
                    });
                let from = self.offline_from(StripId(ch), &field).await;
                if expected.is_some()
                    || (driver == "xair" && !send_members.is_empty())
                    || is_xair_bus
                {
                    let empty = BTreeMap::new();
                    let expected = expected.as_ref().unwrap_or(&empty);
                    let drv = self.drv(&driver);
                    let check = |members: &[StripId]| -> anyhow::Result<()> {
                        anyhow::ensure!(
                            self.active_driver() == driver
                                && self.connection_gen.load(Ordering::Acquire) == generation
                                && !self.reads_suspended.load(Ordering::Acquire),
                            "mixer session changed"
                        );
                        let current_policy = self.policy_of(writer);
                        for member in members {
                            match (field.as_str(), &pv) {
                                ("fader", ParamValue::Float(v)) => {
                                    current_policy.allow_fader(*member, *v)?
                                }
                                ("mute", _) => current_policy.allow_mute(*member)?,
                                ("name", _) => current_policy.allow_rename(*member)?,
                                (field, _) if field == "lr_assign" || field.starts_with("dca") => {
                                    current_policy.allow_assign(*member)?
                                }
                                _ => current_policy.allow_strip(*member)?,
                            }
                            if let Some(holder) = self.lock_holder(member.0) {
                                anyhow::ensure!(
                                    holder == writer,
                                    "strip {} locked by {}",
                                    member.0,
                                    holder
                                );
                            }
                        }
                        Ok(())
                    };
                    if driver == "xair"
                        && (matches!(field.as_str(), "gain" | "phantom")
                            || !send_members.is_empty()
                            || is_xair_bus)
                    {
                        drv.set_param_guarded_checked(
                            StripId(ch),
                            &field,
                            pv.clone(),
                            expected,
                            &check,
                        )
                        .await?;
                    } else {
                        drv.set_param_guarded(StripId(ch), &field, pv, expected)
                            .await?;
                    }
                } else {
                    self.drv_set_param(StripId(ch), &field, pv).await?;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo: re-read the param just written and
                // broadcast what the driver reports (wire quantization may
                // shift it); fall back to the written value when the
                // re-read fails, or when it races the desk's apply lag
                // (see `refresh_param`), so the echo still lands. Provenance
                // rides along (`verified` on desk agreement, `accepted`
                // otherwise); frontends must not count `accepted` as recall
                // confirmation.
                let refreshed = self.refresh_param(StripId(ch), &field, &value).await;
                let (value, authority) = match refreshed {
                    Some(v) => (v, Some("verified".to_string())),
                    None => (value, Some("accepted".to_string())),
                };
                if let Some(from) = from {
                    self.offline_note(ch, &field, from, value.clone()).await;
                }
                out.push(Event::Param {
                    ch,
                    field: field.clone(),
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
                });
                self.project_param_dependents(ch, &field, writer, out).await;
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::ParamGuarded { .. } => unreachable!("guarded parameter normalized above"),
            // R2 desk fire-and-forget action: clear all solos (monitoring-side
            // only). No desk echo, the desk ACKs by silence, so fan out
            // explicit unsoloed frames for every known-soloed strip and drop
            // them from the cache; otherwise the UI would stick on soloed.
            // (A surface clear arrives the same way, via `apply_inbound`
            // solosw broadcasts.)
            Cmd::ClearSolo => {
                let drv = self.active_drv();
                if drv.clear_solo_scope() == Some("advertised") {
                    return self.clear_advertised_solos(writer, out).await;
                }
                let inputs_only = drv.clear_solo_scope() == Some("inputs");
                let count = if inputs_only {
                    drv.channel_count()
                } else {
                    drv.strip_count()
                };
                // Check the complete affected bank before the bulk provider write.
                for ch in 0..count as u16 {
                    policy.allow_strip(StripId(ch))?;
                    if let Some(holder) = self.lock_holder(ch) {
                        if holder != writer {
                            return Ok(Some(Self::locked_error(ch, &holder)));
                        }
                    }
                }
                drv.clear_solo().await?;
                let cleared: Vec<u16> = {
                    let mut sync = self.sync_state.lock().unwrap();
                    let mut cache = self.param_cache.write().unwrap();
                    let mut cleared = Vec::new();
                    for (ch, map) in cache.iter_mut() {
                        if inputs_only && usize::from(*ch) >= count {
                            continue;
                        }
                        if map.get("solo") == Some(&json!(true)) {
                            cleared.push(*ch);
                            map.remove("solo");
                            // Attach must not replay superseded Solo(true).
                            // The false echo below is display continuity,
                            // not a desk-verified replacement cache entry.
                            sync.typed.remove(&format!("{ch}/solo"));
                        }
                    }
                    cleared
                };
                for ch in cleared {
                    out.push(Event::Solo {
                        ch,
                        soloed: false,
                        key: self.active_strip_key(ch),
                    });
                }
                self.note_write(writer);
                Ok(None)
            }
            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
