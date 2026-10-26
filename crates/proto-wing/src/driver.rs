//! MixerDriver adapter; domain operations live in sibling modules.
use crate::{
    catalogue::{capabilities, RANGES, SWATCHES},
    control::{boolean, number},
    delay,
    groups::{self, group_assignment, tag_mapping},
    mapping::{self, mapping, root, Mapping, WireKind, STRIPS, TOPOLOGY},
    presentation, source, WingDriver,
};
use anyhow::{ensure, Context};
use openfader_core::{
    params::{ParamDef, ParamTextRules},
    ChannelStrip, MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue,
    StripFamily, StripId,
};
use rosc::OscType;
use std::time::Instant;
use tokio::sync::broadcast;

impl MixerDriver for WingDriver {
    fn name(&self) -> &'static str {
        "wing"
    }
    fn channel_count(&self) -> usize {
        40
    }
    fn strip_count(&self) -> usize {
        STRIPS
    }
    fn topology(&self) -> Vec<StripFamily> {
        TOPOLOGY.to_vec()
    }
    fn bus_count(&self) -> u8 {
        16
    }
    fn mute_group_count(&self) -> u8 {
        8
    }
    fn capabilities(&self) -> &'static [&'static str] {
        capabilities()
    }
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        RANGES
    }
    fn param_defs(&self) -> &'static [ParamDef] {
        presentation::defs()
    }
    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        presentation::presentations()
    }
    fn param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        if param == "color" {
            &SWATCHES
        } else {
            &[]
        }
    }
    fn param_text_rules(&self, param: &str) -> Option<ParamTextRules> {
        (param == "name").then_some(ParamTextRules {
            max_utf8_bytes: Some(16),
            allow_empty: true,
            allow_edge_whitespace: true,
        })
    }
    fn meter_taps(&self) -> &'static [&'static str] {
        &["in", "post"]
    }
    fn poll_meters(&self) -> MeterFrame {
        self.meter_sample()
            .map(|s| MeterFrame {
                levels: s.levels.iter().map(|v| v.input).collect(),
            })
            .unwrap_or_else(|| MeterFrame::silent(STRIPS))
    }
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<ParamUpdate>> {
        Some(self.updates.subscribe())
    }
    fn telemetry_timestamps(&self) -> Option<(Option<Instant>, Option<Instant>)> {
        Some((
            *self.last_control.lock().unwrap(),
            self.meter_sample().map(|s| s.received),
        ))
    }
    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.connect_session(addr).await
    }
    async fn refresh_streams(&self) -> anyhow::Result<()> {
        self.refresh_meter_streams().await
    }
    async fn disconnect(&self) -> anyhow::Result<()> {
        self.disconnect_session().await
    }
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        if param == source::MODE {
            return self.source_mode_value(strip).await.map(ParamValue::Text);
        }
        if param == source::WRITABLE {
            return self.source_mode_writable(strip).await.map(ParamValue::Bool);
        }

        if param == delay::STATE || param == delay::VALUE {
            let state = self.delay_state(strip).await?;
            return Ok(ParamValue::Text(if param == delay::STATE {
                state.json()
            } else {
                state.tagged_value()
            }));
        }

        if param == "wing:eq_bands" {
            self.session()?;
            let (_, kind) = root(strip)?;
            ensure!(
                kind != openfader_core::StripKind::Dca,
                "WING: DCA has no EQ"
            );
            return Ok(ParamValue::Float(if mapping::OUTPUTS.contains(&kind) {
                8.
            } else {
                6.
            }));
        }
        if let Some((kind, group)) = group_assignment(param) {
            let tags = self.read(&tag_mapping(strip)?).await?;
            let ParamValue::Text(tags) = tags else {
                unreachable!()
            };
            return groups::member(&tags, kind, group).map(ParamValue::Bool);
        }
        if param == "source" {
            let selected = self.read(&mapping(strip, "source_select")?).await?;
            return self
                .routing_source(
                    strip,
                    if selected == ParamValue::Float(1.) {
                        "alt"
                    } else {
                        "main"
                    },
                )
                .await
                .map(ParamValue::Text);
        }
        if param == "source_a" {
            return self.source_id(strip, "main").await.map(ParamValue::Float);
        }
        if param == "source_b" {
            return self.source_id(strip, "alt").await.map(ParamValue::Float);
        }
        let m = if matches!(param, "gain" | "phantom") {
            self.preamp_mapping(strip, param).await?
        } else {
            mapping(strip, param)?
        };
        self.check_model(strip, &m).await?;
        let value = self.read(&m).await?;
        self.check_model(strip, &m).await?;
        Ok(value)
    }
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.set_guarded(strip, param, value, &Default::default())
            .await
    }
    async fn set_fader(&self, strip: StripId, v: f32) -> anyhow::Result<()> {
        self.set_param(strip, "fader", ParamValue::Float(v)).await
    }
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        number(self.get_param(strip, "fader").await?)
    }
    async fn set_mute(&self, strip: StripId, v: bool) -> anyhow::Result<()> {
        self.set_param(strip, "mute", ParamValue::Bool(v)).await
    }
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        boolean(self.get_param(strip, "mute").await?)
    }
    async fn set_name(&self, strip: StripId, v: &str) -> anyhow::Result<()> {
        self.set_param(strip, "name", ParamValue::Text(v.into()))
            .await
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        if let ParamValue::Text(v) = self.get_param(strip, "name").await? {
            Ok(v)
        } else {
            anyhow::bail!("invalid WING name")
        }
    }
    async fn set_solo(&self, strip: StripId, v: bool) -> anyhow::Result<()> {
        self.set_param(strip, "solo", ParamValue::Bool(v)).await
    }
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        boolean(self.get_param(strip, "solo").await?)
    }
    async fn set_bus_fader(&self, bus: u8, v: f32) -> anyhow::Result<()> {
        ensure!(bus < 16, "bus out of range");
        self.set_fader(StripId(48 + bus as u16), v).await
    }
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        ensure!(bus < 16, "bus out of range");
        self.get_fader(StripId(48 + bus as u16)).await
    }
    async fn set_bus_mute(&self, bus: u8, v: bool) -> anyhow::Result<()> {
        ensure!(bus < 16, "bus out of range");
        self.set_mute(StripId(48 + bus as u16), v).await
    }
    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        ensure!(bus < 16, "bus out of range");
        self.get_mute(StripId(48 + bus as u16)).await
    }
    async fn set_main_fader(&self, v: f32) -> anyhow::Result<()> {
        self.set_fader(StripId(64), v).await
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        self.get_fader(StripId(64)).await
    }
    async fn set_main_mute(&self, v: bool) -> anyhow::Result<()> {
        self.set_mute(StripId(64), v).await
    }
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        self.get_mute(StripId(64)).await
    }
    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        self.read_fx(slot, param, None).await
    }
    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        self.write_fx(slot, param, value).await
    }
    async fn get_mute_group_assign(&self, strip: StripId, group: u8) -> anyhow::Result<bool> {
        ensure!(group < 8, "WING: mute group out of range");
        boolean(self.get_param(strip, &format!("_mg{}", group + 1)).await?)
    }
    async fn set_mute_group_assign(
        &self,
        strip: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        ensure!(group < 8, "WING: mute group out of range");
        self.set_param(
            strip,
            &format!("_mg{}", group + 1),
            ParamValue::Bool(member),
        )
        .await
    }
    async fn set_mute_group(&self, group: u8, v: bool) -> anyhow::Result<()> {
        ensure!(group < 8, "mute group out of range");
        self.session()?
            .send(
                &format!("/mgrp/{}/mute", group + 1),
                vec![OscType::Int(i32::from(v))],
            )
            .await
    }
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        ensure!(group < 8, "mute group out of range");
        boolean(
            self.read(&Mapping {
                path: format!("/mgrp/{}/mute", group + 1),
                kind: WireKind::Bool,
                model: None,
            })
            .await?,
        )
    }
    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        self.link
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.info.clone())
            .context("WING: not connected")
    }
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        let mut strips = Vec::with_capacity(STRIPS);
        for i in 0..STRIPS as u16 {
            let id = StripId(i);
            strips.push(ChannelStrip {
                id,
                name: self.get_name(id).await?,
                fader: self.get_fader(id).await?,
                mute: self.get_mute(id).await?,
                pan: if i < 76 {
                    number(self.get_param(id, "pan").await?)?
                } else {
                    0.
                },
            });
        }
        Ok(MixerSnapshot { strips })
    }
    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        // Validate all rows before emitting the first mutation.
        for row in &snapshot.strips {
            mapping(row.id, "fader")?.encode(ParamValue::Float(row.fader))?;
            mapping(row.id, "name")?.encode(ParamValue::Text(row.name.clone()))?;
            if row.id.0 < 76 {
                mapping(row.id, "pan")?.encode(ParamValue::Float(row.pan))?;
            }
        }
        for row in &snapshot.strips {
            self.set_name(row.id, &row.name).await?;
            self.set_fader(row.id, row.fader).await?;
            self.set_mute(row.id, row.mute).await?;
            if row.id.0 < 76 {
                self.set_param(row.id, "pan", ParamValue::Float(row.pan))
                    .await?;
            }
        }
        Ok(())
    }
}
