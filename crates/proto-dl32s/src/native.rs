//! Generation-tagged desk-originated snapshots and destination metadata.
use crate::assignments::{assignment_name_label, assignment_name_parameter};
use crate::{destination, fx, Dl32sDriver};
use openfader_transport::mackie::{
    MackieSession, PushEvent, VALUE_TYPE_MUTE_GROUP, VALUE_TYPE_PARAM,
};
use std::sync::atomic::Ordering;

/// Private generation-tagged wrapper around the public
/// [`openfader_core::NativeSnapshots`] view: `generation` names the
/// connection that owns `view` (same pattern as [`crate::telemetry::MeterState`] /
/// [`crate::telemetry::SpectrumState`]). The ingest task compares inside the lock so a
/// stale task can never write into a newer session's view, and the
/// getter withholds a mismatched generation. The public core model stays
/// untagged.
#[derive(Debug, Default)]
pub(super) struct NativeCache {
    pub(super) generation: u64,
    pub(super) view: openfader_core::NativeSnapshots,
    pub(super) parameter_labels: std::collections::BTreeMap<&'static str, String>,
    pub(super) destinations: destination::DestinationState,
    pub(super) fx: fx::FxState,
    pub(super) globals: crate::global_controls::GlobalControls,
    pub(super) output_routes: crate::output_routing::OutputRoutes,
    pub(super) output_processing: crate::output_processing::OutputProcessingCache,
}

impl Dl32sDriver {
    pub(super) fn native_param_label_overrides(&self) -> Vec<(&'static str, String)> {
        let generation = self.generation.load(Ordering::Acquire);
        self.native
            .lock()
            .ok()
            .filter(|n| n.generation == generation && n.view.connected)
            .map(|n| {
                n.parameter_labels
                    .iter()
                    .map(|(id, name)| (*id, name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Read-only native snapshot view: the push-observed model above
    /// (`Some` whenever this driver is selected — supported but possibly
    /// unconnected/loading — so the engine can render honest status
    /// instead of guessing). A mismatched private generation withholds
    /// the view (unavailable default) so a newer session never reads the
    /// older session's slots. No write path exists behind this hook.
    pub(super) fn read_native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        let current = self.generation.load(Ordering::Acquire);
        self.native.lock().ok().map(|v| {
            if v.generation != current {
                return openfader_core::NativeSnapshots::default();
            }
            v.view.clone()
        })
    }
}

impl NativeCache {
    pub(super) fn seed(&mut self, generation: u64, session: &MackieSession) {
        let mut slots: Vec<openfader_core::NativeSnapshotSlot> = session
            .snapshot_names_pushed()
            .into_iter()
            .map(|(slot, name)| openfader_core::NativeSnapshotSlot { slot, name })
            .collect();
        slots.sort_by_key(|s| s.slot);
        self.generation = generation;
        self.destinations.seed(session);
        self.fx.seed(session);
        self.globals.seed(session);
        self.output_processing.seed(session);
        self.output_routes.seed(session);
        self.parameter_labels = (58..=75)
            .filter_map(|slot| {
                let id = assignment_name_parameter(slot)?;
                let name = session.name_slot_verified(slot)?;
                Some((id, assignment_name_label(slot, &name)?))
            })
            .collect();
        self.view.slots = slots;
        self.view.current = session.current_snapshot_pushed();
        self.view.received = !self.view.slots.is_empty() || self.view.current.is_some();
        self.view.connected = true;
    }
}

/// Ingest desk-originated metadata; false retires an obsolete connection task.
pub(super) fn ingest_native(
    native: &std::sync::Mutex<NativeCache>,
    generation: u64,
    push: &PushEvent,
) -> bool {
    match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if matches!(*value_type, VALUE_TYPE_PARAM | VALUE_TYPE_MUTE_GROUP) => {
            if let Ok(mut v) = native.lock() {
                if v.generation != generation {
                    return false;
                }
                if v.view.connected {
                    if *value_type == VALUE_TYPE_PARAM {
                        v.destinations.ingest(*start, raws);
                        v.output_routes.ingest(*start, raws);
                        v.fx.ingest(*start, raws);
                    }
                    if *value_type == VALUE_TYPE_MUTE_GROUP { v.globals.ingest(*start, raws); }
                    v.output_processing.ingest(*value_type, *start, raws);
                }
            }
        }
        openfader_transport::mackie::PushEvent::Names { pairs } => {
            if let Ok(mut v) = native.lock() {
                if v.generation != generation {
                    return false;
                }
                if v.view.connected {
                    for (slot, name) in pairs {
                        v.destinations.name(*slot, name);
                        v.output_processing.rename(*slot, name);
                        if let Some(id) = assignment_name_parameter(*slot) {
                            if let Some(label) = assignment_name_label(*slot, name) {
                                v.parameter_labels.insert(id, label);
                            } else {
                                v.parameter_labels.remove(id);
                            }
                        }
                    }
                }
            }
        }
        openfader_transport::mackie::PushEvent::SnapshotRecall(n) => {
            if let Ok(mut v) = native.lock() {
                if v.generation != generation {
                    return false;
                }
                if v.view.connected {
                    v.view.current = Some(*n);
                    v.view.received = true;
                }
            }
        }
        openfader_transport::mackie::PushEvent::SnapshotRename { slot, name } => {
            if let Ok(mut v) = native.lock() {
                if v.generation != generation {
                    return false;
                }
                if v.view.connected {
                    match v.view.slots.iter_mut().find(|s| s.slot == *slot) {
                        Some(row) => row.name = name.clone(),
                        None => v.view.slots.push(openfader_core::NativeSnapshotSlot {
                            slot: *slot,
                            name: name.clone(),
                        }),
                    }
                    v.view.slots.sort_by_key(|s| s.slot);
                    v.view.received = true;
                }
            }
        }
        _ => {}
    }
    true
}
