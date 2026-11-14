//! Native observations whose write contract is not sufficiently established.
//! Factual paths/types: pmaillot/X32-Behringer X32CfgMain.h and X32PrefStat.h;
//! OSC reference 4.09-02 pp.21,54–55. Do not infer write domains from Sidecar.
use crate::{X32Driver, QUERY_TIMEOUT};
use openfader_core::console_controls::{ConsoleControl, ConsoleControlKind};
use openfader_transport::OscSession;
use rosc::OscType;
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, Arc};

enum Wire {
    Bitmap,
    Float,
    Text,
}

struct Observation {
    branch: &'static str,
    id: &'static str,
    label: &'static str,
    native: &'static str,
    wire: Wire,
    reason: &'static str,
}

const OBSERVATIONS: &[Observation] = &[
    Observation { branch: "talk", id: "talk.a_destinations", label: "Talkback A destination bitmap", native: "/config/talk/A/destmap", wire: Wire::Bitmap,
        reason: "Read-only native bitmap: destination bit ordering has not been independently verified." },
    Observation { branch: "talk", id: "talk.b_destinations", label: "Talkback B destination bitmap", native: "/config/talk/B/destmap", wire: Wire::Bitmap,
        reason: "Read-only native bitmap: destination bit ordering has not been independently verified." },
    Observation { branch: "settings", id: "settings.lamp_native", label: "Lamp dimmer (native value)", native: "/-prefs/lamp", wire: Wire::Float,
        reason: "Read-only native value: the lamp's wire scaling has not been independently verified." },
    Observation { branch: "settings", id: "settings.profile_name", label: "Preference profile name", native: "/-prefs/style", wire: Wire::Text,
        reason: "Read-only: the native text write limits have not been established." },
    Observation { branch: "settings", id: "settings.device_name", label: "Device name", native: "/-prefs/name", wire: Wire::Text,
        reason: "Read-only: the native name write limits and identification side effects have not been established." },
];

impl Observation {
    fn decode(&self, args: &[OscType]) -> anyhow::Result<Value> {
        match (&self.wire, args) {
            (Wire::Bitmap, [OscType::Int(n)]) if (0..=262143).contains(n) => Ok(json!(n)),
            (Wire::Float, [OscType::Float(n)]) if n.is_finite() => Ok(json!(n)),
            // This is an observation-size safety bound, not a native write limit.
            (Wire::Text, [OscType::String(s)]) if s.len() <= 4096 && !s.contains('\0') => {
                Ok(json!(s))
            }
            _ => anyhow::bail!("Unexpected native observation type or value"),
        }
    }
}

impl X32Driver {
    pub(super) async fn append_console_observations(
        &self,
        session: &Arc<OscSession>,
        branch: &str,
        items: &mut Vec<ConsoleControl>,
    ) -> anyhow::Result<()> {
        for row in OBSERVATIONS.iter().filter(|row| row.branch == branch) {
            self.ensure_current(session)?;
            let epoch = self.console_epoch.load(Ordering::Acquire);
            let observed = session
                .query_timeout(row.native, QUERY_TIMEOUT)
                .await
                .and_then(|reply| {
                    anyhow::ensure!(
                        reply.addr == row.native,
                        "Native observation address mismatch"
                    );
                    row.decode(&reply.args)
                });
            self.ensure_current(session)?;
            let observed = if self.console_epoch.load(Ordering::Acquire) == epoch {
                observed
            } else {
                Err(anyhow::anyhow!(
                    "Console controls changed during observation; refresh"
                ))
            };
            let (value, reason) = match observed {
                Ok(value) => (Some(value), row.reason.into()),
                Err(error) => (None, format!("{} Not observed: {error}", row.reason)),
            };
            items.push(ConsoleControl {
                path: format!("/{}", row.id),
                label: row.label.into(),
                kind: match row.wire {
                    Wire::Bitmap => ConsoleControlKind::Integer,
                    Wire::Float => ConsoleControlKind::Number,
                    Wire::Text => ConsoleControlKind::Text,
                },
                value,
                read_only: true,
                unavailable_reason: Some(reason),
                // No revision or writable registry entry is minted for observations.
                ..Default::default()
            });
        }
        Ok(())
    }
}
