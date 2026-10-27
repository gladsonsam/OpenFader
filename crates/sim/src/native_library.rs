//! Memory-only exercise library, deliberately distinct from local scene storage.
use super::*;
use openfader_core::native_library::{receipt, validate_name, Check};
use serde_json::{json, Value};

#[derive(Default)]
pub(super) struct SimLibrary {
    revision: u64,
    current: Option<u32>,
    slots: std::collections::BTreeMap<u32, (String, MixerSnapshot)>,
}

impl SimDriver {
    fn library_view(&self, lib: &SimLibrary) -> Value {
        json!({"authority":"simulation", "label":"Simulated strip library", "connected":*self.connected.read().unwrap(),
            "generation":self.processor_generation.load(Ordering::Acquire), "revision":lib.revision,
            "capacity":99,"default_slot":null,"complete":true,"source":"simulation",
            "operations":["inspect","capture","replace","rename","recall","delete"],
            "slots":lib.slots.iter().map(|(slot,(name,_))|json!({"slot":slot,"name":name})).collect::<Vec<_>>(),
            "current":lib.current,
            "limitations":"Memory-only simulation of strip fader/mute/pan/name state. Slot IDs remain stable on delete; this is not native DL32S compaction. No native files, factory presets, safes or full DL32S show semantics. Capture does not write the OpenFader local library."})
    }
    pub(super) fn library_inventory(&self) -> Option<Value> {
        Some(self.library_view(&self.native_library.read().unwrap()))
    }
    pub(super) async fn execute_library(
        &self,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let mut lib = self.native_library.write().unwrap();
        anyhow::ensure!(
            check() && self.library_view(&lib) == *expected,
            "Simulated library context changed"
        );
        match action["op"].as_str() {
            Some("capture") => {
                let name = action["name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing capture name"))?;
                validate_name(name)?;
                let slot = (1..=99)
                    .find(|n| !lib.slots.contains_key(n))
                    .ok_or_else(|| anyhow::anyhow!("Simulated library full"))?;
                let snapshot = MixerSnapshot {
                    strips: self.read().clone(),
                };
                anyhow::ensure!(check(), "Simulation access changed");
                lib.slots.insert(slot, (name.into(), snapshot));
            }
            Some("rename") | Some("replace") => {
                let slot = action["slot"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid simulated slot"))?;
                let (name, snapshot) = lib
                    .slots
                    .get_mut(&slot)
                    .ok_or_else(|| anyhow::anyhow!("Unknown simulated slot"))?;
                if action["op"] == "rename" {
                    let value = action["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
                    validate_name(value)?;
                    anyhow::ensure!(check(), "Simulation access changed");
                    *name = value.to_owned();
                } else {
                    let replacement = MixerSnapshot {
                        strips: self.read().clone(),
                    };
                    anyhow::ensure!(check(), "Simulation access changed");
                    *snapshot = replacement;
                }
            }
            Some("recall") | Some("delete") => {
                let slot = action["slot"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid slot"))?;
                let snapshot = lib
                    .slots
                    .get(&slot)
                    .ok_or_else(|| anyhow::anyhow!("Unknown simulated slot"))?
                    .1
                    .clone();
                anyhow::ensure!(check(), "Simulation access changed");
                if action["op"] == "recall" {
                    *self.write() = snapshot.strips;
                    lib.current = Some(slot);
                } else {
                    lib.slots.remove(&slot);
                    if lib.current == Some(slot) {
                        lib.current = None;
                    }
                }
            }
            _ => anyhow::bail!("Simulation operation unsupported"),
        }
        lib.revision += 1;
        Ok(receipt(
            "confirmed",
            "Simulated strip library operation completed in memory.",
            self.library_view(&lib),
        ))
    }
}
