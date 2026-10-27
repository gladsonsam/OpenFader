//! Guarded command validation: source pinning, model checks and strip writes.
use crate::{
    delay,
    groups::{self, group_assignment, tag_mapping},
    mapping::{self, mapping, root, Mapping, WireKind, STRIPS},
    routing::ROUTING_GROUPS,
    source, WingDriver,
};
use anyhow::{ensure, Context};
use openfader_core::{MixerDriver, ParamValue, StripId};
use rosc::OscType;
pub(crate) fn number(v: ParamValue) -> anyhow::Result<f32> {
    if let ParamValue::Float(v) = v {
        Ok(v)
    } else {
        anyhow::bail!("WING: expected number")
    }
}
pub(crate) fn boolean(v: ParamValue) -> anyhow::Result<bool> {
    if let ParamValue::Bool(v) = v {
        Ok(v)
    } else {
        anyhow::bail!("WING: expected bool")
    }
}

impl WingDriver {
    pub(crate) async fn check_model(&self, strip: StripId, m: &Mapping) -> anyhow::Result<()> {
        if let Some((block, model)) = m.model {
            let path = format!("{}/{block}/mdl", root(strip)?.0);
            let current = self
                .read(&Mapping {
                    path,
                    kind: WireKind::Text(32),
                    model: None,
                })
                .await?;
            ensure!(current == ParamValue::Text(model.into()), "WING: this control requires the {model} model; the loaded model has different parameters");
        }
        Ok(())
    }
    pub async fn set_guarded(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &std::collections::BTreeMap<String, ParamValue>,
    ) -> anyhow::Result<()> {
        if matches!(param, "source_a" | "source_b") {
            let ParamValue::Float(value) = value else {
                anyhow::bail!("WING: source requires numeric source ID")
            };
            ensure!(
                value.is_finite() && value >= 0. && value.fract() == 0.,
                "WING: invalid source ID"
            );
            let id = value as usize;
            let group = ROUTING_GROUPS
                .get(id / 1000)
                .context("WING: source group out of range")?
                .0;
            ensure!(group != "OFF" || id == 0, "WING: invalid Off source ID");
            let index = (id % 1000)
                .try_into()
                .context("WING: source index out of range")?;
            self.patch(
                strip,
                if param == "source_a" { "main" } else { "alt" },
                group,
                index,
            )
            .await?;
            return Ok(());
        }
        if param == source::MODE {
            let ParamValue::Text(tagged) = value else {
                anyhow::bail!("WING: source mode requires source-tagged text")
            };
            return self.set_source_mode(strip, &tagged).await;
        }
        ensure!(
            !matches!(param, source::EFFECTIVE | source::WRITABLE),
            "WING: input mode metadata is read-only"
        );
        if param == delay::VALUE {
            let ParamValue::Text(tagged) = value else {
                anyhow::bail!("WING: delay value requires unit-tagged text")
            };
            return self.set_delay_value(strip, &tagged).await;
        }
        if param == "delay_on" || param == delay::UNIT {
            return self.set_delay_switch(strip, param, value).await;
        }
        ensure!(param != delay::STATE, "WING: delay state is read-only");
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        if let Some((kind, group)) = group_assignment(param) {
            let ParamValue::Bool(member) = value else {
                anyhow::bail!("WING: assignment requires boolean")
            };
            let m = tag_mapping(strip)?;
            let ParamValue::Text(tags) = self.read(&m).await? else {
                unreachable!()
            };
            let tags = groups::edit(&tags, kind, group, member)?;
            self.ensure_current(&session)?;
            session
                .send(&m.path, vec![OscType::String(tags.clone())])
                .await?;
            let actual = self.read(&m).await?;
            self.ensure_current(&session)?;
            ensure!(
                actual == ParamValue::Text(tags),
                "WING: group tag readback did not match the requested edit"
            );
            return Ok(());
        }
        let captured_preamp_source = expected.contains_key("source")
            || match expected.get("source_select") {
                Some(ParamValue::Float(value)) if *value == 0. => expected.contains_key("source_a"),
                Some(ParamValue::Float(value)) if *value == 1. => expected.contains_key("source_b"),
                _ => false,
            };
        if matches!(param, "gain" | "phantom") && !captured_preamp_source {
            ensure!(self.get_param(strip, source::AUTO).await? == ParamValue::Bool(false),
                "WING: automatic source switching requires a captured source context for preamp writes");
        }
        let m = if matches!(param, "gain" | "phantom") {
            self.preamp_mapping(strip, param).await?
        } else {
            mapping(strip, param)?
        };
        if param == "eq_mode" {
            let (_, kind) = root(strip)?;
            if let ParamValue::Float(index) = &value {
                let model = mapping::EQ_MODELS
                    .get(*index as usize)
                    .copied()
                    .context("WING: invalid EQ model")?;
                ensure!(
                    kind != openfader_core::StripKind::Ret || model != "MACH4",
                    "WING: aux EQ does not offer MACH4"
                );
                ensure!(
                    kind != openfader_core::StripKind::Mtx || model != "MACH4",
                    "WING: matrix EQ does not offer MACH4"
                );
                ensure!(
                    model != "PIA" || mapping::OUTPUTS.contains(&kind),
                    "WING: PIA requires an output EQ"
                );
            }
        }
        if root(strip)?.1 == openfader_core::StripKind::Bus {
            if let Some(slot) = param
                .strip_prefix("send")
                .and_then(|p| p.split_once('_'))
                .and_then(|(n, _)| n.parse::<u16>().ok())
            {
                ensure!(
                    slot > 16 || strip.0 - 48 + 1 != slot,
                    "WING: a bus cannot send to itself"
                );
            }
        }
        let arg = m.encode(value)?;
        for (id, wanted) in expected {
            ensure!(
                self.get_param(strip, id).await? == *wanted,
                "Input source changed; reopen the control."
            );
        }
        if matches!(param, "gain" | "phantom") {
            ensure!(
                self.preamp_mapping(strip, param).await?.path == m.path,
                "Input source changed; reopen the control."
            );
        }
        self.check_model(strip, &m).await?;
        // The physical address was pinned before checking context. A later
        // desk-side route change cannot redirect this command to another preamp.
        self.ensure_current(&session)?;
        session.send(&m.path, vec![arg]).await
    }
    pub async fn clear_solos(&self) -> anyhow::Result<()> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        for strip in 0..STRIPS as u16 {
            session
                .send(
                    &format!("{}/$solo", root(StripId(strip))?.0),
                    vec![OscType::Int(0)],
                )
                .await?;
        }
        Ok(())
    }
}
