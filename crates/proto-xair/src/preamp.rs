//! Active physical preamp resolution. The OSC protocol has no compare-and-swap;
//! guarded writes pin the physical address and re-read the route before send.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveSource {
    pub(super) mode: i32,
    pub(super) index: i32,
}

#[derive(Clone, Copy)]
struct PreampTarget {
    headamp: u16,
    max_db: f32,
    phantom: bool,
}

impl ActiveSource {
    fn label(&self) -> String {
        input::source_label(self.mode, self.index)
    }
    fn preamp(&self) -> anyhow::Result<PreampTarget> {
        anyhow::ensure!(
            self.mode == 0 && (0..=17).contains(&self.index),
            "xair: active source has no controllable physical preamp"
        );
        Ok(PreampTarget {
            headamp: self.index as u16 + 1,
            max_db: if self.index < 16 { 60.0 } else { 20.0 },
            phantom: self.index < 16,
        })
    }
}

impl XAirDriver {
    pub(super) async fn active_source(&self, strip: StripId) -> anyhow::Result<ActiveSource> {
        let ch = self.check_channel(strip.0)?;
        let session = self.session().await?.as_ref().unwrap().clone();
        let mode_addr = format!("/ch/{ch:02}/preamp/rtnsw");
        let mode = input::source_selector(&session.query(&mode_addr).await?, 1)?;
        let addr = if mode == 0 {
            format!("/ch/{ch:02}/config/insrc")
        } else {
            input::config_rtnsrc(ch)
        };
        let index = input::source_selector(
            &session.query(&addr).await?,
            if mode == 0 { 18 } else { 17 },
        )?;
        anyhow::ensure!(
            input::source_selector(&session.query(&mode_addr).await?, 1)? == mode,
            "xair: input source mode changed while reading"
        );
        Ok(ActiveSource { mode, index })
    }

    pub(super) async fn active_source_label(&self, strip: StripId) -> anyhow::Result<String> {
        Ok(self.active_source(strip).await?.label())
    }

    pub(super) async fn active_preamp_state(&self, strip: StripId) -> anyhow::Result<String> {
        let source = self.active_source(strip).await?;
        let target = source.preamp().ok();
        Ok(serde_json::json!({
            "source": source.label(),
            "gain_available": target.is_some(),
            "gain_min": target.map(|_| -12.0),
            "gain_max": target.map(|t| t.max_db),
            "phantom_available": target.is_some_and(|t| t.phantom),
        })
        .to_string())
    }

    pub(super) async fn read_active_preamp(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<ParamValue> {
        let source = self.active_source(strip).await?;
        let target = source.preamp()?;
        anyhow::ensure!(
            param != "phantom" || target.phantom,
            "xair: active source has no phantom power"
        );
        let session = self.session().await?.as_ref().unwrap().clone();
        let reply = match param {
            "gain" => {
                session
                    .query(&headamp::headamp_gain(target.headamp))
                    .await?
            }
            "phantom" => {
                session
                    .query(&headamp::headamp_phantom(target.headamp))
                    .await?
            }
            _ => anyhow::bail!("xair: invalid preamp parameter"),
        };
        anyhow::ensure!(
            self.active_source(strip).await? == source,
            "xair: input source changed during preamp read"
        );
        match param {
            "gain" => {
                let wire = osc::fader_value(&reply)?;
                anyhow::ensure!(
                    wire.is_finite() && (0.0..=1.0).contains(&wire),
                    "xair: invalid headamp gain"
                );
                Ok(ParamValue::Float(-12.0 + (target.max_db + 12.0) * wire))
            }
            _ => osc::is_on(&reply).map(ParamValue::Bool),
        }
    }

    pub async fn write_active_preamp_guarded(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &BTreeMap<String, ParamValue>,
    ) -> anyhow::Result<()> {
        self.write_active_preamp_guarded_checked(strip, param, value, expected, &|_| Ok(()))
            .await
    }

    pub async fn write_active_preamp_guarded_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &BTreeMap<String, ParamValue>,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let expected_source = expected
            .get("source")
            .ok_or_else(|| anyhow::anyhow!("xair: preamp write requires captured active source"))?;
        let _lane = self.preamp_write.lock().await;
        let generation = self
            .connection_generation
            .load(std::sync::atomic::Ordering::Acquire);
        let source = self.active_source(strip).await?;
        let target = source.preamp()?;
        let members = self.preamp_control_members(strip).await?;
        check(&members)?;
        anyhow::ensure!(
            *expected_source == ParamValue::Text(source.label()),
            "xair: input source changed; reopen the control"
        );
        for (id, value) in expected {
            if id != "source" {
                anyhow::ensure!(
                    self.get_param(strip, id).await? == *value,
                    "xair: input source changed; reopen the control"
                );
            }
        }
        let (address, args) = match (param, value) {
            ("gain", ParamValue::Float(db)) => {
                anyhow::ensure!(
                    db.is_finite() && (-12.0..=target.max_db).contains(&db),
                    "xair: gain outside active preamp range"
                );
                (
                    headamp::headamp_gain(target.headamp),
                    vec![OscType::Float((db + 12.0) / (target.max_db + 12.0))],
                )
            }
            ("phantom", ParamValue::Bool(on)) if target.phantom => (
                headamp::headamp_phantom(target.headamp),
                vec![codec::bool_arg(on)],
            ),
            _ => anyhow::bail!("xair: invalid preamp value"),
        };
        anyhow::ensure!(
            self.active_source(strip).await? == source
                && self
                    .connection_generation
                    .load(std::sync::atomic::Ordering::Acquire)
                    == generation,
            "xair: input source or connection changed; reopen the control"
        );
        anyhow::ensure!(
            self.preamp_control_members(strip).await? == members,
            "xair: shared preamp members changed; reopen the control"
        );
        check(&members)?;
        let session = self.session().await?.as_ref().unwrap().clone();
        anyhow::ensure!(
            self.connection_generation
                .load(std::sync::atomic::Ordering::Acquire)
                == generation,
            "xair: connection changed; reopen the control"
        );
        session.send(&address, args).await
    }

    pub(super) async fn preamp_control_members(
        &self,
        strip: StripId,
    ) -> anyhow::Result<Vec<StripId>> {
        let source = self.active_source(strip).await?;
        let target = source.preamp()?;
        let mut members = self.stereo_control_members(strip, "gain")?;
        for i in 0..16_u16 {
            let candidate = self.active_source(StripId(i)).await?;
            if candidate == source && !members.contains(&StripId(i)) {
                members.push(StripId(i));
            }
        }
        // The stereo aux return can also consume either physical aux side;
        // protect it even if its currently selected source is unknown.
        if !target.phantom && !members.contains(&StripId(35)) {
            members.push(StripId(35));
        }
        anyhow::ensure!(
            self.active_source(strip).await? == source,
            "xair: source changed during access check"
        );
        Ok(members)
    }

    pub async fn preamp_capture_applicable(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<bool> {
        let source = self.active_source(strip).await?;
        if source.mode == 1 || source.index == 18 {
            return Ok(false);
        }
        Ok(param != "phantom" || source.index < 16)
    }
}
