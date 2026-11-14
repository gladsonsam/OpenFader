//! Main/alternate input patching and selected physical preamp addressing.
use crate::{
    mapping::{mapping, root, Mapping, WireKind},
    WingDriver,
};
use anyhow::{ensure, Context};
use openfader_core::{ParamValue, StripId};
use rosc::OscType;
pub const ROUTING_GROUPS: &[(&str, &str, u8)] = &[
    ("OFF", "Off", 0),
    ("LCL", "Local", 24),
    ("AUX", "Aux", 8),
    ("A", "AES50 A", 48),
    ("B", "AES50 B", 48),
    ("C", "AES50 C", 48),
    ("SC", "StageConnect", 32),
    ("USB", "USB", 48),
    ("CRD", "Card", 64),
    ("MOD", "Module", 64),
    ("PLAY", "USB player", 4),
    ("AES", "AES/EBU", 2),
    ("USR", "User signals", 48),
    ("OSC", "Oscillator", 2),
    ("BUS", "Bus", 16),
    ("MAIN", "Main", 4),
    ("MTX", "Matrix", 8),
];
fn route_prefix(strip: StripId, target: &str) -> anyhow::Result<String> {
    ensure!(
        strip.0 < 48,
        "WING: only channels and aux inputs have input patches"
    );
    ensure!(
        matches!(target, "" | "main" | "alt"),
        "WING: unknown routing target"
    );
    Ok(format!(
        "{}/in/conn/{}",
        root(strip)?.0,
        if target == "alt" { "alt" } else { "" }
    ))
}

impl WingDriver {
    /// Physical local sockets differ between the WING variants. Unknown
    /// identities expose no assumed local socket count.
    pub fn local_input_count(&self) -> u8 {
        let link = self.link.read().unwrap();
        match link
            .as_ref()
            .map(|link| link.info.model.to_ascii_lowercase())
            .as_deref()
        {
            Some("ngc-full" | "wing-bk") => 8,
            Some("wing-compact" | "wing-rack") => 24,
            _ => 0,
        }
    }

    pub(crate) async fn source_id(&self, strip: StripId, target: &str) -> anyhow::Result<f32> {
        let label = self.routing_source(strip, target).await?;
        if label == "-" {
            return Ok(0.);
        }
        let (group, index) = label.split_once(' ').context("invalid WING source")?;
        let group = ROUTING_GROUPS
            .iter()
            .position(|(g, _, _)| *g == group)
            .context("unknown WING source group")?;
        Ok((group * 1000 + index.parse::<usize>()?) as f32)
    }
    pub(crate) async fn preamp_mapping(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<Mapping> {
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
            .context("WING: input has no physical preamp")?;
        ensure!(
            matches!(group, "LCL" | "A" | "B" | "C"),
            "WING: selected source has no controllable preamp"
        );
        let index: u8 = index.parse()?;
        ensure!(
            (1..=if group == "LCL" {
                self.local_input_count()
            } else {
                48
            })
                .contains(&index),
            "WING: preamp out of range"
        );
        Ok(Mapping {
            path: format!(
                "/io/in/{group}/{index}/{}",
                if param == "gain" { "g" } else { "vph" }
            ),
            kind: if param == "gain" {
                WireKind::Float(-3., 45.5)
            } else {
                WireKind::Bool
            },
            model: None,
        })
    }
    pub async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> {
        let prefix = route_prefix(strip, target)?;
        let group = self
            .read(&Mapping {
                path: format!("{prefix}grp"),
                kind: WireKind::Text(16),
                model: None,
            })
            .await?;
        let ParamValue::Text(group) = group else {
            unreachable!()
        };
        if group == "OFF" {
            return Ok("-".into());
        }
        let ParamValue::Float(index) = self
            .read(&Mapping {
                path: format!("{prefix}in"),
                kind: WireKind::Int(1, 64),
                model: None,
            })
            .await?
        else {
            unreachable!()
        };
        Ok(format!("{group} {}", index as u8))
    }
    pub async fn patch(
        &self,
        strip: StripId,
        target: &str,
        group: &str,
        index: u8,
    ) -> anyhow::Result<String> {
        route_prefix(strip, target)?;
        let max = ROUTING_GROUPS
            .iter()
            .find(|(g, _, _)| *g == group)
            .map(|(_, _, n)| *n)
            .context("WING: unknown source group")?;
        let max = if group == "LCL" {
            self.local_input_count()
        } else {
            max
        };
        ensure!(
            group == "OFF" || (1..=max).contains(&index),
            "WING: source index out of range"
        );
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        // One documented node update carries both assignments (manual p.21).
        // The protocol does not guarantee atomic application of the pair;
        // confirm both fields afterward rather than assuming it did so.
        let alt = if target == "alt" { "alt" } else { "" };
        let node = if group == "OFF" {
            format!("{alt}grp=OFF")
        } else {
            format!("{alt}grp={group},{alt}in={index}")
        };
        session
            .send(
                &format!("{}/in/conn", root(strip)?.0),
                vec![OscType::String(node)],
            )
            .await?;
        let actual = self.routing_source(strip, target).await?;
        let expected = if group == "OFF" {
            "-".into()
        } else {
            format!("{group} {index}")
        };
        ensure!(
            actual == expected,
            "WING: routing readback did not match requested source at {}/in/conn ({target:?}): expected {expected:?}, got {actual:?}", root(strip)?.0
        );
        Ok(actual)
    }
}
