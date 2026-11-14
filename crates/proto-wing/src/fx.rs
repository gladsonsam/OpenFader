//! Model-specific FX parameter metadata from a WING Compact 3.0.5 schema.
//! See schema/README.md for provenance and the upstream Apache-2.0 license.
use crate::mapping::{Mapping, WireKind};
use openfader_core::ParamValue;
use rosc::OscType;

#[derive(Clone, Copy, Debug)]
pub enum FxKind {
    Linear,
    Log,
    Int,
    Enum,
    FloatEnum,
    Fader,
}
#[derive(Clone, Copy, Debug)]
pub struct FxParameter {
    pub name: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub kind: FxKind,
    pub min: f32,
    pub max: f32,
    pub steps: u32,
    pub choices: &'static [(&'static str, &'static str)],
    pub values: &'static [f32],
}
#[derive(Clone, Copy, Debug)]
pub struct FxModel {
    pub id: &'static str,
    pub label: &'static str,
    pub premium: bool,
    pub params: &'static [FxParameter],
}
include!("schema/fx.rs");

impl FxParameter {
    pub fn encode(self, v: f32) -> anyhow::Result<OscType> {
        anyhow::ensure!(
            v.is_finite() && (0.0..=1.0).contains(&v),
            "invalid WING FX value"
        );
        Ok(match self.kind {
            FxKind::Enum | FxKind::FloatEnum => {
                let i = (v * (self.choices.len() - 1) as f32).round() as usize;
                OscType::String(self.choices[i].0.to_string())
            }
            FxKind::Fader => OscType::Float(crate::mapping::fader_db(v)?),
            _ => {
                let v = if self.steps > 1 {
                    (v * (self.steps - 1) as f32).round() / (self.steps - 1) as f32
                } else {
                    v
                };
                let value = if matches!(self.kind, FxKind::Log) {
                    self.min * (self.max / self.min).powf(v)
                } else {
                    self.min + (self.max - self.min) * v
                };
                if matches!(self.kind, FxKind::Int) {
                    OscType::Int(value.round() as i32)
                } else {
                    OscType::Float(value)
                }
            }
        })
    }
    pub fn decode(self, msg: &rosc::OscMessage) -> anyhow::Result<f32> {
        let arg = match msg.args.as_slice() {
            [one] => one,
            [OscType::String(_), OscType::Float(_), value] => value,
            _ => anyhow::bail!("invalid WING FX reply"),
        };
        if matches!(self.kind, FxKind::Enum | FxKind::FloatEnum) {
            let index = match arg {
                OscType::String(s) => self.choices.iter().position(|(v, _)| *v == s),
                OscType::Float(v) => self.values.iter().position(|n| (*n - *v).abs() < 0.0001),
                _ => None,
            }
            .ok_or_else(|| anyhow::anyhow!("unknown WING FX enum"))?;
            return Ok(if self.choices.len() > 1 {
                index as f32 / (self.choices.len() - 1) as f32
            } else {
                0.
            });
        }
        let kind = match self.kind {
            FxKind::Int => {
                WireKind::Int(self.min.min(self.max) as i32, self.min.max(self.max) as i32)
            }
            FxKind::Fader => WireKind::Fader,
            _ => WireKind::Float(self.min.min(self.max), self.min.max(self.max)),
        };
        let ParamValue::Float(value) = (Mapping {
            path: msg.addr.clone(),
            kind,
            model: None,
        })
        .decode(msg)?
        else {
            anyhow::bail!("invalid FX value")
        };
        if matches!(self.kind, FxKind::Fader) {
            return Ok(value);
        }
        let unit = if matches!(self.kind, FxKind::Log) {
            (value / self.min).ln() / (self.max / self.min).ln()
        } else {
            (value - self.min) / (self.max - self.min)
        };
        anyhow::ensure!(unit.is_finite(), "invalid WING FX range");
        Ok(unit.clamp(0., 1.))
    }
    pub fn scale(self) -> &'static str {
        match self.kind {
            FxKind::Log => "log",
            FxKind::Enum | FxKind::FloatEnum => "enum",
            _ => "lin",
        }
    }
}

use crate::{WingDriver, QUERY_TIMEOUT};
use anyhow::{ensure, Context};

impl WingDriver {
    pub async fn fx_model(&self, slot: u8) -> anyhow::Result<u8> {
        ensure!((1..=16).contains(&slot), "WING: FX slot out of range");
        let value = self
            .read(&Mapping {
                path: format!("/fx/{slot}/mdl"),
                kind: WireKind::Text(32),
                model: None,
            })
            .await?;
        let ParamValue::Text(id) = value else {
            unreachable!()
        };
        MODELS
            .iter()
            .position(|m| m.id == id)
            .map(|i| i as u8)
            .context("WING: unknown FX model")
    }
    pub async fn set_fx_model(&self, slot: u8, model: &str) -> anyhow::Result<()> {
        ensure!((1..=16).contains(&slot), "WING: FX slot out of range");
        let model = MODELS
            .iter()
            .find(|m| m.id == model || m.label == model)
            .context("WING: unknown FX model")?;
        ensure!(
            slot <= 8 || !model.premium,
            "WING: this model requires a premium FX slot (1–8)"
        );
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session
            .send(
                &format!("/fx/{slot}/mdl"),
                vec![OscType::String(model.id.into())],
            )
            .await?;
        ensure!(
            self.fx_model(slot).await?
                == MODELS.iter().position(|m| m.id == model.id).unwrap() as u8,
            "WING: FX model readback did not match"
        );
        self.ensure_current(&session)
    }
    pub async fn read_fx(&self, slot: u8, param: u8, expected: Option<u8>) -> anyhow::Result<f32> {
        let session = self.session()?;
        let actual = self.fx_model(slot).await?;
        ensure!(
            expected.is_none_or(|e| actual == e),
            "WING: FX model changed"
        );
        let model = &MODELS[actual as usize];
        let def = model
            .params
            .get(param as usize)
            .context("WING: FX parameter out of range")?;
        self.ensure_current(&session)?;
        let reply = session
            .query_timeout(&format!("/fx/{slot}/{}", def.name), QUERY_TIMEOUT)
            .await?;
        let value = def.decode(&reply)?;
        ensure!(
            self.fx_model(slot).await? == actual,
            "WING: FX model changed during read"
        );
        self.ensure_current(&session)?;
        Ok(value)
    }
    pub(crate) async fn write_fx(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        self.write_fx_checked(slot, param, value, None).await
    }

    pub async fn write_fx_checked(
        &self,
        slot: u8,
        param: u8,
        value: f32,
        expected: Option<u8>,
    ) -> anyhow::Result<()> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        let model = self.fx_model(slot).await?;
        ensure!(
            expected.is_none_or(|e| e == model),
            "WING: FX model changed before write"
        );
        let def = MODELS[model as usize]
            .params
            .get(param as usize)
            .context("WING: FX parameter out of range")?;
        let arg = def.encode(value)?;
        let path = format!("/fx/{slot}/{}", def.name);
        let encoded = def.decode(&rosc::OscMessage {
            addr: path.clone(),
            args: vec![arg.clone()],
        })?;
        ensure!(
            self.fx_model(slot).await? == model,
            "WING: FX model changed before write"
        );
        self.ensure_current(&session)?;
        session.send(&path, vec![arg]).await?;
        let reply = session.query_timeout(&path, QUERY_TIMEOUT).await?;
        let actual = def.decode(&reply).with_context(|| {
            format!(
                "WING: FX slot {slot} model {} parameter {} invalid readback {:?}",
                MODELS[model as usize].id, def.name, reply.args
            )
        })?;
        ensure!(
            self.fx_model(slot).await? == model,
            "WING: FX model changed during write"
        );
        self.ensure_current(&session)?;
        ensure!(
            (actual - encoded).abs() <= 0.002,
            "WING: FX slot {slot} model {} parameter {} readback mismatch: expected {encoded}, got {actual}, reply {:?}",
            MODELS[model as usize].id, def.name, reply.args
        );
        Ok(())
    }
}
