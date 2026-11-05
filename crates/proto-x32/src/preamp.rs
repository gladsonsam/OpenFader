//! Source-aware preamp path: gain/phantom follow the routed
//! shared headamp, never the channel index.
//!
//! `/ch/NN` and `/auxin/NN` carry no gain leaf of their own. The desk
//! resolves the actual headamp through `/-ha/[00..39]/index` (0..31 =
//! ch 01..32, 32..39 = aux 1..8; -1 = internal source with no headamp).
//! Gain (`-12..+60 dB`) and phantom are read/written at
//! `/headamp/[NNN]/gain|phantom` for the resolved index, and every
//! write re-resolves the index after the context check so a desk-side
//! route change cannot redirect the command to another preamp.
use crate::{
    mapping::{fam_of, ha_index_addr, Mapping, WireKind},
    X32Driver,
};
use anyhow::ensure;
use openfader_core::{ParamValue, StripId};
use std::collections::BTreeMap;

fn headamp_mapping(index: i32) -> anyhow::Result<(Mapping, Mapping)> {
    anyhow::ensure!(
        (0..=127).contains(&index),
        "x32: headamp index out of range"
    );
    let root = format!("/headamp/{index:03}");
    Ok((
        Mapping {
            path: format!("{root}/gain"),
            kind: WireKind::Gain,
        },
        Mapping {
            path: format!("{root}/phantom"),
            kind: WireKind::Bool,
        },
    ))
}

impl X32Driver {
    /// Resolve the routed headamp index for a channel/aux strip.
    /// `-1` (internal source) and non-channel strips are unavailable,
    /// never derived from the channel number.
    pub async fn resolve_headamp(&self, strip: StripId) -> anyhow::Result<i32> {
        let (root, fam) = fam_of(strip)?;
        let _ = root;
        ensure!(
            fam.is_ch || fam.is_aux,
            "x32: headamp gain/phantom is channel/aux only"
        );
        ensure!(
            self.source_label(strip).await? != ParamValue::Text("OFF".into()),
            "x32: input source is off; no active headamp"
        );
        let addr = ha_index_addr(strip)?;
        let index = self.read_int(&addr).await?;
        ensure!(
            (0..=127).contains(&index),
            "x32: channel has no routed headamp (internal source)"
        );
        Ok(index)
    }

    /// Current `source` label for the context guard.
    pub async fn source_label(&self, strip: StripId) -> anyhow::Result<ParamValue> {
        let (root, fam) = fam_of(strip)?;
        ensure!(fam.is_ch || fam.is_aux, "x32: source is channel/aux only");
        let index = self.read_int(&format!("{root}/config/source")).await?;
        Ok(ParamValue::Text(crate::addrs::source_label(index)?))
    }

    pub async fn preamp_capture_applicable(
        &self,
        strip: StripId,
        param: &str,
    ) -> anyhow::Result<bool> {
        let _ = param;
        Ok(self.resolve_headamp(strip).await.is_ok())
    }

    /// State readout for the generic preamp contract (`source` is
    /// produced by the same [`Self::source_label`] read as the `source`
    /// leaf). Channels and aux strips resolve the shared physical
    /// headamp through the documented 0..39 input index. Internal
    /// sources report gain/phantom unavailable.
    pub async fn preamp_state_text(&self, strip: StripId) -> anyhow::Result<String> {
        let source = match self.source_label(strip).await? {
            ParamValue::Text(label) => label,
            _ => anyhow::bail!("x32: invalid source label"),
        };
        let headamp = self.resolve_headamp(strip).await.ok();
        ensure!(
            self.source_label(strip).await? == ParamValue::Text(source.clone()),
            "x32: input source changed during preamp context read"
        );
        if let Some(index) = headamp {
            ensure!(
                self.resolve_headamp(strip).await? == index,
                "x32: routed headamp changed during preamp context read"
            );
        }
        Ok(serde_json::json!({
            "source": source,
            "headamp": headamp,
            "gain_available": headamp.is_some(),
            "gain_min": headamp.map(|_| -12.0),
            "gain_max": headamp.map(|_| 60.0),
            "phantom_available": headamp.is_some(),
        })
        .to_string())
    }

    /// Display read: resolved headamp value, or an error when no
    /// headamp is routed (the engine omits the value, never invents one).
    pub async fn preamp_display(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        let session = self.session()?;
        let source = self.source_label(strip).await?;
        let index = self.resolve_headamp(strip).await?;
        let (gain, phantom) = headamp_mapping(index)?;
        let value = match param {
            "gain" => self.read(&gain).await,
            "phantom" => self.read(&phantom).await,
            _ => anyhow::bail!("x32: unknown preamp parameter"),
        }?;
        ensure!(
            self.resolve_headamp(strip).await? == index
                && self.source_label(strip).await? == source,
            "x32: input source changed during preamp read"
        );
        self.ensure_current(&session)?;
        Ok(value)
    }

    /// Every strip consuming the same physical headamp is affected, even
    /// when the strips have different source labels or are not linked.
    pub async fn preamp_control_members(&self, strip: StripId) -> anyhow::Result<Vec<StripId>> {
        let index = self.resolve_headamp(strip).await?;
        let mut members = self.linked_members(strip).await?;
        for candidate in 0..40 {
            let member = StripId(candidate);
            let observed = self.read_int(&ha_index_addr(member)?).await?;
            ensure!(
                (-1..=127).contains(&observed),
                "x32: invalid routed headamp index"
            );
            if observed == index && !members.contains(&member) {
                members.push(member);
            }
        }
        ensure!(
            self.resolve_headamp(strip).await? == index,
            "x32: routed headamp changed while resolving affected strips"
        );
        members.sort_by_key(|member| member.0);
        Ok(members)
    }

    /// Guarded write: the caller's captured `source` must still match
    /// the live source, then the headamp index is resolved fresh and
    /// the write goes to that physical preamp only.
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
        ensure!(
            matches!(param, "gain" | "phantom"),
            "x32: unknown preamp parameter"
        );
        // Pin the value encoding first (range-checked, no I/O).
        let probe = match param {
            "gain" => Mapping {
                path: String::new(),
                kind: WireKind::Gain,
            },
            _ => Mapping {
                path: String::new(),
                kind: WireKind::Bool,
            },
        };
        let arg = probe.encode(value.clone())?;
        let captured_source = expected
            .get("source")
            .ok_or_else(|| anyhow::anyhow!("x32: preamp write requires captured source"))?;
        let Some(ParamValue::Text(context)) = expected.get("x32:preamp_state") else {
            anyhow::bail!("x32: preamp write requires captured physical headamp context");
        };
        let context: serde_json::Value = serde_json::from_str(context)?;
        let captured_headamp = context["headamp"]
            .as_i64()
            .filter(|index| (0..=127).contains(index))
            .ok_or_else(|| anyhow::anyhow!("x32: captured physical headamp is unavailable"))?
            as i32;
        ensure!(
            context["source"]
                .as_str()
                .map(|source| ParamValue::Text(source.into()))
                .as_ref()
                == Some(captured_source),
            "x32: captured preamp source does not match source context"
        );
        ensure!(
            expected
                .keys()
                .all(|id| matches!(id.as_str(), "source" | "x32:preamp_state")),
            "x32: unknown preamp context field"
        );
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        let index = self.resolve_headamp(strip).await?;
        ensure!(
            index == captured_headamp && self.source_label(strip).await? == *captured_source,
            "Input source or physical headamp changed; reopen the control."
        );
        let members = self.preamp_control_members(strip).await?;
        check(&members)?;
        ensure!(
            self.preamp_control_members(strip).await? == members,
            "x32: shared headamp membership changed; reopen the control"
        );
        ensure!(
            self.resolve_headamp(strip).await? == captured_headamp
                && self.source_label(strip).await? == *captured_source,
            "Input source or physical headamp changed; reopen the control."
        );
        self.ensure_current(&session)?;
        check(&members)?;
        let (gain, phantom) = headamp_mapping(index)?;
        let m = if param == "gain" { gain } else { phantom };
        session.send(&m.path, vec![arg]).await?;
        // Fresh independent readback of the same physical leaf.
        let actual = session.query_timeout(&m.path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let read = m.decode(&actual)?;
        let ok = match (&value, &read) {
            // 0.5 dB steps: tolerate one desk quantum.
            (ParamValue::Float(a), ParamValue::Float(b)) => (a - b).abs() <= 0.26,
            (ParamValue::Bool(a), ParamValue::Bool(b)) => a == b,
            _ => false,
        };
        ensure!(
            ok,
            "x32: headamp readback did not match the requested write"
        );
        Ok(())
    }
}
