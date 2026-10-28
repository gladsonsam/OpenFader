//! Mute-group membership and bank controls.
use super::*;

struct BusLinkPending<'a> {
    driver: &'a XAirDriver,
    pair: usize,
    generation: u64,
    confirmed: bool,
}
impl Drop for BusLinkPending<'_> {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        let mut admission = self.driver.bus_link_cache.lock().unwrap();
        let mut observed = self.driver.bus_link_observations.lock().unwrap();
        if self.driver.connection_generation.load(Ordering::Acquire) != self.generation {
            return;
        }
        admission[self.pair].0 = admission[self.pair].0.wrapping_add(1);
        admission[self.pair].1 = None;
        if !self.confirmed {
            observed[self.pair].0 = observed[self.pair].0.wrapping_add(1);
            observed[self.pair].1 = None;
        }
        self.driver
            .bus_link_pending
            .fetch_and(!(1 << self.pair), Ordering::AcqRel);
    }
}

impl XAirDriver {
    pub(super) async fn set_bus_link_control(
        &self,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        use std::sync::atomic::Ordering;
        let pair = ["pair_1_2", "pair_3_4", "pair_5_6"]
            .iter()
            .position(|key| *key == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown bus link"))?;
        anyhow::ensure!(value == 0.0 || value == 1.0, "Invalid bus link value");
        let _lane = self.bus_link_write.lock().await;
        let _dispatch = self.bus_dispatch.write().await;
        let generation = self.connection_generation.load(Ordering::Acquire);
        anyhow::ensure!(
            check() && self.bus_link_controls() == snapshot && snapshot.len() == 3,
            "Bus links changed; reopen the control"
        );
        anyhow::ensure!(
            snapshot[pair]
                .current
                .as_ref()
                .is_some_and(|v| v.verified && v.value == expected)
                && snapshot
                    .iter()
                    .all(|row| row.current.as_ref().is_some_and(|v| v.verified)),
            "Wait for complete bus link readback"
        );
        let topology = self.topology();
        for key in &snapshot[pair].members {
            let index = openfader_core::strip_index_for_key(&topology, key)
                .ok_or_else(|| anyhow::anyhow!("Bus link topology unavailable"))?;
            anyhow::ensure!(
                self.bus_pair(StripId(u16::try_from(index)?))?
                    .is_some_and(|(p, _)| p == pair),
                "Bus link topology changed"
            );
            anyhow::ensure!(
                self.current_bus_control_members(StripId(u16::try_from(index)?))?
                    .is_some(),
                "Bus link roots unavailable"
            );
        }
        anyhow::ensure!(
            snapshot[pair].members.len() == 2,
            "Bus link scope unavailable"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        for (index, row) in snapshot.iter().enumerate() {
            let first = index * 2 + 1;
            let reply = session
                .query_timeout(
                    &format!("/config/buslink/{first}-{}", first + 1),
                    std::time::Duration::from_millis(350),
                )
                .await?;
            anyhow::ensure!(
                check()
                    && self.connection_generation.load(Ordering::Acquire) == generation
                    && parse_bus_link(&reply).map(|(_, linked)| u8::from(linked) as f32)
                        == row.current.as_ref().map(|v| v.value),
                "Bus links changed; reopen the control"
            );
        }
        anyhow::ensure!(
            self.bus_link_controls() == snapshot,
            "Bus links changed; reopen the control"
        );
        {
            let mut admission = self.bus_link_cache.lock().unwrap();
            let mut observed = self.bus_link_observations.lock().unwrap();
            anyhow::ensure!(
                self.connection_generation.load(Ordering::Acquire) == generation
                    && self.bus_link_pending.load(Ordering::Acquire) == 0
                    && observed.iter().zip(snapshot).all(|(cell, row)| Some(cell.0)
                        == row.revision
                        && cell.1.is_some_and(|(at, v)| at.elapsed()
                            <= std::time::Duration::from_secs(5)
                            && row
                                .current
                                .as_ref()
                                .is_some_and(|r| r.value == u8::from(v) as f32))),
                "Bus links changed; reopen the control"
            );
            self.bus_link_pending.fetch_or(1 << pair, Ordering::AcqRel);
            admission[pair].0 = admission[pair].0.wrapping_add(1);
            admission[pair].1 = None;
            observed[pair].0 = observed[pair].0.wrapping_add(1);
            observed[pair].1 = None;
        }
        let mut pending = BusLinkPending {
            driver: self,
            pair,
            generation,
            confirmed: false,
        };
        let first = pair * 2 + 1;
        let address = format!("/config/buslink/{first}-{}", first + 1);
        let guard = self.session().await?;
        anyhow::ensure!(
            guard
                .as_ref()
                .is_some_and(|s| std::sync::Arc::ptr_eq(s, &session)),
            "Bus link session changed"
        );
        session
            .send_checked(&address, vec![OscType::Int(value as i32)], &|| {
                anyhow::ensure!(
                    check()
                        && self.connected.load(Ordering::Acquire)
                        && self.connection_generation.load(Ordering::Acquire) == generation,
                    "Bus link access or session changed"
                );
                Ok(())
            })
            .await?;
        drop(guard);
        for _ in 0..8 {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let reply = session
                .query_timeout(&address, std::time::Duration::from_millis(350))
                .await;
            anyhow::ensure!(
                check()
                    && self.connected.load(Ordering::Acquire)
                    && self.connection_generation.load(Ordering::Acquire) == generation,
                "Bus link result uncertain: access or session changed"
            );
            if reply
                .as_ref()
                .ok()
                .and_then(parse_bus_link)
                .is_some_and(|(p, v)| p == pair && v == (value == 1.0))
            {
                let mut observed = self.bus_link_observations.lock().unwrap();
                anyhow::ensure!(
                    self.connection_generation.load(Ordering::Acquire) == generation,
                    "Bus link session changed"
                );
                observed[pair].0 = observed[pair].0.wrapping_add(1);
                observed[pair].1 = Some((std::time::Instant::now(), value == 1.0));
                pending.confirmed = true;
                drop(observed);
                drop(pending);
                return self
                    .bus_link_controls()
                    .into_iter()
                    .find(|row| {
                        row.id == id
                            && row
                                .current
                                .as_ref()
                                .is_some_and(|v| v.verified && v.value == value)
                    })
                    .ok_or_else(|| anyhow::anyhow!("Bus link result unavailable"));
            }
        }
        anyhow::bail!("Bus link command sent; independent readback unavailable")
    }

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
        let _dispatch = self.bus_dispatch.read().await;
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
        let _dispatch = self.bus_dispatch.read().await;
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
        self.write_stereo_link_checked(strip, linked, &|_| Ok(()))
            .await
    }

    pub(super) async fn write_stereo_link_checked(
        &self,
        strip: StripId,
        linked: bool,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        let (pair, addr) = channel_link_address(strip)?;
        let left = (pair * 2) as u16;
        let members = [StripId(left), StripId(left + 1)];
        let _lane = self.preamp_write.lock().await;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        let authorize = || {
            anyhow::ensure!(
                self.connected.load(std::sync::atomic::Ordering::Acquire)
                    && self
                        .connection_generation
                        .load(std::sync::atomic::Ordering::Acquire)
                        == generation,
                "xair: connection changed; reopen the control"
            );
            check(&members)
        };
        authorize()?;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap().clone();
        authorize()?;
        // Discard the prior observation before sending; an unacknowledged
        // local write must not grant unlinked control to either member.
        {
            let mut writes = self.link_writes.lock().unwrap();
            writes[pair] = Some(std::time::Instant::now());
            self.link_cache.lock().unwrap()[pair] = None;
        }
        session
            .send_checked(&addr, vec![OscType::Int(i32::from(linked))], &authorize)
            .await?;
        authorize()?;
        drop(guard);
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
                            && self
                                .bus_link_pending
                                .load(std::sync::atomic::Ordering::Acquire)
                                & (1 << pair)
                                == 0
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
        let unlinked = self
            .bus_link_pending
            .load(std::sync::atomic::Ordering::Acquire)
            & (1 << pair)
            == 0
            && self.bus_link_cache.lock().unwrap()[pair]
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
        let cache = self.bus_link_observations.clone();
        let pending = self.bus_link_pending.clone();
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
                        if pending.load(std::sync::atomic::Ordering::Acquire) & (1 << pair) == 0
                            && index == pair
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
        let cache = self.bus_link_observations.lock().unwrap();
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
                    surface: "bus_link".into(),
                    description: "Links both buses and may copy their levels, names and processing. Full access to both buses is required.".into(),
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
                        .filter(|(at, _)| self.bus_link_pending.load(Ordering::Acquire) & (1 << pair) == 0 && at.elapsed() <= std::time::Duration::from_secs(5))
                        .map(|(_, linked)| DestinationValue {
                            value: u8::from(linked) as f32,
                            verified: true,
                        }),
                })
            })
            .collect()
    }
}
