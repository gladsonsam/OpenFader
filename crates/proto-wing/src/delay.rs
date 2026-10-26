//! Native delay values stay tagged with their unit, including in local scenes.
//! Ranges come from the live DD definition rather than the inconsistent static
//! delay tables. No distance/time conversion is inferred by OpenFader.
use crate::{
    mapping::{root, Mapping, WireKind, OUTPUTS},
    native, WingDriver,
};
use anyhow::{ensure, Context};
use openfader_core::{ParamValue, StripId, StripKind};
use rosc::OscType;

pub const UNITS: &[&str] = &["M", "FT", "MS", "SMP"];
pub const STATE: &str = "wing:delay_state";
pub const VALUE: &str = "wing:delay_value";
pub const UNIT: &str = "wing:delay_unit";

pub fn path(strip: StripId, field: &str) -> anyhow::Result<String> {
    let (base, kind) = root(strip)?;
    ensure!(kind != StripKind::Dca, "WING: DCA has no delay");
    Ok(if OUTPUTS.contains(&kind) {
        format!("{base}/dly/{field}")
    } else {
        format!(
            "{base}/in/set/{}",
            match field {
                "on" => "dlyon",
                "mode" => "dlymode",
                _ => "dly",
            }
        )
    })
}
fn leaf(strip: StripId, field: &str, kind: WireKind) -> anyhow::Result<Mapping> {
    Ok(Mapping {
        path: path(strip, field)?,
        kind,
        model: None,
    })
}

#[derive(Clone, Debug)]
pub struct State {
    pub unit: usize,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub steps: i32,
    pub on: bool,
    pub editable: bool,
}
impl State {
    pub fn tagged_value(&self) -> String {
        format!("{}:{}", UNITS[self.unit], self.value)
    }
    pub fn json(&self) -> String {
        format!(
            r#"{{"unit":{},"value":{},"min":{},"max":{},"steps":{},"on":{},"editable":{}}}"#,
            self.unit, self.value, self.min, self.max, self.steps, self.on, self.editable
        )
    }
}
fn domain(def: &native::Definition) -> anyhow::Result<(WireKind, f32, f32, i32)> {
    let (wire, min, max, steps) = match def.kind {
        native::Kind::Linear { min, max, steps } => (
            WireKind::Float(min.min(max), min.max(max)),
            min.min(max),
            min.max(max),
            steps,
        ),
        native::Kind::Integer { min, max } => (
            WireKind::Int(min.min(max), min.max(max)),
            min.min(max) as f32,
            min.max(max) as f32,
            max.abs_diff(min).saturating_add(1).min(i32::MAX as u32) as i32,
        ),
        _ => anyhow::bail!("WING: delay numeric definition unavailable"),
    };
    ensure!(
        min.is_finite() && max.is_finite() && min < max && steps >= 0,
        "WING: invalid live delay range"
    );
    Ok((wire, min, max, steps))
}
/// Unit tags are semantic data; numeric spelling is not. Only compare actual
/// readback with the requested number, with the engine's ordinary f32 epsilon.
pub fn tagged_agree(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Option<(usize, f32)> {
        let (unit, value) = s.split_once(':')?;
        let unit = UNITS.iter().position(|u| *u == unit)?;
        let value: f32 = value.parse().ok()?;
        value.is_finite().then_some((unit, value))
    };
    match (parse(a), parse(b)) {
        (Some((ua, a)), Some((ub, b))) => ua == ub && (a - b).abs() <= 0.002,
        _ => false,
    }
}
fn number(value: ParamValue) -> anyhow::Result<f32> {
    let ParamValue::Float(value) = value else {
        anyhow::bail!("WING: invalid delay value")
    };
    Ok(value)
}
impl WingDriver {
    async fn delay_unit(&self, strip: StripId) -> anyhow::Result<usize> {
        Ok(number(
            self.read(&leaf(strip, "mode", WireKind::Enum(UNITS))?)
                .await?,
        )? as usize)
    }
    pub async fn delay_state(&self, strip: StripId) -> anyhow::Result<State> {
        let session = self.session()?;
        let unit = self.delay_unit(strip).await?;
        let def = self.parameter_definition(&path(strip, "dly")?).await?;
        let (kind, min, max, steps) = domain(&def)?;
        let value = number(self.read(&leaf(strip, "dly", kind)?).await?)?;
        let ParamValue::Bool(on) = self.read(&leaf(strip, "on", WireKind::Bool)?).await? else {
            unreachable!()
        };
        ensure!(
            self.delay_unit(strip).await? == unit,
            "WING: delay unit changed during read"
        );
        self.ensure_current(&session)?;
        Ok(State {
            unit,
            value,
            min,
            max,
            steps,
            on,
            editable: !def.read_only,
        })
    }
    pub async fn set_delay_value(&self, strip: StripId, tagged: &str) -> anyhow::Result<()> {
        let (unit, value) = tagged
            .split_once(':')
            .context("WING: delay value requires a unit tag")?;
        let unit = UNITS
            .iter()
            .position(|u| *u == unit)
            .context("WING: unknown delay unit")?;
        let value: f32 = value.parse().context("WING: invalid delay number")?;
        ensure!(value.is_finite(), "WING: invalid delay number");
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        ensure!(
            self.delay_unit(strip).await? == unit,
            "WING: delay unit changed; reopen the control"
        );
        let def = self.parameter_definition(&path(strip, "dly")?).await?;
        ensure!(!def.read_only, "WING: delay value is read-only");
        let (kind, min, max, steps) = domain(&def)?;
        let mapping = leaf(strip, "dly", kind)?;
        let argument = mapping.encode(ParamValue::Float(value))?;
        ensure!(
            self.delay_unit(strip).await? == unit,
            "WING: delay unit changed; reopen the control"
        );
        self.ensure_current(&session)?;
        session.send(&mapping.path, vec![argument]).await?;
        let actual = number(self.read(&mapping).await?)?;
        ensure!(
            self.delay_unit(strip).await? == unit,
            "WING: delay unit changed during write"
        );
        self.ensure_current(&session)?;
        let tolerance = if steps > 1 {
            (max - min) / (steps - 1) as f32 / 2.
        } else {
            0.
        };
        ensure!(
            (actual - value).abs() <= tolerance + 0.0001,
            "WING: delay readback did not match the requested value"
        );
        Ok(())
    }
    pub async fn set_delay_switch(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        let mapping = if param == "delay_on" {
            leaf(strip, "on", WireKind::Bool)?
        } else {
            leaf(strip, "mode", WireKind::Enum(UNITS))?
        };
        let argument: OscType = mapping.encode(value.clone())?;
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session.send(&mapping.path, vec![argument]).await?;
        ensure!(
            self.read(&mapping).await? == value,
            "WING: delay switch readback did not match"
        );
        self.ensure_current(&session)
    }
}
