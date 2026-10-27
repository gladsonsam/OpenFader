//! Destination command arms.

use super::*;

impl Engine {
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
                    if row.surface != "solo" { policy.allow_master()?; }
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
                        (!matches!(row.surface.as_str(), "global" | "solo" | "fx") && row.gesture.is_none()) || guard.is_some(),
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
                            (row.preset || matches!(row.surface.as_str(), "global" | "solo" | "fx") || row.gesture.is_some())
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
                            let context = || driver_id == self.active_driver()
                                && epoch == self.connection_gen.load(Ordering::Acquire)
                                && !self.reads_suspended.load(Ordering::Acquire)
                                && self.check_solo_scope(&row.members, writer).is_ok();
                            Some(driver.set_target_solo(&target, &control, value, expected, &block, &context).await?)
                        } else if row.surface == "fx" {
                            self.check_solo_scope(std::slice::from_ref(&target), writer)?;
                            let driver_id = self.active_driver();
                            let context = || driver_id == self.active_driver()
                                && epoch == self.connection_gen.load(Ordering::Acquire)
                                && !self.reads_suspended.load(Ordering::Acquire)
                                && self.check_solo_scope(std::slice::from_ref(&target), writer).is_ok();
                            Some(driver.set_fx_control_guarded(&target, &control, value, expected, &block, &context).await?)
                        } else { driver
                            .set_destination_control_guarded(
                                &target, &control, value, expected, &block,
                            )
                            .await? };
                        if let Some(receipt) = receipt {
                            anyhow::ensure!(receipt.target == target && receipt.id == control
                                && receipt.surface == row.surface && receipt.gesture == row.gesture
                                && receipt.revision != row.revision
                                && receipt.current.as_ref().is_some_and(|v| v.verified && v.value == value)
                                && receipt.revision.is_some() && receipt.generation == row.generation,
                                "Invalid guarded-write receipt; result uncertain");
                            applied = Self::destination_rows(&[receipt]).into_iter().next();
                        }
                        anyhow::ensure!(row.gesture.is_none() || applied.is_some(),
                            "Gesture write lacks independent receipt; result uncertain");
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
                let muted = self.active_drv().get_bus_mute(bus).await.ok();
                self.active_drv().set_bus_fader(bus, value).await?;
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
                let fader = self.active_drv().get_bus_fader(bus).await.ok();
                self.active_drv().set_bus_mute(bus, muted).await?;
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
                policy.allow_assign(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
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
                policy.allow_assign(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                self.active_drv()
                    .set_mute_group_assign(StripId(ch), group, member)
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
