//! Strip parameter query dispatch.
use super::*;

impl XAirDriver {
    pub(super) async fn driver_get_param(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<ParamValue> {
        let (kind, root) = strip_root(strip.0)?;
        if openfader_core::params::find(param).is_some() {
            check_scope(param, kind)?;
        }
        let ch = || self.check_channel(strip.0);
        if let Some(suffix) = param.strip_prefix("_node/") {
            let allowed = matches!(
                suffix,
                "config"
                    | "preamp"
                    | "gate"
                    | "gate/filter"
                    | "dyn"
                    | "dyn/filter"
                    | "eq"
                    | "mix"
                    | "grp"
                    | "insert"
                    | "automix"
            ) || suffix
                .strip_prefix("eq/")
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|n| (1..=6).contains(&n))
                || suffix
                    .strip_prefix("mix/")
                    .and_then(|s| s.parse::<u8>().ok())
                    .is_some_and(|n| (1..=10).contains(&n));
            anyhow::ensure!(allowed, "unsupported node query");
            let guard = self.session().await?;
            guard
                .as_ref()
                .unwrap()
                .query(&format!("{root}/{suffix}"))
                .await?;
            // The ordered receive plane expands the complete typed reply.
            return Ok(ParamValue::Bool(true));
        }
        if let Some(addr) = sync_decode::address(strip.0, param) {
            let guard = self.session().await?;
            let reply = guard.as_ref().unwrap().query(&addr).await?;
            return sync_decode::decode(param, &reply);
        }
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            "color" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&look::strip_color(&root))
                    .await?;
                look::int_from_msg(&reply, look::COLOR_MIN, look::COLOR_MAX, "color")
                    .map(|i| ParamValue::Float(i as f32))
            }
            "icon" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&look::strip_icon(&root))
                    .await?;
                look::int_from_msg(&reply, look::ICON_MIN, look::ICON_MAX, "icon")
                    .map(|i| ParamValue::Float(i as f32))
            }
            "link" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&look::strip_link(&root))
                    .await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "gain" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&headamp::headamp_gain(ch))
                    .await?;
                headamp::wire_to_db(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "hpf_on" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf_on(ch))
                    .await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "hpf_freq" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf(ch))
                    .await?;
                input::hpf_wire_to_hz(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "automix_on" => {
                // No standalone leaf (the desk is silent on
                // `{root}/automix/on`): read the container and take pos0.
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&automix::automix(&root))
                    .await?;
                automix::automix_from_msg(&reply).map(|(on, _)| ParamValue::Bool(on))
            }
            p if p.starts_with("dca") => {
                // DCA membership bits of the `{root}/grp/dca` bitmask
                // (the scope check above already confined this to strips
                // whose root answers the leaf — channels, FX returns,
                // buses; main/DCA/FX-send masters are silent there).
                let group = dca_assign(p)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&groups::grp_dca(&root))
                    .await?;
                let mask = groups::mask_from_msg(&reply)?;
                groups::mask_has(mask, group, "dca").map(ParamValue::Bool)
            }
            "insert_on" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&insert::insert_on(&root))
                    .await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "insert_sel" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&insert::insert_sel(&root))
                    .await?;
                insert::sel_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
            }
            "pan" => {
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&format!("{root}/mix/pan"))
                    .await?;
                mix::wire_to_pan(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "lr_assign" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&mix::mix_lr(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "eq_on" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&eq::eq_on_r(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            // GR reads ride the param path (DYN blob cache; no desk query
            // address). Placed before the `gate_`/`comp_` prefix arms.
            "gate_gr" => self.cached_gr(strip, true).await.map(ParamValue::Float),
            "comp_gr" => self.cached_gr(strip, false).await.map(ParamValue::Float),
            p if p.starts_with("eq") => {
                let (band, leaf) = eq_leaf(p)?;
                check_eq_band(kind, band)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                match leaf {
                    "gain" => {
                        let reply = session.query(&eq::eq_gain_r(&root, band)?).await?;
                        eq::wire_to_gain_db(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "freq" => {
                        let reply = session.query(&eq::eq_freq_r(&root, band)?).await?;
                        eq::wire_to_freq_hz(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "q" => {
                        let reply = session.query(&eq::eq_q_r(&root, band)?).await?;
                        eq::wire_to_q(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "type" => {
                        let reply = session.query(&eq::eq_type_r(&root, band)?).await?;
                        eq::eqtype_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
                    }
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                }
            }
            p if p.starts_with("aux") => {
                let n = aux_send(p)?;
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&mix::send_level(ch, mix::aux_slot(n)?)?)
                    .await?;
                mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
            }
            p if p.starts_with("fx") => {
                let n = fx_send(p)?;
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&mix::send_level(ch, mix::fx_slot(n)?)?)
                    .await?;
                mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
            }
            // Preserve the pre-existing direct helper for callers using
            // unadvertised leaves (loopback compatibility only). XR18 caps,
            // push and polling deliberately exclude send on/mute/dest.
            p if p.starts_with("send") => {
                let (n, leaf) = generic_send(p)?;
                let ch = ch()?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                match leaf {
                    "level" => {
                        let reply = session.query(&mix::send_level(ch, slot)?).await?;
                        mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
                    }
                    "tap" => {
                        let reply = session.query(&mix::send_tap(&root, slot)?).await?;
                        mix::tap_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
                    }
                    "grpon" => {
                        let reply = session.query(&mix::send_grpon(&root, slot)?).await?;
                        osc::is_on(&reply).map(ParamValue::Bool)
                    }
                    "pan" => {
                        let reply = session.query(&mix::send_pan(&root, slot)?).await?;
                        mix::wire_to_pan(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    _ => {
                        let reply = session.query(&mix::send_on(&root, slot)?).await?;
                        osc::is_on(&reply).map(ParamValue::Bool)
                    }
                }
            }
            _ => anyhow::bail!("{}: unknown param {param:?}", self.name()),
        }
    }
}
