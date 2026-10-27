//! Destinations operations.

use super::*;

impl Engine {
    // ================= R2: routing =================

    /// `caps.routing` for the named driver (design racks fully supported;
    /// elsewhere unsupported).
    pub(super) fn routing_caps(&self, driver: &str) -> crate::protocol::RoutingCaps {
        let groups: Vec<crate::protocol::RoutingGroupCaps> = self
            .drv(driver)
            .routing_groups()
            .iter()
            .map(|g| {
                // `RoutingGroup` fields project 1:1 onto the wire caps.
                crate::protocol::RoutingGroupCaps {
                    key: g.key.to_string(),
                    label: g.label.to_string(),
                    count: g.count,
                }
            })
            .collect();
        let planes = self.drv(driver).routing_planes();
        let supported = planes.iter().any(|p| !p.destinations.is_empty());
        let per_strip = !groups.is_empty();
        crate::protocol::RoutingCaps {
            planes,
            epoch: Some(self.connection_gen.load(Ordering::Acquire)),
            targets: self
                .drv(driver)
                .routing_targets()
                .iter()
                .filter(|t| !t.key.is_empty())
                .map(|t| crate::protocol::RoutingTargetCaps {
                    key: t.key.to_string(),
                    label: t.label.to_string(),
                    clearable: t.clearable,
                })
                .collect(),
            groups,
            per_strip,
            supported,
        }
    }

    /// R2 `caps.groups`: master-section counts of the named driver (bus /
    /// subgroup / mute-group masters behind the new commands; zero =
    /// honest `unsupported`). Assign-leaf support rides alongside so the
    /// UI collapses assign toggles where the desk has masters only.
    pub(super) fn groups_caps(&self, driver: &str) -> crate::protocol::GroupsCaps {
        let drv = self.drv(driver);
        let (mg_assign, sub_assign) = drv.group_assign_caps();
        crate::protocol::GroupsCaps {
            clear_solo_scope: drv.clear_solo_scope().map(str::to_string),
            buses: drv.bus_count(),
            subgroups: drv.subgroup_count(),
            mute_groups: drv.mute_group_count(),
            mg_assign,
            sub_assign,
        }
    }

    /// R2 `caps.files`: desk file-transfer support of the named driver.
    pub(super) fn files_caps(&self, driver: &str) -> crate::protocol::FilesCaps {
        let supported = self.drv(driver).file_transfer();
        crate::protocol::FilesCaps { supported }
    }

    /// `caps.native_snapshots`: read-only native slot support of the named
    /// driver (`supported:false` where the driver models no native slots
    /// skipped on the wire there, so old frames stay byte-identical).
    pub(super) fn native_caps(&self, driver: &str) -> NativeSnapshotsCaps {
        match self.drv(driver).native_snapshots() {
            Some(_) => NativeSnapshotsCaps {
                supported: true,
                read_only: true,
            },
            None => NativeSnapshotsCaps {
                supported: false,
                read_only: false,
            },
        }
    }

    /// Serialize provider-owned destination domains without mixer-name logic.
    pub(super) fn destination_rows(
        controls: &[openfader_core::destinations::DestinationControl],
    ) -> Vec<Value> {
        controls.iter().map(|control| {
            let mut row = json!({
            "revision": control.revision, "generation": control.generation, "preset": control.preset,
            "members": control.members,
            "target": control.target, "target_label": control.target_label,
            "target_name": control.target_name, "send_slot": control.send_slot,
            "surface": control.surface, "description": control.description,
            "id": control.id, "label": control.label, "scope": "destination",
            "choices": control.choices.iter().map(|choice| json!({"value": choice.value, "label": choice.label})).collect::<Vec<_>>(),
            "range": control.range.as_ref().map(|range| json!({"min": range.min, "max": range.max, "unit": range.unit, "decimals": range.decimals})),
            "value": control.current.as_ref().map(|value| value.value),
            "authority": control.current.as_ref().map(|value| if value.verified { "verified" } else { "accepted" }),
            });
            if let Some(gesture) = control.gesture { row["gesture"] = json!(gesture); }
            if let Some(step) = control.range.as_ref().and_then(|range| range.step) { row["range"]["step"] = json!(step); }
            if control.tap_tempo { row["tap_tempo"] = json!(true); }
            row
        }).collect()
    }

    pub(super) fn destination_controls_message(&self) -> Option<Event> {
        let epoch = self.connection_gen.load(Ordering::Acquire);
        let controls = self.active_drv().destination_controls();
        (!controls.is_empty()).then(|| Event::DestinationControls {
            epoch,
            controls: Self::destination_rows(&controls),
        })
    }

    /// Current native slot view, with the engine epoch sampled first.
    pub(super) fn native_snapshots_message(&self) -> Event {
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active_driver();
        Self::native_message_for(&driver, &self.drv(&driver).native_snapshots(), generation)
    }

    /// Build the `native_snapshots` event for `driver`'s hook view:
    /// `unsupported` (not modelled, never faked), `unavailable`
    /// (modelled but no live desk session), `loading` (live session, no
    /// slot data pushed yet) or `ready` (push-observed PARTIAL metadata
    /// only, never a full inventory). `current` stays `None` until a
    /// desk recall is observed, never a slot-0 default. `epoch` is the
    /// ENGINE connection generation (matches `SyncEpoch`, NOT the RTA
    /// epoch) and is populated at every constructor. Read-only: no
    /// command path writes native slots.
    pub(super) fn native_message_for(
        driver: &str,
        view: &Option<NativeSnapshots>,
        epoch: u64,
    ) -> Event {
        match view {
            None => Event::NativeSnapshots {
                driver: driver.to_string(),
                supported: false,
                read_only: false,
                status: "unsupported".to_string(),
                slots: Vec::new(),
                current: None,
                epoch,
            },
            Some(v) => Event::NativeSnapshots {
                driver: driver.to_string(),
                supported: true,
                read_only: true,
                status: if !v.connected {
                    "unavailable"
                } else if !v.received {
                    "loading"
                } else {
                    "ready"
                }
                .to_string(),
                slots: v
                    .slots
                    .iter()
                    .map(|s| NativeSnapshotEntry {
                        slot: s.slot,
                        name: s.name.clone(),
                    })
                    .collect(),
                current: v.current,
                epoch,
            },
        }
    }

    /// Record the just-emitted native view so the ticker stays quiet until
    /// it actually changes (see [`native_last`](Engine::native_last)).
    /// The generation is captured BEFORE the driver view so a racing
    /// connect invalidates this record.
    pub(super) fn record_native_view(&self) {
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active_driver();
        let view = self.drv(&driver).native_snapshots();
        *self.native_last.write().unwrap() = (generation, view);
    }

    /// Full-state `bus` echo (DISPLAY continuity, not proof): both master
    /// fields re-read via the inclusive display getters. No `authority`
    /// field; never used for preset capture.
    pub(super) async fn bus_message(&self, bus: u8) -> anyhow::Result<Event> {
        let drv = self.active_drv();
        let (fader, muted) = (drv.get_bus_fader(bus).await?, drv.get_bus_mute(bus).await?);
        Ok(Event::Bus {
            bus,
            fader,
            muted,
            key: Some(openfader_core::strip_key(
                "bus",
                bus.saturating_add(1) as u16,
            )),
        })
    }

    /// Full-state `subgroup` echo (DISPLAY continuity, not proof, see
    /// [`bus_message`](Self::bus_message)).
    pub(super) async fn subgroup_message(&self, group: u8) -> anyhow::Result<Event> {
        let drv = self.active_drv();
        let (fader, muted) = (
            drv.get_subgroup_fader(group).await?,
            drv.get_subgroup_mute(group).await?,
        );
        Ok(Event::Subgroup {
            group,
            fader,
            muted,
            key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
        })
    }

    /// Full-state `main` echo (DISPLAY continuity, not proof): Main LR fader
    /// and mute re-read from the active driver inclusive getters (own writes
    /// included so a new attach sees locally moved masters). These frames
    /// carry no `authority` field and are never used for preset capture
    /// see the `param` event for verified values.
    pub(super) async fn main_message(&self) -> Event {
        let drv = self.active_drv();
        let (fader, muted) = (
            drv.get_main_fader().await.ok(),
            drv.get_main_mute().await.ok(),
        );
        Event::Main { fader, muted }
    }

    /// Master-section + membership snapshot for attach/post_switch (DISPLAY
    /// continuity, not proof): every bus/subgroup/mute-group master re-read
    /// live via the inclusive display getters (own writes included so a new
    /// attach sees locally moved masters indefinitely), every known
    /// membership (sparse, members only), Main LR, and sparse solos. These
    /// frames carry no `authority` field and are never used for preset
    /// capture, see the `param`/`params` events for verified values. Fresh pages
    /// learn existing assignments without a write (gap: the engine used to
    /// broadcast these only as write echoes). Assign/solo reads are
    /// driver-gated: sim/dl32s serve assigns from memory/cache (xair has
    /// no assign leaf at all); sim solos come from memory while xair
    /// re-reads the live solo bank per channel strip. All reads are
    /// best-effort: a failed re-read drops that frame instead of failing
    /// the snapshot.
    pub(super) async fn masters_snapshot(&self) -> Vec<Event> {
        let mut out = Vec::new();
        let driver = self.active_driver();
        let drv = self.drv(&driver);
        let (buses, subgroups, mute_groups) = (
            drv.bus_count(),
            drv.subgroup_count(),
            drv.mute_group_count(),
        );
        for bus in 0..buses {
            let msg = match (drv.get_bus_fader(bus).await, drv.get_bus_mute(bus).await) {
                (Ok(fader), Ok(muted)) => Some(Event::Bus {
                    bus,
                    fader,
                    muted,
                    key: Some(openfader_core::strip_key(
                        "bus",
                        bus.saturating_add(1) as u16,
                    )),
                }),
                _ => None,
            };
            if let Some(m) = msg {
                out.push(m);
            }
        }
        for group in 0..subgroups {
            let msg = match (
                drv.get_subgroup_fader(group).await,
                drv.get_subgroup_mute(group).await,
            ) {
                (Ok(fader), Ok(muted)) => Some(Event::Subgroup {
                    group,
                    fader,
                    muted,
                    key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                }),
                _ => None,
            };
            if let Some(m) = msg {
                out.push(m);
            }
        }
        for group in 0..mute_groups {
            let muted = drv.get_mute_group(group).await.ok();
            if let Some(muted) = muted {
                out.push(Event::MuteGroup {
                    group,
                    muted,
                    key: Some(format!("mgroup/{:02}", group.saturating_add(1))),
                });
            }
        }
        out.push(self.main_message().await);
        // Membership: every known true and false. Reads are memory/cache-speed
        // on drivers that serve the assign leaf; elsewhere the default
        // reads honestly fail and no members are reported. Drivers with a
        // bulk source (in-memory maps) serve the whole scan from one lock
        // acquisition; the frame set and order match the per-strip reads
        // exactly, so consumers see no difference.
        if drv.group_assign_caps() != (false, false) {
            let topo = self.driver_topology(&driver);
            let strips = self.driver_strips(&driver);
            let sub_bulk = drv.sub_assign_bulk();
            let mg_bulk = drv.mg_assign_bulk();
            let solo_bulk = if !drv.is_hardware() {
                drv.solo_bulk()
            } else {
                None
            };
            for ch in 0..strips as u16 {
                let id = StripId(ch);
                let key = openfader_core::strip_key_for_index(&topo, ch as usize);
                for group in 0..subgroups {
                    let member = match &sub_bulk {
                        Some(map) => Some(map.get(&(ch, group)).copied().unwrap_or(false)),
                        None => drv.get_subgroup_assign(id, group).await.ok(),
                    };
                    if let Some(member) = member {
                        out.push(Event::SubgroupAssign {
                            ch,
                            group,
                            member,
                            key: key.clone(),
                        });
                    }
                }
                for group in 0..mute_groups {
                    let member = match &mg_bulk {
                        Some(map) => Some(map.get(&(ch, group)).copied().unwrap_or(false)),
                        None => drv.get_mute_group_assign(id, group).await.ok(),
                    };
                    if let Some(member) = member {
                        out.push(Event::MuteGroupAssign {
                            ch,
                            group,
                            member,
                            key: key.clone(),
                        });
                    }
                }
            }
            // Sparse solos served from driver memory (sim in-memory;
            // dl32s solos ride echoes, so no rows here).
            if !drv.is_hardware() {
                for ch in 0..strips as u16 {
                    let soloed = match &solo_bulk {
                        Some(map) => map.get(&ch).copied().unwrap_or(false),
                        None => drv.get_solo(StripId(ch)).await.unwrap_or(false),
                    };
                    {
                        out.push(Event::Solo {
                            ch,
                            soloed,
                            key: openfader_core::strip_key_for_index(&topo, ch as usize),
                        });
                    }
                }
            }
        }
        // Sparse desk-side solos: best-effort live bank re-read (drivers
        // whose solos live in the desk) so a fresh attach (or driver
        // switch) learns them without waiting for the poll sweep. One bank
        // query fanned out per channel (not one query per channel), cached
        // alongside (the poll diff then stays quiet); a silent desk yields
        // no frames, never fails the snapshot.
        if drv.live_solo_bank() {
            let topo = self.driver_topology(&driver);
            let started = Instant::now();
            if let Ok(bank) = drv.solo_bank(drv.channel_count()).await {
                for (ch, soloed) in bank.iter().enumerate() {
                    let ch = ch as u16;
                    self.apply_received(
                        ch,
                        "solo",
                        &ParamValue::Bool(*soloed),
                        Instant::now(),
                        Some(started),
                    );
                    out.push(Event::Solo {
                        ch,
                        soloed: *soloed,
                        key: openfader_core::strip_key_for_index(&topo, ch as usize),
                    });
                }
            }
        }
        out
    }
}
