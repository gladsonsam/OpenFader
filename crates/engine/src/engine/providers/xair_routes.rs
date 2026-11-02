//! XR18 snapshot route validation and conversion.
use super::super::*;

/// Validate the complete XR18 input-selector set before capture/recall. Older
/// scenes with no input-route rows retain their legacy behavior.
pub(crate) fn scene_input_routes(
    routing: &[StoredRoute],
    strips: &[StoredStrip],
    topo: &[openfader_core::StripFamily],
) -> anyhow::Result<Option<std::collections::BTreeMap<u16, Vec<crate::drivers::SceneRouteChange>>>>
{
    use openfader_proto_xair::input_routing::InputRoute;
    let mut parts: std::collections::BTreeMap<u16, [Option<i32>; 3]> =
        std::collections::BTreeMap::new();
    for row in routing
        .iter()
        .filter(|r| matches!(r.target.as_str(), "analog" | "usb" | "bank"))
    {
        let strip = row
            .key
            .as_deref()
            .and_then(|k| openfader_core::strip_index_for_key(topo, k))
            .map(|i| i as u16)
            .unwrap_or(row.strip);
        anyhow::ensure!(
            strip < 16,
            "snapshot input route points outside XR18 channels"
        );
        let (field, value) = match (row.target.as_str(), row.group.as_str(), row.index) {
            ("analog", "", 0) => (0, 18),
            ("analog", "Mic", n @ 1..=16) => (0, i32::from(n) - 1),
            ("analog", "Aux", n @ 1..=2) => (0, i32::from(n) + 15),
            ("usb", "USB", n @ 1..=18) => (1, i32::from(n) - 1),
            ("bank", "Bank", n @ 1..=2) => (2, i32::from(n) - 1),
            _ => anyhow::bail!("snapshot contains an invalid XR18 input route"),
        };
        let entry = parts.entry(strip).or_insert([None; 3]);
        anyhow::ensure!(
            entry[field].replace(value).is_none(),
            "snapshot repeats an XR18 input route"
        );
    }
    if parts.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(
        parts.len() == 16,
        "snapshot XR18 input routes are incomplete"
    );
    let mut routes = std::collections::BTreeMap::new();
    for (strip, p) in parts {
        let route = InputRoute {
            analog: p[0].ok_or_else(|| anyhow::anyhow!("snapshot analog route missing"))?,
            usb: p[1].ok_or_else(|| anyhow::anyhow!("snapshot USB route missing"))?,
            bank: p[2].ok_or_else(|| anyhow::anyhow!("snapshot input bank missing"))?,
        };
        let saved = strips
            .iter()
            .find(|s| {
                s.key
                    .as_deref()
                    .and_then(|k| openfader_core::strip_index_for_key(topo, k))
                    .map(|i| i as u16)
                    .unwrap_or(s.strip)
                    == strip
            })
            .and_then(|s| s.params.get("source"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("snapshot input source context missing"))?;
        anyhow::ensure!(
            saved == route.active_label(),
            "snapshot input route and preamp source disagree on channel {}",
            strip + 1
        );
        let analog_source = match route.analog {
            0..=15 => Some(("Mic", (route.analog + 1) as u8)),
            16..=17 => Some(("Aux", (route.analog - 15) as u8)),
            18 => None,
            _ => unreachable!(),
        };
        routes.insert(
            strip,
            vec![
                crate::drivers::SceneRouteChange {
                    target: "analog",
                    desired: route.analog,
                    max: 18,
                    source: analog_source,
                },
                crate::drivers::SceneRouteChange {
                    target: "usb",
                    desired: route.usb,
                    max: 17,
                    source: Some(("USB", (route.usb + 1) as u8)),
                },
                crate::drivers::SceneRouteChange {
                    target: "bank",
                    desired: route.bank,
                    max: 1,
                    source: Some(("Bank", (route.bank + 1) as u8)),
                },
            ],
        );
    }
    Ok(Some(routes))
}

/// Compatibility selector mapping used by the XR18 adapter and its simulator fixture.
pub(crate) fn selector_change(
    expected: crate::engine::selector_session::InputRouteSelectors,
    desired: crate::engine::selector_session::InputRouteSelectors,
) -> anyhow::Result<(&'static str, Option<(&'static str, u8)>)> {
    if expected.analog != desired.analog {
        Ok(match desired.analog {
            0..=15 => ("analog", Some(("Mic", (desired.analog + 1) as u8))),
            16..=17 => ("analog", Some(("Aux", (desired.analog - 15) as u8))),
            18 => ("analog", None),
            _ => anyhow::bail!("invalid analog selector"),
        })
    } else if expected.usb != desired.usb {
        anyhow::ensure!((0..=17).contains(&desired.usb), "invalid usb selector");
        Ok(("usb", Some(("USB", (desired.usb + 1) as u8))))
    } else {
        anyhow::ensure!((0..=1).contains(&desired.bank), "invalid bank selector");
        Ok(("bank", Some(("Bank", (desired.bank + 1) as u8))))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InputRouteSelectors {
    pub analog: i32,
    pub usb: i32,
    pub bank: i32,
}

impl crate::engine::selector_session::SelectorSession {
    pub async fn read_input_route(&self, channel: u16) -> anyhow::Result<InputRouteSelectors> {
        self.check()?;
        let result = self.driver.input_route_guard(StripId(channel)).await;
        self.check()?;
        let value = result?.ok_or_else(|| anyhow::anyhow!("input route unsupported"))?;
        Ok(serde_json::from_value(value)?)
    }

    /// Read a pair and its link state using the pinned driver and routing guard.
    pub async fn read_link_pair(
        &self,
        left: u16,
    ) -> anyhow::Result<([InputRouteSelectors; 2], bool)> {
        anyhow::ensure!(self.driver.selector_pair_valid(left), "invalid input pair");
        self.check()?;
        let link = self
            .driver
            .input_pair_link(StripId(left))
            .await?
            .ok_or_else(|| anyhow::anyhow!("input link unsupported"))?;
        let routes = [
            self.read_input_route(left).await?,
            self.read_input_route(left + 1).await?,
        ];
        self.check()?;
        Ok((routes, link))
    }

    /// Switch a linked pair only if both captured routes and link still match.
    pub async fn guarded_pair_bank(
        &self,
        left: u16,
        expected: [InputRouteSelectors; 2],
        bank: i32,
    ) -> anyhow::Result<()> {
        self.check()?;
        let check = |_: &[StripId]| self.check();
        self.driver
            .linked_input_bank_guarded(
                StripId(left),
                [
                    serde_json::to_value(expected[0])?,
                    serde_json::to_value(expected[1])?,
                ],
                bank,
                &check,
            )
            .await?;
        self.check()
    }

    pub async fn guarded_single_selector_write(
        &self,
        channel: u16,
        expected: InputRouteSelectors,
        desired: InputRouteSelectors,
    ) -> anyhow::Result<()> {
        self.check()?;
        anyhow::ensure!(
            self.read_input_route(channel).await? == expected,
            "input route changed"
        );
        let differences = [
            expected.analog != desired.analog,
            expected.usb != desired.usb,
            expected.bank != desired.bank,
        ];
        anyhow::ensure!(
            differences.iter().filter(|v| **v).count() == 1,
            "exactly one selector must change"
        );
        let (target, source) = self.driver.selector_change(expected, desired)?;
        let guard = serde_json::to_value(expected)?;
        let check = |_: &[StripId]| self.check();
        let result = self
            .driver
            .patch_target_guarded(StripId(channel), target, source, Some(&guard), &check)
            .await;
        self.check()?;
        result?;
        anyhow::ensure!(
            self.read_input_route(channel).await? == desired,
            "selector readback mismatch"
        );
        Ok(())
    }
}
