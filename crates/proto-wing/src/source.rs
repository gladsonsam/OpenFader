//! Input topology controls. /in/set/$mode is read-only; editable M/ST/M-S
//! belongs to the shared `/io/in/<group>/<index>/mode` source, not a channel.
//! Mode writes carry the captured source identity and optionally old mode.
use crate::{
    mapping::{mapping, Mapping, WireKind},
    native, WingDriver, ROUTING_GROUPS,
};
use anyhow::{ensure, Context};
use openfader_core::{params::*, ParamValue, StripId};

pub const MODES: &[&str] = &["M", "ST", "M/S"];
pub const MODE: &str = "wing:source_mode";
pub const WRITABLE: &str = "wing:source_mode_writable";
pub const EFFECTIVE: &str = "wing:input_mode";
pub const AUTO: &str = "wing:source_auto";
pub const BALANCE: &str = "wing:input_balance";
pub const WIDTH: &str = "wing:width";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    token: String,
    path: String,
}
fn tagged(value: &str) -> Option<(&str, Option<&str>, &str)> {
    let (target, mode) = value.split_once(':')?;
    let (group, index) = target.split_once('/')?;
    let count = ROUTING_GROUPS.iter().find(|(g, _, _)| *g == group)?.2;
    let index: u8 = index.parse().ok()?;
    if index == 0 || index > count {
        return None;
    }
    let (old, new) = match mode.split_once('>') {
        Some((a, b)) => (Some(a), b),
        None => (None, mode),
    };
    if !MODES.contains(&new) || old.is_some_and(|m| !MODES.contains(&m)) {
        return None;
    }
    Some((target, old, new))
}
pub fn tagged_agree(read: &str, written: &str) -> bool {
    match (tagged(read), tagged(written)) {
        (Some((a, _, ma)), Some((b, _, mb))) => a == b && ma == mb,
        _ => false,
    }
}
impl WingDriver {
    async fn source_mode_target(&self, strip: StripId) -> anyhow::Result<Target> {
        let selector = self.read(&mapping(strip, "source_select")?).await?;
        let source = self
            .routing_source(
                strip,
                if selector == ParamValue::Float(1.) {
                    "alt"
                } else {
                    "main"
                },
            )
            .await?;
        let (group, index) = source
            .split_once(' ')
            .context("WING: no source mode for Off input")?;
        ensure!(
            matches!(
                group,
                "LCL"
                    | "AUX"
                    | "A"
                    | "B"
                    | "C"
                    | "SC"
                    | "USB"
                    | "CRD"
                    | "MOD"
                    | "PLAY"
                    | "AES"
                    | "USR"
                    | "OSC"
            ),
            "WING: internal source mode is read-only; use the effective input mode"
        );
        let index: u8 = index.parse()?;
        let count = ROUTING_GROUPS
            .iter()
            .find(|(g, _, _)| *g == group)
            .context("WING: unknown source group")?
            .2;
        ensure!(
            (1..=count).contains(&index),
            "WING: source index out of range"
        );
        ensure!(
            self.read(&mapping(strip, "source_select")?).await? == selector,
            "WING: selected source changed during read"
        );
        Ok(Target {
            token: format!("{group}/{index}"),
            path: format!("/io/in/{group}/{index}/mode"),
        })
    }
    pub async fn source_mode_value(&self, strip: StripId) -> anyhow::Result<String> {
        self.source_mode_read(strip, false).await
    }
    pub async fn source_mode_capture(&self, strip: StripId) -> anyhow::Result<String> {
        self.source_mode_read(strip, true).await
    }
    async fn source_mode_read(
        &self,
        strip: StripId,
        writable_only: bool,
    ) -> anyhow::Result<String> {
        let session = self.session()?;
        let target = self.source_mode_target(strip).await?;
        if writable_only {
            let def = self.parameter_definition(&target.path).await?;
            ensure!(!def.read_only, "WING: source mode is read-only");
        }
        let value = self
            .read(&Mapping {
                path: target.path.clone(),
                kind: WireKind::Enum(MODES),
                model: None,
            })
            .await?;
        ensure!(
            self.source_mode_target(strip).await? == target,
            "WING: selected source changed during read"
        );
        self.ensure_current(&session)?;
        let ParamValue::Float(mode) = value else {
            unreachable!()
        };
        Ok(format!("{}:{}", target.token, MODES[mode as usize]))
    }
    pub async fn source_mode_writable(&self, strip: StripId) -> anyhow::Result<bool> {
        let session = self.session()?;
        let target = self.source_mode_target(strip).await?;
        let def = self.parameter_definition(&target.path).await?;
        ensure!(
            self.source_mode_target(strip).await? == target,
            "WING: selected source changed during read"
        );
        self.ensure_current(&session)?;
        let known = match &def.kind {
            native::Kind::Enum(choices) => {
                choices.len() == MODES.len()
                    && MODES
                        .iter()
                        .all(|m| choices.iter().any(|(name, _)| name == m))
            }
            _ => false,
        };
        Ok(!def.read_only && known)
    }
    pub async fn set_source_mode(&self, strip: StripId, tagged_value: &str) -> anyhow::Result<()> {
        let (token, old, new) =
            tagged(tagged_value).context("WING: source mode requires source-tagged text")?;
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        let target = self.source_mode_target(strip).await?;
        ensure!(
            target.token == token,
            "WING: selected source changed; reopen the source mode control"
        );
        let def = self.parameter_definition(&target.path).await?;
        ensure!(!def.read_only, "WING: source mode is read-only");
        let native::Kind::Enum(choices) = &def.kind else {
            anyhow::bail!("WING: unsupported source mode definition")
        };
        ensure!(
            choices.iter().any(|(name, _)| name == new),
            "WING: source mode unavailable on this firmware"
        );
        let mapping = Mapping {
            path: target.path.clone(),
            kind: WireKind::Enum(MODES),
            model: None,
        };
        if let Some(old) = old {
            let expected = MODES.iter().position(|m| *m == old).unwrap() as f32;
            ensure!(
                self.read(&mapping).await? == ParamValue::Float(expected),
                "WING: source mode changed; reopen the control"
            );
        }
        ensure!(
            self.source_mode_target(strip).await? == target,
            "WING: selected source changed; reopen the source mode control"
        );
        self.ensure_current(&session)?;
        let desired = ParamValue::Float(MODES.iter().position(|m| *m == new).unwrap() as f32);
        session
            .send(&mapping.path, vec![mapping.encode(desired.clone())?])
            .await?;
        ensure!(
            self.read(&mapping).await? == desired,
            "WING: source mode readback did not match"
        );
        ensure!(
            self.source_mode_target(strip).await? == target,
            "WING: selected source changed during write"
        );
        self.ensure_current(&session)
    }
}

pub const DEFS: &[ParamDef] = &[
    def(
        MODE,
        "Shared source mode",
        ParamKind::Text,
        0.,
        0.,
        "",
        SCOPES_INPUT_AUX,
    ),
    def(
        WRITABLE,
        "Source mode writable",
        ParamKind::Bool,
        0.,
        1.,
        "",
        SCOPES_INPUT_AUX,
    ),
    def(
        EFFECTIVE,
        "Effective input mode",
        ParamKind::Enum,
        0.,
        2.,
        "",
        SCOPES_INPUT_AUX,
    ),
    def(
        AUTO,
        "Automatic alternate source",
        ParamKind::Bool,
        0.,
        1.,
        "",
        SCOPES_INPUT_AUX,
    ),
    def(
        BALANCE,
        "Input balance",
        ParamKind::Float,
        -9.,
        9.,
        "dB",
        SCOPES_INPUT_AUX,
    ),
    def(
        WIDTH,
        "Stereo width",
        ParamKind::Float,
        -150.,
        150.,
        "%",
        crate::presentation::DSP,
    ),
];
const SCOPES_INPUT_AUX: &[openfader_core::StripKind] = crate::presentation::INPUTS;
const fn def(
    id: &'static str,
    label: &'static str,
    kind: ParamKind,
    min: f32,
    max: f32,
    unit: &'static str,
    scopes: &'static [openfader_core::StripKind],
) -> ParamDef {
    ParamDef {
        id,
        label,
        kind,
        scope: ParamScope::Channel,
        scopes,
        min,
        max,
        unit,
        default: match kind {
            ParamKind::Bool => ParamDefault::Bool(false),
            ParamKind::Text => ParamDefault::Text(""),
            _ => ParamDefault::Float(0.),
        },
    }
}
pub const PRESENTATIONS: &[ParamPresentation] = &[ParamPresentation {
    id: EFFECTIVE,
    when: None,
    choices: &[
        ParamChoice {
            value: 0.,
            label: "Mono",
        },
        ParamChoice {
            value: 1.,
            label: "Stereo",
        },
        ParamChoice {
            value: 2.,
            label: "Mid/side",
        },
    ],
    value_labels: &[],
    limiter_value: None,
    hidden: false,
    curve: None,
}];
