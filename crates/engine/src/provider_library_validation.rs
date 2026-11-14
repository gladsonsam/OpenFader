//! Validation of persisted provider-native domains. Unknown providers retain
//! structural validation; a registered adapter can enforce its own schema.
use crate::engine::{StoredMonitor, StoredMuteGroups, StoredOutputEq, StoredOutputTaps, StoredRta};

pub(super) trait LibraryPolicy {
    fn output_eq(&self, _: &StoredOutputEq, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn rta(&self, _: &StoredRta, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn monitor(&self, _: &StoredMonitor, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn output_taps(&self, _: &StoredOutputTaps, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn mute_groups(&self, _: &StoredMuteGroups, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn accepts_extra_strip_key(&self, _: &str) -> bool {
        false
    }
}

struct GenericLibraryPolicy;
impl LibraryPolicy for GenericLibraryPolicy {}
struct XAirLibraryPolicy;

pub(super) fn policy_for_provider(provider: &str) -> &'static dyn LibraryPolicy {
    match provider {
        "xair" => &XAirLibraryPolicy,
        _ => &GenericLibraryPolicy,
    }
}

impl LibraryPolicy for XAirLibraryPolicy {
    fn output_eq(&self, eq: &StoredOutputEq, what: &str) -> Result<(), String> {
        {
            for row in &eq.targets {
                if !matches!(
                    row.target.as_str(),
                    "bus/01" | "bus/02" | "bus/03" | "bus/04" | "bus/05" | "bus/06" | "main"
                ) || row.mode > 2
                    || row.gains_db.len() != 31
                    || row.gains_db.iter().any(|g| {
                        !(-15.0..=15.0).contains(g) || (g * 2.0 - (g * 2.0).round()).abs() > 1e-4
                    })
                {
                    return Err(format!("{what}: invalid XR18 output EQ"));
                }
            }
            if eq.targets.len() != 7 {
                return Err(format!("{what}: incomplete XR18 output EQ roster"));
            }
        }
        Ok(())
    }

    fn rta(&self, rta: &StoredRta, what: &str) -> Result<(), String> {
        if rta.detector > 1 || !(0.0..=1.0).contains(&rta.decay) {
            return Err(format!("{what}: invalid RTA settings"));
        }
        Ok(())
    }

    fn monitor(&self, monitor: &StoredMonitor, what: &str) -> Result<(), String> {
        {
            for row in &monitor.values {
                let max = match row.id.as_str() {
                    "level" | "trim" | "dim_gain" => None,
                    "source" => Some(14.0),
                    "channel_mode" | "bus_mode" | "dim" | "mono" | "mute" | "dim_pfl" => Some(1.0),
                    _ => return Err(format!("{what}: unknown XR18 monitor ID")),
                };
                if match max {
                    Some(max) => row.value.fract() != 0.0 || !(0.0..=max).contains(&row.value),
                    None => !(0.0..=1.0).contains(&row.value),
                } {
                    return Err(format!("{what}: invalid XR18 monitor value"));
                }
            }
            if monitor.values.len() != 10 {
                return Err(format!("{what}: incomplete XR18 monitor roster"));
            }
        }
        Ok(())
    }

    fn output_taps(&self, taps: &StoredOutputTaps, what: &str) -> Result<(), String> {
        if taps.values.iter().any(|value| value.tap > 10) {
            return Err(format!("{what}: tap out of range"));
        }
        Ok(())
    }

    fn mute_groups(&self, groups: &StoredMuteGroups, what: &str) -> Result<(), String> {
        if groups.masters.len() != 4 || groups.memberships.len() != 31 {
            return Err(format!(
                "{what}: XR18 requires four masters and 31 strip memberships"
            ));
        }
        Ok(())
    }

    fn accepts_extra_strip_key(&self, key: &str) -> bool {
        key == "aux"
    }
}

/// Reject unknown provider payloads; provider validators own native rosters.
pub(super) fn native_processors(snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, what: &str) -> Result<(), String> {
    openfader_core::native_processor::validate_structure(snapshot)
        .map_err(|e| format!("{what}: {e}"))?;
    match snapshot.provider.as_str() {
        "wing" => openfader_proto_wing::validate_complete_native_processor_snapshot(snapshot)
            .map_err(|e| format!("{what}: {e:#}")),
        _ => Err(format!("{what}: no registered provider snapshot schema")),
    }
}
