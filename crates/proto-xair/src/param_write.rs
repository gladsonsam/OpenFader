//! Strip parameter command dispatch.
use super::*;

type SendCheck<'a> = dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync + 'a;

impl XAirDriver {
    /// Full param plane (see [`XAIR_CAPABILITIES`]). Strip roots come from
    /// [`strip_root`]: channels (`/ch/NN`) carry the whole set; FX returns
    /// (`/rtn/N`), buses and LR carry fader/mute/name/color plus the
    /// kind-scoped gate/comp/EQ sets; FX-send masters and DCAs carry
    /// fader/mute/name/color only (catalogue scopes). Every address/codec is
    /// confirmed against the owner XR18 (FW 1.20) — see
    /// XR18 baseline notes for the readback table.
    ///
    /// GR reads ride the param path (`gate_gr`/`comp_gr` serve dB reduction
    /// `>= 0` from the last `/meters/6` blob; no GR query address exists).
    /// `delay_on`/`delay_ms` (X32-only), `hpf_slope` (X32-only), `source`
    /// (input source selection not yet integrated), `icon` (no firmware leaf),
    /// per-channel `eq5_*`
    /// (channels/returns carry 4 bands; `eq5/6_*` ride buses + main),
    /// `aux7/8_*`, `send11..16_*` and all legacy `auxN_mute`/`fxN_mute` plus
    /// the v2 `send1..10_on` (the send block has no on/off leaf and
    /// `/ch/NN/mix/MM/on` is live-silent — baseline T7) stay unsupported.
    pub(super) async fn driver_set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.driver_set_param_checked(strip, param, value, None)
            .await
    }

    pub async fn set_bus_send_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let (slot, leaf) = generic_send(param)?;
        anyhow::ensure!(
            (1..=6).contains(&slot) && matches!(leaf, "level" | "tap" | "pan" | "grpon"),
            "not a bus send"
        );
        self.driver_set_param_checked(strip, param, value, Some(check))
            .await
    }

    pub async fn set_bus_param_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        check: &SendCheck<'_>,
    ) -> anyhow::Result<()> {
        self.driver_set_param_checked(strip, param, value, Some(check))
            .await
    }

    async fn driver_set_param_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        check: Option<&SendCheck<'_>>,
    ) -> anyhow::Result<()> {
        let (kind, root) = strip_root(strip.0)?;
        if param == "automix_group" {
            anyhow::ensure!(
                kind == openfader_core::StripKind::In,
                "automix group is input-only"
            );
        }
        if openfader_core::params::find(param).is_some() {
            check_scope(param, kind)?;
        }
        let ch = || self.check_channel(strip.0);
        let authorize = || {
            if kind == openfader_core::StripKind::Bus {
                if let Some(check) = check {
                    check(&self.current_output_control_members(strip, param)?)?;
                }
            }
            Ok(())
        };
        match (param, value) {
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n))
                if kind == openfader_core::StripKind::Bus && check.is_some() =>
            {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &strip_config_name(&root),
                        vec![OscType::String(n)],
                        &authorize,
                    )
                    .await
            }
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("link", ParamValue::Bool(linked)) => {
                self.write_stereo_link(strip, linked).await.map(|_| ())
            }
            ("pan", ParamValue::Float(pan)) => {
                let wire = mix::pan_to_wire(pan)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &format!("{root}/mix/pan"),
                        vec![OscType::Float(wire)],
                        &authorize,
                    )
                    .await
            }
            ("lr_assign", ParamValue::Bool(assigned)) => {
                // No leaf on `/lr` (the `/lr/mix` container has no `lr`
                // field) — the scope check above already confines this to
                // channel/FX-return/bus strips.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &mix::mix_lr(&root),
                        vec![codec::bool_arg(assigned)],
                        &authorize,
                    )
                    .await
            }
            ("color", ParamValue::Float(v)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &look::strip_color(&root),
                        vec![look::color_to_arg(v)?],
                        &authorize,
                    )
                    .await
            }
            ("icon", ParamValue::Float(v)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &look::strip_icon(&root),
                        vec![look::icon_to_arg(v)?],
                        &authorize,
                    )
                    .await
            }
            ("gate_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &format!("{root}/gate/on"),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("comp_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &format!("{root}/dyn/on"),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("eq_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&eq::eq_on_r(&root), vec![codec::bool_arg(on)], &authorize)
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("eq") => {
                let (band, leaf) = eq_leaf(p)?;
                check_eq_band(kind, band)?;
                let (addr, arg) = match leaf {
                    "gain" => (
                        eq::eq_gain_r(&root, band)?,
                        OscType::Float(eq::gain_db_to_wire(v)?),
                    ),
                    "freq" => (
                        eq::eq_freq_r(&root, band)?,
                        OscType::Float(eq::freq_hz_to_wire(v)?),
                    ),
                    "q" => (eq::eq_q_r(&root, band)?, OscType::Float(eq::q_to_wire(v)?)),
                    "type" => (eq::eq_type_r(&root, band)?, eq::eqtype_to_arg(v)?),
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr, vec![arg], &authorize)
                    .await
            }
            (p, ParamValue::Float(v))
                if matches!(p, "gate_mode" | "gate_key_src" | "gate_filter_type") =>
            {
                let (addr, arg) = match p {
                    "gate_mode" => (
                        gate::gate_mode(&root),
                        gate::int_leaf_to_arg(v, gate::GATE_MODE_MIN, gate::GATE_MODE_MAX, "mode")?,
                    ),
                    "gate_key_src" => (
                        gate::gate_key_src(&root),
                        gate::int_leaf_to_arg(
                            v,
                            gate::GATE_KEY_SRC_MIN,
                            gate::GATE_KEY_SRC_MAX,
                            "keysrc",
                        )?,
                    ),
                    _ => (
                        gate::gate_filter_type(&root),
                        gate::int_leaf_to_arg(
                            v,
                            gate::GATE_FILTER_TYPE_MIN,
                            gate::GATE_FILTER_TYPE_MAX,
                            "filter type",
                        )?,
                    ),
                };
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr, vec![arg], &authorize)
                    .await
            }
            ("gate_filter_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &gate::gate_filter_on(&root),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("gate_filter_freq", ParamValue::Float(v)) => {
                let wire = gate::filter_hz_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &gate::gate_filter_freq(&root),
                        vec![OscType::Float(wire)],
                        &authorize,
                    )
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("gate_") => {
                let addr = match p {
                    "gate_thr" => (
                        gate::gate_thr(&root),
                        gate::lin_to_wire(v, gate::GATE_THR_MIN_DB, gate::GATE_THR_MAX_DB, "thr")?,
                    ),
                    "gate_range" => (
                        gate::gate_range(&root),
                        gate::lin_to_wire(
                            v,
                            gate::GATE_RANGE_MIN_DB,
                            gate::GATE_RANGE_MAX_DB,
                            "range",
                        )?,
                    ),
                    "gate_att" => (
                        gate::gate_attack(&root),
                        gate::lin_to_wire(
                            v,
                            gate::GATE_ATT_MIN_MS,
                            gate::GATE_ATT_MAX_MS,
                            "attack",
                        )?,
                    ),
                    "gate_hold" => (
                        gate::gate_hold(&root),
                        gate::log_to_wire(
                            v,
                            gate::GATE_HOLD_MIN_MS,
                            gate::GATE_HOLD_MAX_MS,
                            "hold",
                        )?,
                    ),
                    "gate_rel" => (
                        gate::gate_release(&root),
                        gate::log_to_wire(
                            v,
                            gate::GATE_REL_MIN_MS,
                            gate::GATE_REL_MAX_MS,
                            "release",
                        )?,
                    ),
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr.0, vec![OscType::Float(addr.1)], &authorize)
                    .await
            }
            (p, ParamValue::Float(v))
                if matches!(
                    p,
                    "comp_thr"
                        | "comp_knee"
                        | "comp_makeup"
                        | "comp_att"
                        | "comp_hold"
                        | "comp_rel"
                        | "comp_mix"
                ) =>
            {
                let (addr, wire) = match p {
                    "comp_thr" => (
                        comp::comp_thr(&root),
                        comp::lin_to_wire(v, comp::COMP_THR_MIN_DB, comp::COMP_THR_MAX_DB, "thr")?,
                    ),
                    "comp_knee" => (
                        comp::comp_knee(&root),
                        comp::lin_to_wire(v, comp::COMP_KNEE_MIN, comp::COMP_KNEE_MAX, "knee")?,
                    ),
                    "comp_makeup" => (
                        comp::comp_makeup(&root),
                        comp::lin_to_wire(
                            v,
                            comp::COMP_MAKEUP_MIN_DB,
                            comp::COMP_MAKEUP_MAX_DB,
                            "makeup",
                        )?,
                    ),
                    "comp_att" => (
                        comp::comp_attack(&root),
                        comp::lin_to_wire(
                            v,
                            comp::COMP_ATT_MIN_MS,
                            comp::COMP_ATT_MAX_MS,
                            "attack",
                        )?,
                    ),
                    "comp_hold" => (
                        comp::comp_hold(&root),
                        comp::log_to_wire(
                            v,
                            comp::COMP_HOLD_MIN_MS,
                            comp::COMP_HOLD_MAX_MS,
                            "hold",
                        )?,
                    ),
                    "comp_rel" => (
                        comp::comp_release(&root),
                        comp::log_to_wire(
                            v,
                            comp::COMP_REL_MIN_MS,
                            comp::COMP_REL_MAX_MS,
                            "release",
                        )?,
                    ),
                    _ => (
                        comp::comp_mix(&root),
                        comp::lin_to_wire(v, comp::COMP_MIX_MIN, comp::COMP_MIX_MAX, "mix")?,
                    ),
                };
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr, vec![OscType::Float(wire)], &authorize)
                    .await
            }
            ("comp_ratio", ParamValue::Float(v)) => {
                let idx = comp::ratio_to_index(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &comp::comp_ratio(&root),
                        vec![OscType::Int(idx)],
                        &authorize,
                    )
                    .await
            }
            (p, ParamValue::Float(v))
                if matches!(
                    p,
                    "comp_mode" | "comp_det" | "comp_env" | "comp_key_src" | "comp_filter_type"
                ) =>
            {
                let (addr, arg) = match p {
                    "comp_mode" => (
                        comp::comp_mode(&root),
                        comp::int_leaf_to_arg(v, comp::COMP_MODE_MIN, comp::COMP_MODE_MAX, "mode")?,
                    ),
                    "comp_det" => (
                        comp::comp_det(&root),
                        comp::int_leaf_to_arg(v, comp::COMP_DET_MIN, comp::COMP_DET_MAX, "det")?,
                    ),
                    "comp_env" => (
                        comp::comp_env(&root),
                        comp::int_leaf_to_arg(v, comp::COMP_ENV_MIN, comp::COMP_ENV_MAX, "env")?,
                    ),
                    "comp_key_src" => (
                        comp::comp_key_src(&root),
                        comp::int_leaf_to_arg(v, 0, 22, "keysrc")?,
                    ),
                    _ => (
                        comp::comp_filter_type(&root),
                        comp::int_leaf_to_arg(v, 0, 8, "filter type")?,
                    ),
                };
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr, vec![arg], &authorize)
                    .await
            }
            ("comp_auto", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &comp::comp_auto(&root),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("comp_filter_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &comp::comp_filter_on(&root),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("comp_filter_freq", ParamValue::Float(v)) => {
                let wire = gate::filter_hz_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &comp::comp_filter_freq(&root),
                        vec![OscType::Float(wire)],
                        &authorize,
                    )
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("aux") => {
                let n = aux_send(p)?;
                let ch = ch()?;
                let wire = mix::send_db_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(
                        &mix::send_level(ch, mix::aux_slot(n)?)?,
                        vec![OscType::Float(wire)],
                    )
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("fx") => {
                let n = fx_send(p)?;
                let ch = ch()?;
                let wire = mix::send_db_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(
                        &mix::send_level(ch, mix::fx_slot(n)?)?,
                        vec![OscType::Float(wire)],
                    )
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("send") => {
                let (n, leaf) = generic_send(p)?;
                let ch = ch()?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let authorize = || self.authorize_send(strip, n.into(), check);
                match leaf {
                    "level" => {
                        let wire = mix::send_db_to_wire(v)?;
                        session
                            .send_checked(
                                &mix::send_level(ch, slot)?,
                                vec![OscType::Float(wire)],
                                &authorize,
                            )
                            .await
                    }
                    "tap" => {
                        session
                            .send_checked(
                                &mix::send_tap(&root, slot)?,
                                vec![mix::tap_to_arg(v)?],
                                &authorize,
                            )
                            .await
                    }
                    "pan" => {
                        let wire = mix::pan_to_wire(v)?;
                        session
                            .send_checked(
                                &mix::send_pan(&root, slot)?,
                                vec![OscType::Float(wire)],
                                &authorize,
                            )
                            .await
                    }
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                }
            }
            (p, ParamValue::Bool(on)) if p.starts_with("send") => {
                // `sendN_on` and `sendN_grpon` (level/tap/pan are floats):
                // wrong-leaf bools fall through to the unknown-param error
                // below. The addresses are root-based; the scope check above
                // already confined this to input strips.
                let (n, leaf) = generic_send(p)?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let addr = match leaf {
                    "on" => mix::send_on(&root, slot)?,
                    "grpon" => mix::send_grpon(&root, slot)?,
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                let authorize = || self.authorize_send(strip, n.into(), check);
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(&addr, vec![codec::bool_arg(on)], &authorize)
                    .await
            }
            ("gain", ParamValue::Float(db)) => {
                let _ = db;
                anyhow::bail!("xair: preamp write requires captured active source")
            }
            ("phantom", ParamValue::Bool(on)) => {
                let _ = on;
                anyhow::bail!("xair: preamp write requires captured active source")
            }
            ("usbreturn", ParamValue::Float(_)) => {
                anyhow::bail!("xair: USB return requires guarded input routing")
            }
            ("trim", ParamValue::Float(db)) => {
                let ch = self.check_channel(strip.0)?;
                let wire = input::rtntrim_db_to_wire(db)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_rtntrim(ch), vec![OscType::Float(wire)])
                    .await
            }
            ("polarity", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_invert(ch), vec![codec::bool_arg(on)])
                    .await
            }
            ("hpf_on", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_hpf_on(ch), vec![codec::bool_arg(on)])
                    .await
            }
            ("hpf_freq", ParamValue::Float(hz)) => {
                let ch = self.check_channel(strip.0)?;
                let wire = input::hpf_hz_to_wire(hz)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_hpf(ch), vec![OscType::Float(wire)])
                    .await
            }
            ("automix_weight", ParamValue::Float(w)) => {
                // Channels only (the scope check above already rejected
                // buses/returns/mains); no engineering curve is verified,
                // so the wire float rides verbatim.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(
                        &automix::automix_weight(&root),
                        vec![OscType::Float(automix::weight_to_wire(w)?)],
                    )
                    .await
            }
            ("automix_on", ParamValue::Bool(on)) => {
                // No standalone leaf: read the container (keeping the
                // current weight) and rewrite it with the new enable bit.
                // Two queries, one write — the enable survives verbatim.
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let reply = session.query(&automix::automix(&root)).await?;
                let (group, w) = automix::automix_group_from_msg(&reply)?;
                let next = if on {
                    if group == 0 {
                        1
                    } else {
                        group
                    }
                } else {
                    0
                };
                session
                    .send(
                        &automix::automix(&root),
                        vec![OscType::Int(next), OscType::Float(w)],
                    )
                    .await
            }
            ("automix_group", ParamValue::Float(value)) => {
                anyhow::ensure!(
                    value.is_finite() && value.fract() == 0.0 && (0.0..=2.0).contains(&value),
                    "invalid automix group"
                );
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let reply = session.query(&automix::automix(&root)).await?;
                let (_, weight) = automix::automix_group_from_msg(&reply)?;
                session
                    .send(
                        &automix::automix(&root),
                        vec![OscType::Int(value as i32), OscType::Float(weight)],
                    )
                    .await
            }
            (p, ParamValue::Bool(member)) if p.starts_with("dca") => {
                // Read-modify-write on the `{root}/grp/dca` bitmask (the
                // desk has no per-group leaf). Out-of-range groups and
                // upper bits never reach the desk (see `groups`).
                let group = dca_assign(p)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let reply = session.query(&groups::grp_dca(&root)).await?;
                let mask = groups::mask_from_msg(&reply)?;
                let next = groups::mask_with(mask, group, member, "dca")?;
                session
                    .send_checked(
                        &groups::grp_dca(&root),
                        vec![OscType::Int(next)],
                        &authorize,
                    )
                    .await
            }
            ("insert_on", ParamValue::Bool(on)) => {
                // Insert bypass switch (`{root}/insert/on`, rig-verified
                // 2026-10-20). The scope check above already confined this
                // to channel/bus/main strips (returns/sends/masters have no
                // insert leaf — the desk is silent there).
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &insert::insert_on(&root),
                        vec![codec::bool_arg(on)],
                        &authorize,
                    )
                    .await
            }
            ("insert_sel", ParamValue::Float(v)) => {
                // Insert select (`{root}/insert/sel`, raw int `0..=8`,
                // rig-verified 2026-10-20 — NOT 1..4 FX slots, so no slot
                // mapping is asserted). Same scoping as `insert_on`.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send_checked(
                        &insert::insert_sel(&root),
                        vec![insert::sel_to_arg(v)?],
                        &authorize,
                    )
                    .await
            }
            (p, _) => anyhow::bail!("{}: unknown param {p:?}", self.name()),
        }
    }
}

impl XAirDriver {
    fn authorize_send(
        &self,
        strip: StripId,
        slot: u16,
        check: Option<&SendCheck<'_>>,
    ) -> anyhow::Result<()> {
        let Some(check) = check else {
            return Ok(());
        };
        let topology = self.topology();
        let first = ((slot - 1) / 2) * 2 + 1;
        let mut buses = Vec::new();
        for n in first..=first + 1 {
            let key = openfader_core::strip_key("bus", n);
            let index = openfader_core::strip_index_for_key(&topology, &key)
                .ok_or_else(|| anyhow::anyhow!("missing XR18 bus strip {key}"))?;
            anyhow::ensure!(
                index < self.strip_count()
                    && index <= u16::MAX as usize
                    && strip_root(index as u16)?.1 == format!("/bus/{n}"),
                "invalid XR18 bus mapping {key}"
            );
            buses.push(StripId(index as u16));
        }
        let unlinked = self.bus_link_cache.lock().unwrap()[usize::from((slot - 1) / 2)]
            .1
            .is_some_and(|(at, linked)| {
                !linked && at.elapsed() <= std::time::Duration::from_secs(5)
            });
        let mut members = self.stereo_control_members(strip, "send")?;
        if unlinked {
            members.push(buses[usize::from((slot - 1) % 2)]);
        } else {
            members.extend(buses);
        }
        check(&members)
    }
}
