//! FX specification, metadata, cache and guarded writes.
mod source;
use super::Dl32sDriver;
use openfader_core::destinations::{DestinationChoice, DestinationControl, DestinationRange};
use openfader_transport::mackie::{MackieSession, VALUE_TYPE_PARAM};
use std::sync::atomic::Ordering;

use specs::FX_PARAMETERS;
mod specs;

#[derive(Debug, Default)]
struct FxParameterCache {
    algorithm: Option<u32>,
    current: Option<openfader_core::destinations::DestinationValue>,
    revision: u64,
    source: Option<source::SourceState>,
}

#[derive(Debug, Default)]
pub(super) struct FxState {
    parameters: std::collections::BTreeMap<u32, FxParameterCache>,
}

impl FxState {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.parameters = FX_PARAMETERS
            .iter()
            .map(|spec| {
                let algorithm = session.cached_verified(spec.algorithm_address);
                (
                    spec.address,
                    FxParameterCache {
                        algorithm,
                        current: (algorithm == Some(spec.algorithm))
                            .then(|| {
                                session
                                    .cached_verified(spec.address)
                                    .and_then(|raw| spec.read_value(raw))
                            })
                            .flatten(),
                        revision: 0,
                        source: (spec.id == "send_source")
                            .then(|| source::SourceState::seed(spec.slot, session)),
                    },
                )
            })
            .collect();
        for spec in FX_PARAMETERS {
            let parameter = self.parameters.get_mut(&spec.address).expect("seeded");
            if let Some(source) = parameter.source.as_ref() {
                parameter.current = (parameter.algorithm == Some(spec.algorithm))
                    .then(|| source.current(spec.slot))
                    .flatten();
            }
        }
    }
    pub(super) fn ingest(&mut self, start: u32, raws: &[u32]) {
        let raw_at = |address: u32| {
            address
                .checked_sub(start)
                .and_then(|offset| raws.get(offset as usize))
                .copied()
        };
        for spec in FX_PARAMETERS {
            let Some(parameter) = self.parameters.get_mut(&spec.address) else {
                continue;
            };
            if let Some(algorithm) = raw_at(spec.algorithm_address) {
                if parameter.algorithm != Some(algorithm) {
                    parameter.algorithm = Some(algorithm);
                    parameter.current = None;
                    parameter.revision = parameter.revision.wrapping_add(1);
                }
            }
            if let Some(source) = parameter.source.as_mut() {
                source.ingest(spec.slot, start, raws);
                parameter.current = (parameter.algorithm == Some(spec.algorithm))
                    .then(|| source.current(spec.slot))
                    .flatten();
                continue;
            }
            if let Some(raw) = raw_at(spec.address) {
                parameter.revision = parameter.revision.wrapping_add(1);
                parameter.current = (parameter.algorithm == Some(spec.algorithm))
                    .then(|| spec.read_value(raw))
                    .flatten();
            }
        }
    }
    pub(super) fn controls(&self, controls: &mut Vec<DestinationControl>) {
        for spec in FX_PARAMETERS {
            let Some(parameter) = self
                .parameters
                .get(&spec.address)
                .filter(|parameter| parameter.algorithm == Some(spec.algorithm))
            else {
                continue;
            };
            controls.push(DestinationControl {
                revision: None,
                generation: None,
                preset: false,
                target: format!("fx/{:02}", spec.slot), target_label: format!("FX {}", spec.slot),
                target_name: Some(spec.algorithm_name.to_string()), send_slot: None,
                surface: "fx".to_string(), id: spec.id.to_string(), label: spec.label.to_string(),
                // Native Tape Echo tapping changes only this delay-time word.
                gesture: None,
                tap_tempo: spec.slot == 2 && spec.algorithm == 12 && spec.id == "time",
                description: if spec.id == "mute" {
                    format!("Changes the mute state of FX {}.", spec.slot)
                } else if spec.id == "send_mute" {
                    format!("Mutes the FX {} send master. Turning this off does not clear mute groups.", spec.slot)
                } else if spec.id == "send_source" {
                    format!("Changes the input send sources feeding FX {}. This can change the effect mix for multiple inputs.", spec.slot)
                } else if spec.id == "send_master" {
                    format!("Changes the master send level feeding FX {}. Return levels and mute are separate controls.", spec.slot)
                } else if spec.id == "main_return" {
                    format!("Changes the FX {} return level feeding Main. Other destinations and mute are separate controls.", spec.slot)
                } else {
                    format!("Changes {} for every route through FX {}. Mute, returns and routing are separate controls.", spec.label.to_lowercase(), spec.slot)
                },
                choices: if spec.id == "send_source" { vec![
                    DestinationChoice { value: 0.0, label: "Post Fader".to_string() },
                    DestinationChoice { value: 1.0, label: "Pre Fader".to_string() },
                ] } else if spec.id == "send_mute" { vec![
                    DestinationChoice { value: 0.0, label: "Off".to_string() },
                    DestinationChoice { value: 1.0, label: "On".to_string() },
                ] } else if spec.id == "mute" { vec![
                    DestinationChoice { value: 0.0, label: "Unmuted".to_string() },
                    DestinationChoice { value: 1.0, label: "Muted".to_string() },
                ] } else { Vec::new() },
                range: (!spec.is_binary()).then(|| DestinationRange { min: spec.min, max: spec.max, unit: spec.unit.to_string(), decimals: spec.decimals }),
                current: parameter.current.clone(),
            });
        }
    }
}

impl Dl32sDriver {
    pub(super) async fn write_fx_control(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
    ) -> anyhow::Result<bool> {
        if let Some(spec) = FX_PARAMETERS
            .iter()
            .find(|spec| target == format!("fx/{:02}", spec.slot) && control == spec.id)
        {
            anyhow::ensure!(
                spec.allows(value) && spec.allows(expected),
                "dl32s: FX parameter outside documented range"
            );
            let guard = self.session().await?;
            let session = guard.as_ref().expect("checked above");
            anyhow::ensure!(session.is_alive(), "dl32s: FX session unavailable");
            let generation = self.generation.load(Ordering::Acquire);
            let revision = {
                let mut native = self
                    .native
                    .lock()
                    .map_err(|_| anyhow::anyhow!("FX cache unavailable"))?;
                anyhow::ensure!(
                    native.generation == generation && native.view.connected,
                    "FX connection changed."
                );
                let parameter = native
                    .fx
                    .parameters
                    .get_mut(&spec.address)
                    .ok_or_else(|| anyhow::anyhow!("FX parameter unavailable."))?;
                anyhow::ensure!(
                    parameter.algorithm == Some(spec.algorithm)
                        && session.cached_verified(spec.algorithm_address) == Some(spec.algorithm),
                    "FX algorithm changed; reopen the control."
                );
                anyhow::ensure!(
                    parameter
                        .current
                        .as_ref()
                        .is_some_and(|current| current.value == expected),
                    "FX value changed; reopen the control."
                );
                if let Some(raw) = session.cached_verified(spec.address) {
                    anyhow::ensure!(
                        raw == spec.raw_value(expected),
                        "FX value changed; reopen the control."
                    );
                }
                let revision = if let Some(source) = parameter.source.as_mut() {
                    source.begin(spec.slot, expected as u32, session)?
                } else {
                    parameter.revision
                };
                parameter.current = None;
                revision
            };
            let result = if spec.id == "send_source" {
                let pairs: Vec<_> = source::addresses(spec.slot)
                    .into_iter()
                    .map(|address| (address, value as u32))
                    .collect();
                session.write_multi(&pairs, VALUE_TYPE_PARAM).await
            } else {
                session
                    .write_int(spec.address, spec.raw_value(value), VALUE_TYPE_PARAM)
                    .await
            };
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("FX cache unavailable"))?;
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation
                    && native.generation == generation
                    && native.view.connected,
                "FX connection changed while writing; result uncertain."
            );
            let parameter = native
                .fx
                .parameters
                .get_mut(&spec.address)
                .ok_or_else(|| anyhow::anyhow!("FX parameter unavailable."))?;
            anyhow::ensure!(
                parameter.algorithm == Some(spec.algorithm),
                "FX algorithm changed while writing; result uncertain."
            );
            if let Some(source) = parameter.source.as_mut() {
                source.finish(spec.slot, revision, value as u32, result.is_ok());
                parameter.current = source.current(spec.slot);
            } else if result.is_ok() && parameter.revision == revision {
                parameter.current = Some(openfader_core::destinations::DestinationValue {
                    value,
                    verified: false,
                });
            }
            result?;
            return Ok(true);
        }
        Ok(false)
    }
}
