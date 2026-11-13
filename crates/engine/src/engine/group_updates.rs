use super::*;

pub(super) fn master_field(param: &str) -> Option<(&str, &str)> {
    let (family, field) = param.strip_prefix('_')?.rsplit_once('_')?;
    (matches!(family, "main" | "bus" | "subgroup") && matches!(field, "fader" | "mute"))
        .then_some((family, field))
}

impl Engine {
    pub(super) fn group_update_event(
        &self,
        ch: u16,
        param: &str,
        pv: &ParamValue,
    ) -> Option<Event> {
        if let Some((family, field)) = master_field(param) {
            let (fader, muted) = match (field, pv) {
                ("fader", ParamValue::Float(value)) if value.is_finite() => (Some(*value), None),
                ("mute", ParamValue::Bool(value)) => (None, Some(*value)),
                _ => return None,
            };
            let target = if family == "main" {
                family.to_string()
            } else {
                format!("{family}/{:02}", ch + 1)
            };
            return Some(Event::Master {
                target,
                fader,
                muted,
            });
        }
        if param == "_mute_group_name" {
            if let ParamValue::Text(name) = pv {
                Some(Event::MuteGroupName {
                    group: ch as u8,
                    name: name.clone(),
                })
            } else {
                None
            }
        } else if param == "_mute_group" {
            pv_bool(pv).map(|muted| Event::MuteGroup {
                group: ch as u8,
                muted,
                key: Some(format!("mgroup/{:02}", ch + 1)),
            })
        } else if let Some(group) = param
            .strip_prefix("_mg")
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|g| *g > 0)
        {
            pv_bool(pv).map(|member| Event::MuteGroupAssign {
                ch,
                group: group - 1,
                member,
                key: self.active_strip_key(ch),
            })
        } else if let Some(group) = param
            .strip_prefix("_sg")
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|g| *g > 0)
        {
            pv_bool(pv).map(|member| Event::SubgroupAssign {
                ch,
                group: group - 1,
                member,
                key: self.active_strip_key(ch),
            })
        } else if param == "solo" {
            pv_bool(pv).map(|soloed| Event::Solo {
                ch,
                soloed,
                key: self.active_strip_key(ch),
            })
        } else {
            None
        }
    }
}

impl Engine {
    pub(super) fn reconcile_group_event(
        &self,
        event: &mut Event,
        fresh: &dyn Fn(u16, &str) -> Option<Value>,
    ) {
        let number = |index, field: &str| {
            fresh(index, field)
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
        };
        let boolean = |index, field: &str| fresh(index, field).and_then(|v| v.as_bool());
        match event {
            Event::Main { fader, muted } => {
                if let Some(value) = number(0, "_main_fader") {
                    *fader = Some(value);
                }
                if let Some(value) = boolean(0, "_main_mute") {
                    *muted = Some(value);
                }
            }
            Event::State { main_fader, .. } => {
                if let Some(value) = number(0, "_main_fader") {
                    *main_fader = Some(value);
                }
            }
            Event::Bus {
                bus, fader, muted, ..
            } => {
                if let Some(value) = number(*bus as u16, "_bus_fader") {
                    *fader = value;
                }
                if let Some(value) = boolean(*bus as u16, "_bus_mute") {
                    *muted = value;
                }
            }
            Event::Subgroup {
                group,
                fader,
                muted,
                ..
            } => {
                if let Some(value) = number(*group as u16, "_subgroup_fader") {
                    *fader = value;
                }
                if let Some(value) = boolean(*group as u16, "_subgroup_mute") {
                    *muted = value;
                }
            }
            Event::SubgroupAssign {
                ch, group, member, ..
            } => {
                if let Some(value) = boolean(*ch, &format!("_sg{}", *group + 1)) {
                    *member = value;
                }
            }
            Event::Master {
                target,
                fader,
                muted,
            } => {
                let address = if target == "main" {
                    Some(("main", 0))
                } else {
                    target.split_once('/').and_then(|(family, index)| {
                        Some((family, index.parse::<u16>().ok()?.checked_sub(1)?))
                    })
                };
                if let Some((family, index)) = address {
                    if fader.is_some() {
                        if let Some(value) = number(index, &format!("_{family}_fader")) {
                            *fader = Some(value);
                        }
                    }
                    if muted.is_some() {
                        if let Some(value) = boolean(index, &format!("_{family}_mute")) {
                            *muted = Some(value);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Engine {
    pub(super) fn partial_masters(sync: &SyncState) -> impl Iterator<Item = Event> + '_ {
        sync.typed
            .values()
            .filter(|event| matches!(event, Event::Master { .. }))
            .cloned()
    }

    pub(super) fn group_write_field(&self, cmd: &Cmd) -> Option<(u16, String)> {
        let (index, field) = match cmd {
            Cmd::MainFader { .. } => (0, "_main_fader".to_string()),
            Cmd::MainMute { .. } => (0, "_main_mute".to_string()),
            Cmd::BusFader { bus, .. } => (*bus as u16, "_bus_fader".to_string()),
            Cmd::BusMute { bus, .. } => (*bus as u16, "_bus_mute".to_string()),
            Cmd::SubgroupFader { group, .. } => (*group as u16, "_subgroup_fader".to_string()),
            Cmd::SubgroupMute { group, .. } => (*group as u16, "_subgroup_mute".to_string()),
            Cmd::MuteGroup { group, .. } => (*group as u16, "_mute_group".to_string()),
            Cmd::MuteGroupAssign { ch, group, key, .. } => (
                self.resolve_strip(*ch, key.as_deref()).ok()?,
                format!("_mg{}", u16::from(*group) + 1),
            ),
            Cmd::SubgroupAssign { ch, group, key, .. } => (
                self.resolve_strip(*ch, key.as_deref()).ok()?,
                format!("_sg{}", u16::from(*group) + 1),
            ),
            _ => return None,
        };
        Some((index, field))
    }

    pub(super) fn retire_group_observation(&self, index: u16, field: &str, started: Instant) {
        let mut sync = self.sync_state.lock().unwrap();
        let key = (index, field.to_string());
        if sync.received.get(&key).is_some_and(|(at, _)| *at > started) {
            return;
        }
        sync.revision += 1;
        let revision = sync.revision;
        sync.received.insert(key, (started, revision));
        sync.typed.remove(&format!("{index}/{field}"));
        if let Some(values) = self.param_cache.write().unwrap().get_mut(&index) {
            values.remove(field);
        }
    }
}
