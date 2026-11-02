//! Destination command arms.

use super::*;

impl Engine {
    // A provider with a bus strip family exposes the same masters through
    // strip commands. Admit both the target and known linked members before
    // using the native master write. Providers without that family retain
    // their native master-only behavior.
    async fn admit_bus_strip(
        &self,
        bus: u8,
        field: &str,
        value: Option<f32>,
        writer: &str,
        driver_id: &str,
        epoch: u64,
    ) -> anyhow::Result<Option<Event>> {
        let driver = self.drv(driver_id);
        let topology = driver.topology();
        if topology
            .iter()
            .any(|f| f.kind == openfader_core::StripKind::Bus)
        {
            let key = openfader_core::strip_key("bus", u16::from(bus) + 1);
            let index = openfader_core::strip_index_for_key(&topology, &key)
                .filter(|&i| i < driver.strip_count() && i <= u16::MAX as usize)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "bus {bus} out of range or strip key {key:?} unavailable on {driver_id}"
                    )
                })?;
            let target = StripId(index as u16);
            let linked = driver.control_members(target, field).await?;
            let members = std::iter::once(target).chain(linked);
            anyhow::ensure!(
                driver_id == self.active_driver()
                    && epoch == self.connection_gen.load(Ordering::Acquire)
                    && !self.reads_suspended.load(Ordering::Acquire),
                "Destination connection changed; reopen the control."
            );
            let policy = self.policy_of(writer);
            if let Some(value) = value {
                policy.allow_master_fader(value)?;
            } else {
                policy.allow_master_mute()?;
            }
            for member in members {
                if let Some(value) = value {
                    policy.allow_fader(member, value)?;
                } else {
                    policy.allow_mute(member)?;
                }
                if let Some(holder) = self.lock_holder(member.0) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(member.0, &holder)));
                    }
                }
            }
        } else {
            anyhow::ensure!(
                driver_id == self.active_driver()
                    && epoch == self.connection_gen.load(Ordering::Acquire)
                    && !self.reads_suspended.load(Ordering::Acquire),
                "Destination connection changed; reopen the control."
            );
            let policy = self.policy_of(writer);
            if let Some(value) = value {
                policy.allow_master_fader(value)?;
            } else {
                policy.allow_master_mute()?;
            }
        }
        Ok(None)
    }

    async fn dispatch_bus_master(
        &self,
        bus: u8,
        field: &str,
        value: ParamValue,
        writer: &str,
        driver_id: &str,
        epoch: u64,
    ) -> anyhow::Result<()> {
        let driver = self.drv(driver_id);
        if !driver.guarded_bus_writes() {
            return match (field, value) {
                ("fader", ParamValue::Float(value)) => driver.set_bus_fader(bus, value).await,
                ("mute", ParamValue::Bool(muted)) => driver.set_bus_mute(bus, muted).await,
                _ => anyhow::bail!("unsupported bus master control"),
            };
        }
        let key = openfader_core::strip_key("bus", u16::from(bus) + 1);
        let index = openfader_core::strip_index_for_key(&driver.topology(), &key)
            .ok_or_else(|| anyhow::anyhow!("bus {bus} out of range"))?;
        let strip = StripId(u16::try_from(index)?);
        let desired = value.clone();
        let check = |members: &[StripId]| -> anyhow::Result<()> {
            anyhow::ensure!(
                driver_id == self.active_driver()
                    && epoch == self.connection_gen.load(Ordering::Acquire)
                    && !self.reads_suspended.load(Ordering::Acquire),
                "mixer session changed"
            );
            let policy = self.policy_of(writer);
            match &desired {
                ParamValue::Float(value) => policy.allow_master_fader(*value)?,
                ParamValue::Bool(_) => policy.allow_master_mute()?,
                _ => anyhow::bail!("unsupported bus master control"),
            }
            for member in std::iter::once(&strip).chain(members) {
                match &desired {
                    ParamValue::Float(value) => policy.allow_fader(*member, *value)?,
                    ParamValue::Bool(_) => policy.allow_mute(*member)?,
                    _ => unreachable!(),
                }
                if let Some(holder) = self.lock_holder(member.0) {
                    anyhow::ensure!(holder == writer, "strip {} locked by {}", member.0, holder);
                }
            }
            Ok(())
        };
        driver
            .set_param_guarded_checked(strip, field, value, &BTreeMap::new(), &check)
            .await
    }

    async fn admit_assignment_members(
        &self,
        ch: u16,
        writer: &str,
    ) -> anyhow::Result<Option<Event>> {
        let id = self.active_driver();
        let epoch = self.connection_gen.load(Ordering::Acquire);
        let driver = self.drv(&id);
        let members = driver.control_members(StripId(ch), "assign").await?;
        anyhow::ensure!(
            id == self.active_driver()
                && epoch == self.connection_gen.load(Ordering::Acquire)
                && !self.reads_suspended.load(Ordering::Acquire),
            "Destination connection changed; reopen the control."
        );
        let policy = self.policy_of(writer);
        for member in std::iter::once(StripId(ch)).chain(members) {
            policy.allow_assign(member)?;
            if let Some(holder) = self.lock_holder(member.0) {
                if holder != writer {
                    return Ok(Some(Self::locked_error(member.0, &holder)));
                }
            }
        }
        Ok(None)
    }

    pub(super) async fn apply_destination(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
        policy: &Policy,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            // R2 mix-bus masters (0-based `bus` over `0..bus_count()`).
            Cmd::DestinationParam {
                request_id,
                target,
                control,
                value,
                expected,
                epoch,
                guard,
            } => {
                let mut applied = None;
                let result: anyhow::Result<()> = async {
                    anyhow::ensure!(
                        !request_id.is_empty() && request_id.len() <= 128,
                        "Invalid destination request ID."
                    );
                    anyhow::ensure!(
                        !self.reads_suspended.load(Ordering::Acquire)
                            && epoch == self.connection_gen.load(Ordering::Acquire),
                        "Destination connection changed; reopen the control."
                    );
                    anyhow::ensure!(
                        value.is_finite() && expected.is_finite(),
                        "Destination values must be finite."
                    );
                    let driver = self.active_drv();
                    let controls = driver.destination_controls();
                    let row = controls
                        .iter()
                        .find(|row| row.target == target && row.id == control)
                        .ok_or_else(|| anyhow::anyhow!("Destination control unavailable."))?;
                    if !matches!(row.surface.as_str(), "solo" | "input_insert") {
                        policy.allow_master()?;
                    }
                    anyhow::ensure!(
                        row.allows_value(value),
                        "Destination value is not advertised."
                    );
                    anyhow::ensure!(
                        row.current
                            .as_ref()
                            .is_some_and(|current| current.value == expected),
                        "Destination changed; reopen the control."
                    );
                    anyhow::ensure!(
                        (!matches!(
                            row.surface.as_str(),
                            "global"
                                | "solo"
                                | "fx"
                                | "routing"
                                | "input_insert"
                                | "automix_settings"
                                | "bus_link"
                                | "link_policy"
                                | "usb_interface"
                                | "safe_levels"
                                | "clock_rate"
                                | "midi_setup"
                                | "mute_behavior"
                        ) && !row.surface.starts_with("output_")
                            && row.gesture.is_none())
                            || guard.is_some(),
                        "Shared control requires the current configuration guard."
                    );
                    if let Some(guard) = guard {
                        let block: Vec<_> = controls
                            .iter()
                            .filter(|r| r.target == target && r.surface == row.surface)
                            .cloned()
                            .collect();
                        let serialized = Self::destination_rows(&block);
                        anyhow::ensure!(
                            (row.preset
                                || matches!(
                                    row.surface.as_str(),
                                    "global"
                                        | "solo"
                                        | "fx"
                                        | "routing"
                                        | "input_insert"
                                        | "automix_settings"
                                        | "bus_link"
                                        | "link_policy"
                                        | "usb_interface"
                                        | "safe_levels"
                                        | "clock_rate"
                                        | "midi_setup"
                                        | "mute_behavior"
                                )
                                || row.surface.starts_with("output_")
                                || row.gesture.is_some())
                                && row.revision.is_some()
                                && row.generation.is_some()
                                && !guard.is_empty()
                                && guard.len() <= 64
                                && guard == serialized,
                            "Shared control changed; reopen the control."
                        );
                        let receipt = if row.surface == "solo" {
                            self.check_solo_scope(&row.members, writer)?;
                            let driver_id = self.active_driver();
                            let context = || {
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire)
                                    && self.check_solo_scope(&row.members, writer).is_ok()
                            };
                            Some(
                                driver
                                    .set_target_solo(
                                        &target, &control, value, expected, &block, &context,
                                    )
                                    .await?,
                            )
                        } else if row.surface == "input_insert" {
                            self.check_solo_scope(&row.members, writer)?;
                            let driver_id = self.active_driver();
                            let context = || {
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire)
                                    && self.check_solo_scope(&row.members, writer).is_ok()
                            };
                            driver
                                .set_output_processor_guarded(
                                    &target, &control, value, expected, &block, &context,
                                )
                                .await?
                        } else if matches!(
                            row.surface.as_str(),
                            "bus_link"
                                | "link_policy"
                                | "usb_interface"
                                | "safe_levels"
                                | "midi_setup"
                                | "mute_behavior"
                                | "clock_rate"
                        ) {
                            let driver_id = self.active_driver();
                            let topology = driver.topology();
                            let members = row
                                .members
                                .iter()
                                .map(|key| {
                                    let index = openfader_core::strip_index_for_key(&topology, key)
                                        .ok_or_else(|| {
                                            anyhow::anyhow!("Shared mixer scope unavailable")
                                        })?;
                                    anyhow::ensure!(
                                        row.surface != "bus_link"
                                            || topology.iter().any(|family| family.kind
                                                == openfader_core::StripKind::Bus
                                                && index >= family.first
                                                && index < family.first + family.count),
                                        "Bus link member is not a bus"
                                    );
                                    Ok(StripId(u16::try_from(index)?))
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            if row.surface != "bus_link" {
                                let roster = (0..driver.strip_count())
                                    .map(|index| {
                                        openfader_core::strip_key_for_index(&topology, index)
                                    })
                                    .collect::<Option<Vec<_>>>()
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("Shared mixer scope unavailable")
                                    })?;
                                anyhow::ensure!(
                                    !roster.is_empty() && row.members == roster,
                                    "Complete mixer scope required"
                                );
                            } else {
                                anyhow::ensure!(
                                    members.len() == 2 && members[0] != members[1],
                                    "Shared mixer scope unavailable"
                                );
                            }
                            let check = || -> anyhow::Result<()> {
                                anyhow::ensure!(
                                    driver_id == self.active_driver()
                                        && epoch == self.connection_gen.load(Ordering::Acquire)
                                        && !self.reads_suspended.load(Ordering::Acquire),
                                    "Shared mixer session changed"
                                );
                                if row.surface == "clock_rate" && value != expected {
                                    let main = topology
                                        .iter()
                                        .find(|family| {
                                            family.kind == openfader_core::StripKind::Main
                                        })
                                        .ok_or_else(|| {
                                            anyhow::anyhow!("Main LR state unavailable")
                                        })?;
                                    anyhow::ensure!(
                                        self.param_cache
                                            .read()
                                            .unwrap()
                                            .get(&(main.first as u16))
                                            .and_then(|params| params.get("mute"))
                                            .and_then(Value::as_bool)
                                            == Some(true),
                                        "Mute Main LR before changing clock rate"
                                    );
                                }
                                let policy = self.policy_of(writer);
                                if row.surface == "clock_rate" {
                                    policy.allow_master_mute()?;
                                }
                                // Shared device reconfiguration affects the full mixer.
                                // Require full-range mixer access at dispatch.
                                policy.allow_master_fader(1.0)?;
                                for member in &members {
                                    policy.allow_assign(*member)?;
                                    policy.allow_rename(*member)?;
                                    policy.allow_fader(*member, 1.0)?;
                                    anyhow::ensure!(
                                        self.lock_holder(member.0)
                                            .is_none_or(|holder| holder == writer),
                                        "Shared mixer member locked"
                                    );
                                }
                                Ok(())
                            };
                            check()?;
                            driver
                                .set_output_processor_guarded(
                                    &target,
                                    &control,
                                    value,
                                    expected,
                                    &block,
                                    &|| check().is_ok(),
                                )
                                .await?
                        } else if row.surface.starts_with("output_")
                            || row.surface == "automix_settings"
                        {
                            let driver_id = self.active_driver();
                            let mut members = row.members.clone();
                            let topology = driver.topology();
                            for key in &row.members {
                                if let Some(index) =
                                    openfader_core::strip_index_for_key(&topology, key)
                                {
                                    for member in driver
                                        .control_members(StripId(index as u16), &control)
                                        .await?
                                    {
                                        if let Some(key) = openfader_core::strip_key_for_index(
                                            &topology,
                                            member.0 as usize,
                                        ) {
                                            if !members.contains(&key) {
                                                members.push(key);
                                            }
                                        }
                                    }
                                }
                            }
                            anyhow::ensure!(
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire),
                                "Destination connection changed; reopen the control."
                            );
                            if !members.is_empty() {
                                self.check_solo_scope(&members, writer)?;
                            }
                            let context = || {
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire)
                                    && self.policy_of(writer).allow_master().is_ok()
                                    && (|| -> anyhow::Result<()> {
                                        let mut current = Vec::new();
                                        for key in &row.members {
                                            let Some(index) =
                                                openfader_core::strip_index_for_key(&topology, key)
                                            else {
                                                current.push(key.clone());
                                                continue;
                                            };
                                            match driver.current_control_members(
                                                StripId(index as u16),
                                                &control,
                                            )? {
                                                Some(companions) => {
                                                    for member in companions {
                                                        let key =
                                                            openfader_core::strip_key_for_index(
                                                                &topology,
                                                                member.0 as usize,
                                                            )
                                                            .ok_or_else(|| {
                                                                anyhow::anyhow!(
                                                                    "Missing control member"
                                                                )
                                                            })?;
                                                        if !current.contains(&key) {
                                                            current.push(key);
                                                        }
                                                    }
                                                }
                                                None => {
                                                    return self.check_solo_scope(&members, writer)
                                                }
                                            }
                                        }
                                        if current.is_empty() {
                                            // Global controls have no strip members;
                                            // master policy was checked above.
                                            Ok(())
                                        } else {
                                            self.check_solo_scope(&current, writer)
                                        }
                                    })()
                                    .is_ok()
                            };
                            driver
                                .set_output_processor_guarded(
                                    &target, &control, value, expected, &block, &context,
                                )
                                .await?
                        } else if row.surface == "routing" {
                            let driver_id = self.active_driver();
                            let context = || {
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire)
                                    && self.policy_of(writer).allow_master().is_ok()
                            };
                            Some(
                                driver
                                    .set_routing_control_guarded(
                                        &target, &control, value, expected, &block, &context,
                                    )
                                    .await?,
                            )
                        } else if row.surface == "fx" {
                            self.check_solo_scope(std::slice::from_ref(&target), writer)?;
                            let driver_id = self.active_driver();
                            let context = || {
                                driver_id == self.active_driver()
                                    && epoch == self.connection_gen.load(Ordering::Acquire)
                                    && !self.reads_suspended.load(Ordering::Acquire)
                                    && self
                                        .check_solo_scope(std::slice::from_ref(&target), writer)
                                        .is_ok()
                            };
                            Some(
                                driver
                                    .set_fx_control_guarded(
                                        &target, &control, value, expected, &block, &context,
                                    )
                                    .await?,
                            )
                        } else {
                            driver
                                .set_destination_control_guarded(
                                    &target, &control, value, expected, &block,
                                )
                                .await?
                        };
                        if let Some(receipt) = receipt {
                            anyhow::ensure!(
                                receipt.target == target
                                    && receipt.id == control
                                    && receipt.surface == row.surface
                                    && receipt.gesture == row.gesture
                                    && receipt.revision != row.revision
                                    && receipt
                                        .current
                                        .as_ref()
                                        .is_some_and(|v| v.verified && v.value == value)
                                    && receipt.revision.is_some()
                                    && receipt.generation == row.generation,
                                "Invalid guarded-write receipt; result uncertain"
                            );
                            applied = Self::destination_rows(&[receipt]).into_iter().next();
                        }
                        anyhow::ensure!(
                            row.gesture.is_none() || applied.is_some(),
                            "Gesture write lacks independent receipt; result uncertain"
                        );
                    } else {
                        driver
                            .set_destination_control(&target, &control, value, expected)
                            .await?;
                    }
                    anyhow::ensure!(
                        epoch == self.connection_gen.load(Ordering::Acquire),
                        "Destination connection changed while writing; result uncertain."
                    );
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.extend(self.destination_controls_message());
                    Ok(())
                }
                .await;
                let accepted = result.is_ok();
                let mut message = result.err().map(|error| error.to_string());
                if accepted
                    && self.active_drv().destination_controls().iter().any(|row| {
                        row.target == target
                            && row.id == control
                            && row
                                .current
                                .as_ref()
                                .is_some_and(|current| current.verified && current.value != value)
                    })
                {
                    message = Some(
                        "Command accepted, but the latest mixer value differs. Reopen the control."
                            .to_string(),
                    );
                }
                Ok(Some(Event::DestinationControlResult {
                    applied: if accepted { applied } else { None },
                    request_id,
                    epoch,
                    accepted,
                    message,
                }))
            }
            Cmd::BusFader { bus, value } => {
                let value = openfader_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                // Preserve the sibling for read failures. As in
                // `refresh_param`, a disagreeing immediate read can be the
                // desk's pre-write value; echo the submitted value instead.
                // This bus display frame does not establish verified proof.
                let driver_id = self.active_driver();
                let epoch = self.connection_gen.load(Ordering::Acquire);
                let driver = self.drv(&driver_id);
                let muted = driver.get_bus_mute(bus).await.ok();
                if let Some(error) = self
                    .admit_bus_strip(bus, "fader", Some(value), writer, &driver_id, epoch)
                    .await?
                {
                    return Ok(Some(error));
                }
                self.dispatch_bus_master(
                    bus,
                    "fader",
                    ParamValue::Float(value),
                    writer,
                    &driver_id,
                    epoch,
                )
                .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.bus_message(bus).await, muted) {
                    (Ok(mut m), _) => {
                        match &mut m {
                            Event::Bus { fader, .. }
                                if !refresh_agrees(&json!(*fader), &json!(value)) =>
                            {
                                *fader = value;
                            }
                            _ => {}
                        }
                        m
                    }
                    (Err(_), Some(muted)) => Event::Bus {
                        bus,
                        fader: value,
                        muted,
                        key: Some(openfader_core::strip_key(
                            "bus",
                            bus.saturating_add(1) as u16,
                        )),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            Cmd::BusMute { bus, muted } => {
                policy.allow_master_mute()?;
                let driver_id = self.active_driver();
                let epoch = self.connection_gen.load(Ordering::Acquire);
                let driver = self.drv(&driver_id);
                let fader = driver.get_bus_fader(bus).await.ok();
                if let Some(error) = self
                    .admit_bus_strip(bus, "mute", None, writer, &driver_id, epoch)
                    .await?
                {
                    return Ok(Some(error));
                }
                self.dispatch_bus_master(
                    bus,
                    "mute",
                    ParamValue::Bool(muted),
                    writer,
                    &driver_id,
                    epoch,
                )
                .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.bus_message(bus).await, fader) {
                    (Ok(mut m), _) => {
                        // Bool agreement is exact; retain the accepted
                        // write on a stale read, without caching it as proof.
                        if let Event::Bus { muted: read, .. } = &mut m {
                            *read = muted;
                        }
                        m
                    }
                    (Err(_), Some(fader)) => Event::Bus {
                        bus,
                        fader,
                        muted,
                        key: Some(openfader_core::strip_key(
                            "bus",
                            bus.saturating_add(1) as u16,
                        )),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            // R2 subgroup masters (0-based `group` over `0..subgroup_count()`).
            Cmd::SubgroupFader { group, value } => {
                let value = openfader_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                let muted = self.active_drv().get_subgroup_mute(group).await.ok();
                self.active_drv().set_subgroup_fader(group, value).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.subgroup_message(group).await, muted) {
                    (Ok(m), _) => m,
                    (Err(_), Some(muted)) => Event::Subgroup {
                        group,
                        fader: value,
                        muted,
                        key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            Cmd::SubgroupMute { group, muted } => {
                policy.allow_master_mute()?;
                let fader = self.active_drv().get_subgroup_fader(group).await.ok();
                self.active_drv().set_subgroup_mute(group, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.subgroup_message(group).await, fader) {
                    (Ok(m), _) => m,
                    (Err(_), Some(fader)) => Event::Subgroup {
                        group,
                        fader,
                        muted,
                        key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            // R2 group membership (channel-scoped lock + master gate).
            Cmd::SubgroupAssign {
                ch,
                group,
                member,
                key,
            } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                if let Some(error) = self.admit_assignment_members(ch, writer).await? {
                    return Ok(Some(error));
                }
                self.active_drv()
                    .set_subgroup_assign(StripId(ch), group, member)
                    .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(Event::SubgroupAssign {
                    ch,
                    group,
                    member,
                    key: self.active_strip_key(ch),
                });
                Ok(None)
            }
            Cmd::MuteGroup { group, muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_mute_group(group, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Display-continuity echo: the inclusive getter already
                // carries own writes, so fall back to the submitted value
                // only when nothing was ever pushed (no fabrication: this IS
                // the submitted value, and these frames carry no `authority`
                // field, see the `param` event).
                let muted = self
                    .active_drv()
                    .get_mute_group(group)
                    .await
                    .unwrap_or(muted);
                out.push(Event::MuteGroup {
                    group,
                    muted,
                    key: Some(format!("mgroup/{:02}", group.saturating_add(1))),
                });
                Ok(None)
            }
            Cmd::MuteGroupAssign {
                ch,
                group,
                member,
                key,
            } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                if let Some(error) = self.admit_assignment_members(ch, writer).await? {
                    return Ok(Some(error));
                }
                let id = self.active_driver();
                let epoch = self.connection_gen.load(Ordering::Acquire);
                let driver = self.drv(&id);
                let check = |members: &[StripId]| -> anyhow::Result<()> {
                    anyhow::ensure!(
                        id == self.active_driver()
                            && epoch == self.connection_gen.load(Ordering::Acquire)
                            && !self.reads_suspended.load(Ordering::Acquire),
                        "mixer session changed"
                    );
                    let policy = self.policy_of(writer);
                    for member in std::iter::once(&StripId(ch)).chain(members) {
                        policy.allow_assign(*member)?;
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
                driver
                    .set_mute_group_assign_checked(StripId(ch), group, member, &check)
                    .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(Event::MuteGroupAssign {
                    ch,
                    group,
                    member,
                    key: self.active_strip_key(ch),
                });
                Ok(None)
            }

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
