//! Mute-group membership and bank controls.
use super::*;

impl XAirDriver {
    /// Read the complete four-bit mute-group membership in a single query.
    pub async fn snapshot_mute_group_mask(&self, strip: StripId) -> anyhow::Result<u32> {
        let (_, root) = self.assign_root(strip.0, "mute-group assign")?;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(&groups::grp_mute(&root))
            .await?;
        let mask = groups::mask_from_msg(&reply)?;
        anyhow::ensure!(
            (0..=groups::GROUP_MASK).contains(&mask),
            "invalid XR18 mute-group mask {mask}"
        );
        Ok(mask as u32)
    }

    /// Restore the whole membership mask with one write. Per-bit read/modify/
    /// write can lose earlier bits when the desk applies writes asynchronously.
    pub async fn restore_mute_group_mask(
        &self,
        strip: StripId,
        mask: u32,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            mask <= groups::GROUP_MASK as u32,
            "invalid XR18 mute-group mask {mask}"
        );
        let (_, root) = self.assign_root(strip.0, "mute-group assign")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send_checked(
                &groups::grp_mute(&root),
                vec![OscType::Int(mask as i32)],
                &|| {
                    let members = self
                        .current_bus_control_members(strip)?
                        .unwrap_or_else(|| vec![strip]);
                    check(&members)
                },
            )
            .await
    }

    pub async fn restore_mute_group_master(
        &self,
        group: u8,
        muted: bool,
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        check()?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(muted))])
            .await
    }

    /// Mute-group master mute (`/config/mute/N`, int). Polarity is
    /// INFERRED from the leaf name (`1` = group muted): only `0` has ever
    /// been observed live and the master has never been toggled on hardware
    /// (toggling could mute member channels audibly), so the `1` direction
    /// is UNVERIFIED-ON-RIG. Same-value writes round-trip bit-exact.
    pub(super) async fn driver_set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(muted))])
            .await
    }

    pub(super) async fn driver_get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        match reply.args.first() {
            Some(OscType::Int(i)) => Ok(*i != 0),
            other => anyhow::bail!("{addr}: mute group needs an int arg, got {other:?}"),
        }
    }

    /// Channel -> mute-group membership, bit `group` of the strip's
    /// `{root}/grp/mute` bitmask (rig-verified XR18 FW 1.20, 2026-10-20:
    /// write `1` → `1`, `2` → `2`, restore `0`; out-of-range writes
    /// saturate on the desk, so the group is validated locally first).
    /// Read-modify-write: the desk has no per-group leaf. Main-LR and DCA
    /// strips carry no `grp` leaf (live-silent) and bail locally instead
    /// of timing out on hardware — this also keeps the engine attach
    /// snapshot (which scans every strip) fast.
    pub(super) async fn driver_set_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        self.set_mute_group_assign_checked(ch, group, member, &|_| Ok(()))
            .await
    }

    pub async fn set_mute_group_assign_checked(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let (_, root) = self.assign_root(ch.0, "mute-group assign")?;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        let reply = session.query(&groups::grp_mute(&root)).await?;
        let mask = groups::mask_from_msg(&reply)?;
        let next = groups::mask_with(mask, group, member, "mute group")?;
        session
            .send_checked(&groups::grp_mute(&root), vec![OscType::Int(next)], &|| {
                let members = self
                    .current_bus_control_members(ch)?
                    .unwrap_or_else(|| vec![ch]);
                check(&members)
            })
            .await
    }

    /// Channel -> mute-group membership read (same leaf; groups with no
    /// desk leaf bail locally — see [`Self::set_mute_group_assign`]).
    pub(super) async fn driver_get_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
    ) -> anyhow::Result<bool> {
        let (kind, root) = self.assign_root(ch.0, "mute-group assign")?;
        let _ = kind;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(&groups::grp_mute(&root))
            .await?;
        let mask = groups::mask_from_msg(&reply)?;
        groups::mask_has(mask, group, "mute group")
    }
}

/// Global channel-link paths use one switch for each fixed input pair.
fn channel_link_address(strip: StripId) -> anyhow::Result<(usize, String)> {
    anyhow::ensure!(
        strip.0 < 16,
        "xair: stereo linking is only supported for input channels"
    );
    let pair = usize::from(strip.0 / 2);
    Ok((
        pair,
        format!("/config/chlink/{}-{}", pair * 2 + 1, pair * 2 + 2),
    ))
}

pub(super) fn parse_channel_link(msg: &OscMessage) -> Option<(usize, bool)> {
    let pair = msg.addr.strip_prefix("/config/chlink/")?;
    let (first, second) = pair.split_once('-')?;
    let first: usize = first.parse().ok()?;
    let second: usize = second.parse().ok()?;
    if first == 0 || first > 15 || first.is_multiple_of(2) || second != first + 1 {
        return None;
    }
    let [OscType::Int(value @ 0..=1)] = msg.args.as_slice() else {
        return None;
    };
    Some(((first - 1) / 2, *value == 1))
}

impl XAirDriver {
    pub(super) async fn read_stereo_link(&self, strip: StripId) -> anyhow::Result<bool> {
        let (pair, addr) = channel_link_address(strip)?;
        let started = std::time::Instant::now();
        let session = self.session().await?.as_ref().unwrap().clone();
        let msg = session.query(&addr).await?;
        let (_, linked) = parse_channel_link(&msg)
            .ok_or_else(|| anyhow::anyhow!("{addr}: invalid link reply"))?;
        let writes = self.link_writes.lock().unwrap();
        if writes[pair]
            .is_none_or(|at| at < started && at.elapsed() > std::time::Duration::from_millis(500))
        {
            self.link_cache.lock().unwrap()[pair] = Some((std::time::Instant::now(), linked));
        }
        Ok(linked)
    }

    pub(super) fn stereo_control_members(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<Vec<StripId>> {
        if strip.0 >= 16 || param == "link" {
            return Ok(vec![strip]);
        }
        let pair = usize::from(strip.0 / 2);
        // Never block the fader/mute hot path on an OSC query. A fresh
        // desk-evidenced OFF permits a single-strip write; linked, unknown,
        // or stale state protects both strips and their access/locks.
        let unlinked = self.link_cache.lock().unwrap()[pair].is_some_and(|(at, linked)| {
            !linked && at.elapsed() <= std::time::Duration::from_secs(5)
        });
        if unlinked {
            Ok(vec![strip])
        } else {
            let left = strip.0 & !1;
            Ok(vec![StripId(left), StripId(left + 1)])
        }
    }

    pub(super) async fn write_stereo_link(
        &self,
        strip: StripId,
        linked: bool,
    ) -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        let (pair, addr) = channel_link_address(strip)?;
        let session = self.session().await?.as_ref().unwrap().clone();
        // Discard the prior observation before sending; an unacknowledged
        // local write must not grant unlinked control to either member.
        {
            let mut writes = self.link_writes.lock().unwrap();
            writes[pair] = Some(std::time::Instant::now());
            self.link_cache.lock().unwrap()[pair] = None;
        }
        session
            .send(&addr, vec![OscType::Int(i32::from(linked))])
            .await?;
        let left = (pair * 2) as u16;
        Ok(vec![
            (StripId(left), "link".into(), ParamValue::Bool(linked)),
            (StripId(left + 1), "link".into(), ParamValue::Bool(linked)),
        ])
    }
}

/// Bus keys are topology identities; OSC uses unpadded one-based numbers.
pub(super) fn parse_bus_link(msg: &OscMessage) -> Option<(usize, bool)> {
    let pair = msg.addr.strip_prefix("/config/buslink/")?;
    let index = match pair {
        "1-2" => 0,
        "3-4" => 1,
        "5-6" => 2,
        _ => return None,
    };
    let [OscType::Int(value @ 0..=1)] = msg.args.as_slice() else {
        return None;
    };
    Some((index, *value == 1))
}

impl XAirDriver {
    pub(super) async fn bus_control_members(
        &self,
        strip: StripId,
    ) -> anyhow::Result<Option<Vec<StripId>>> {
        let Some((pair, first)) = self.bus_pair(strip)? else {
            return Ok(None);
        };
        let (revision, fresh) = {
            let cache = self.bus_link_cache.lock().unwrap();
            (
                cache[pair].0,
                cache[pair]
                    .1
                    .is_some_and(|(at, _)| at.elapsed() <= std::time::Duration::from_secs(5)),
            )
        };
        if !fresh {
            let generation = self
                .connection_generation
                .load(std::sync::atomic::Ordering::Acquire);
            let addr = format!("/config/buslink/{}-{}", first, first + 1);
            if let Ok(guard) = self.session().await {
                let session = guard.as_ref().unwrap().clone();
                drop(guard);
                if let Ok(msg) = session.query(&addr).await {
                    if let Some((reply_pair, linked)) = parse_bus_link(&msg) {
                        let mut cache = self.bus_link_cache.lock().unwrap();
                        if reply_pair == pair
                            && cache[pair].0 == revision
                            && generation
                                == self
                                    .connection_generation
                                    .load(std::sync::atomic::Ordering::Acquire)
                        {
                            cache[pair].0 = cache[pair].0.wrapping_add(1);
                            cache[pair].1 = Some((std::time::Instant::now(), linked));
                        }
                    }
                }
            }
        }
        self.current_bus_control_members(strip)
    }

    pub fn current_output_control_members(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<Vec<StripId>> {
        match self.current_bus_control_members(strip)? {
            Some(members) => Ok(members),
            None => self.stereo_control_members(strip, param),
        }
    }

    /// Resolve the cached bus relationship without network I/O. Unknown or
    /// expired observations protect both members. Independent desk edits can
    /// still race an OSC send; this is not a desk-side atomic transaction.
    pub fn current_bus_control_members(
        &self,
        strip: StripId,
    ) -> anyhow::Result<Option<Vec<StripId>>> {
        let Some((pair, first)) = self.bus_pair(strip)? else {
            return Ok(None);
        };
        let topology = self.topology();
        let members: Vec<StripId> = (first..=first + 1)
            .map(|n| {
                let key = openfader_core::strip_key("bus", n);
                let index = openfader_core::strip_index_for_key(&topology, &key)
                    .ok_or_else(|| anyhow::anyhow!("missing XR18 bus strip {key}"))?;
                anyhow::ensure!(
                    index < self.strip_count()
                        && index <= u16::MAX as usize
                        && strip_root(index as u16)?.1 == format!("/bus/{n}"),
                    "invalid XR18 bus mapping {key}"
                );
                Ok(StripId(index as u16))
            })
            .collect::<anyhow::Result<_>>()?;
        let unlinked = self.bus_link_cache.lock().unwrap()[pair]
            .1
            .is_some_and(|(at, linked)| {
                !linked && at.elapsed() <= std::time::Duration::from_secs(5)
            });
        Ok(Some(if unlinked { vec![strip] } else { members }))
    }

    fn bus_pair(&self, strip: StripId) -> anyhow::Result<Option<(usize, u16)>> {
        let topology = self.topology();
        let Some(bus) = (1..=6u16).find(|n| {
            openfader_core::strip_index_for_key(&topology, &openfader_core::strip_key("bus", *n))
                == Some(usize::from(strip.0))
        }) else {
            return Ok(None);
        };
        let pair = usize::from((bus - 1) / 2);
        Ok(Some((pair, (pair * 2 + 1) as u16)))
    }
}

impl XAirDriver {
    pub(super) fn refresh_bus_links(&self, session: std::sync::Arc<OscSession>, generation: u64) {
        let cache = self.bus_link_cache.clone();
        let current_generation = self.connection_generation.clone();
        let revisions = {
            let mut state = cache.lock().unwrap();
            for entry in state.iter_mut() {
                entry.0 = entry.0.wrapping_add(1);
                entry.1 = None;
            }
            [state[0].0, state[1].0, state[2].0]
        };
        let task = tokio::spawn(async move {
            for (pair, revision) in revisions.into_iter().enumerate() {
                let first = pair * 2 + 1;
                let addr = format!("/config/buslink/{first}-{}", first + 1);
                if let Ok(msg) = session.query(&addr).await {
                    if let Some((index, linked)) = parse_bus_link(&msg) {
                        let mut state = cache.lock().unwrap();
                        if index == pair
                            && state[pair].0 == revision
                            && current_generation.load(std::sync::atomic::Ordering::Acquire)
                                == generation
                        {
                            state[pair].0 = state[pair].0.wrapping_add(1);
                            state[pair].1 = Some((std::time::Instant::now(), linked));
                        }
                    }
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
    }

    pub(super) fn bus_link_controls(
        &self,
    ) -> Vec<openfader_core::destinations::DestinationControl> {
        use openfader_core::destinations::{
            DestinationChoice, DestinationControl, DestinationValue,
        };
        use std::sync::atomic::Ordering;
        if !self.connected.load(Ordering::Acquire) {
            return vec![];
        }
        let generation = self.connection_generation.load(Ordering::Acquire);
        let topology = self.topology();
        let cache = self.bus_link_cache.lock().unwrap();
        (0..3)
            .filter_map(|pair| {
                let first = pair * 2 + 1;
                let members: Vec<_> = (first..=first + 1)
                    .map(|n| openfader_core::strip_key("bus", n as u16))
                    .collect();
                if members
                    .iter()
                    .any(|key| openfader_core::strip_index_for_key(&topology, key).is_none())
                {
                    return None;
                }
                Some(DestinationControl {
                    revision: Some(cache[pair].0),
                    generation: Some(generation),
                    preset: false,
                    members,
                    target: "bus_links".into(),
                    target_label: "Bus links".into(),
                    target_name: None,
                    send_slot: None,
                    surface: "bus_link_state".into(),
                    description: "Read-only bus link state.".into(),
                    id: format!("pair_{first}_{}", first + 1),
                    label: format!("Bus {first}–{}", first + 1),
                    tap_tempo: false,
                    gesture: None,
                    choices: vec![
                        DestinationChoice {
                            value: 0.0,
                            label: "Unlinked".into(),
                        },
                        DestinationChoice {
                            value: 1.0,
                            label: "Linked".into(),
                        },
                    ],
                    range: None,
                    current: cache[pair]
                        .1
                        .filter(|(at, _)| at.elapsed() <= std::time::Duration::from_secs(5))
                        .map(|(_, linked)| DestinationValue {
                            value: u8::from(linked) as f32,
                            verified: true,
                        }),
                })
            })
            .collect()
    }
}
