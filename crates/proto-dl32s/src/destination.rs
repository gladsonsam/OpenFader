//! Aux destination metadata, push cache and guarded tap writes.
use super::Dl32sDriver;
use openfader_core::destinations::{DestinationChoice, DestinationControl};
use openfader_transport::mackie::{MackieSession, VALUE_TYPE_PARAM};
use std::sync::atomic::Ordering;

// Each Aux1..8 address was independently changed in Master Fader from
// Pre Fader1 -> Pre DSP2 and restored against the complete bank. Other
// tap values and the separate Use LR Mute flag remain unverified.
const AUX_SEND_SOURCE_ADDRESSES: [u32; 8] = [4434, 4524, 4614, 4704, 4794, 4884, 4974, 5064];
fn aux_tap_value(raw: u32) -> Option<openfader_core::destinations::DestinationValue> {
    matches!(raw, 1 | 2).then_some(openfader_core::destinations::DestinationValue {
        value: raw as f32,
        verified: true,
    })
}

#[derive(Debug, Default)]
struct AuxTapCache {
    name: Option<String>,
    current: Option<openfader_core::destinations::DestinationValue>,
    revision: u64,
}

#[derive(Debug, Default)]
pub(super) struct DestinationState {
    aux_taps: std::collections::BTreeMap<u16, AuxTapCache>,
}

impl DestinationState {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.aux_taps = AUX_SEND_SOURCE_ADDRESSES
            .iter()
            .enumerate()
            .map(|(index, address)| {
                let slot = index as u16 + 1;
                (
                    slot,
                    AuxTapCache {
                        name: session
                            .name_slot_verified(49 + slot as u32)
                            .filter(|name| !name.is_empty()),
                        current: session.cached_verified(*address).and_then(aux_tap_value),
                        revision: 0,
                    },
                )
            })
            .collect();
    }
    pub(super) fn ingest(&mut self, start: u32, raws: &[u32]) {
        for (index, address) in AUX_SEND_SOURCE_ADDRESSES.iter().enumerate() {
            if *address < start || (*address - start) >= raws.len() as u32 {
                continue;
            }
            if let Some(tap) = self.aux_taps.get_mut(&(index as u16 + 1)) {
                tap.revision = tap.revision.wrapping_add(1);
                tap.current = aux_tap_value(raws[(*address - start) as usize]);
            }
        }
    }
    pub(super) fn name(&mut self, slot: u32, name: &str) {
        if (50..=57).contains(&slot) {
            if let Some(tap) = self.aux_taps.get_mut(&((slot - 49) as u16)) {
                tap.name = (!name.is_empty()).then(|| name.to_string());
            }
        }
    }
}

impl Dl32sDriver {
    pub(super) fn collect_destination_controls(&self) -> Vec<DestinationControl> {
        let generation = self.generation.load(Ordering::Acquire);
        let native = self
            .native
            .lock()
            .ok()
            .filter(|n| n.generation == generation && n.view.connected);
        let mut controls: Vec<_> = (1..=AUX_SEND_SOURCE_ADDRESSES.len() as u16).map(|slot| {
            let tap = native.as_ref().and_then(|n| n.destinations.aux_taps.get(&slot));
            DestinationControl {
                members: Vec::new(),
                revision: None,
                generation: None,
                preset: false,
                target: format!("bus/{slot:02}"), target_label: format!("Aux {slot}"),
                target_name: tap.and_then(|tap| tap.name.clone()), send_slot: Some(slot), id: "tap".to_string(),
                surface: "send".to_string(),
                description: format!("Applies to every input feeding Aux {slot}. Changing this can alter that monitor mix."),
                label: "Send source".to_string(),
                gesture: None,
                tap_tempo: false,
                choices: vec![
                    DestinationChoice { value: 2.0, label: "Pre DSP".to_string() },
                    DestinationChoice { value: 1.0, label: "Pre Fader".to_string() },
                ], range: None, current: tap.and_then(|tap| tap.current.clone()),
            }
        }).collect();
        if let Some(native) = native.as_ref() {
            native.fx.controls(&mut controls, generation);
        }
        controls
    }
    pub(super) async fn write_aux_control(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
    ) -> anyhow::Result<()> {
        let slot = (1..=AUX_SEND_SOURCE_ADDRESSES.len() as u16)
            .find(|slot| target == format!("bus/{slot:02}"))
            .ok_or_else(|| anyhow::anyhow!("dl32s: unverified destination control"))?;
        anyhow::ensure!(control == "tap", "dl32s: unverified destination control");
        let address = AUX_SEND_SOURCE_ADDRESSES[slot as usize - 1];
        anyhow::ensure!(
            matches!(value, 1.0 | 2.0) && matches!(expected, 1.0 | 2.0),
            "dl32s: unverified destination tap value"
        );
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        anyhow::ensure!(session.is_alive(), "dl32s: destination session unavailable");
        let generation = self.generation.load(Ordering::Acquire);
        // Display continuity permits an exact restore after an accepted own
        // write. This comparison is a guard, never claimed as verification.
        let revision = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("destination cache unavailable"))?;
            anyhow::ensure!(
                native.generation == generation && native.view.connected,
                "Destination connection changed."
            );
            let tap = native
                .destinations
                .aux_taps
                .get_mut(&slot)
                .ok_or_else(|| anyhow::anyhow!("Destination unavailable."))?;
            anyhow::ensure!(
                tap.current
                    .as_ref()
                    .is_some_and(|current| current.value == expected),
                "Destination changed; reopen the control."
            );
            if let Some(raw) = session.cached_verified(address) {
                anyhow::ensure!(
                    raw == expected as u32,
                    "Destination changed; reopen the control."
                );
            }
            tap.current = None; // failed/uncertain writes cannot retain old proof
            tap.revision
        };
        session
            .write_int(address, value as u32, VALUE_TYPE_PARAM)
            .await?;
        let mut native = self
            .native
            .lock()
            .map_err(|_| anyhow::anyhow!("destination cache unavailable"))?;
        anyhow::ensure!(
            self.generation.load(Ordering::Acquire) == generation
                && native.generation == generation
                && native.view.connected,
            "Destination connection changed while writing; result uncertain."
        );
        // A genuine newer push wins over accepted intent, even if it disagrees.
        let tap = native
            .destinations
            .aux_taps
            .get_mut(&slot)
            .ok_or_else(|| anyhow::anyhow!("Destination unavailable."))?;
        if tap.revision == revision {
            tap.current = Some(openfader_core::destinations::DestinationValue {
                value,
                verified: false,
            });
        }
        Ok(())
    }
}
