//! X32/M32 native show and library operations, separate from local scenes.
//! OSC exposes dataset operations, not arbitrary desk-USB file access.
//! Unconfirmed sends never become readback; stale sessions cannot validate writes.

#[path = "native_tables.rs"]
mod tables;

mod archive;
mod inventory;
mod showdump;

pub use archive::validate_archive;

use openfader_core::native_library::Check;
use openfader_transport::OscSession;
use rosc::OscType;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crate::{X32Driver, QUERY_TIMEOUT};

/// Desk slot ranges (protocol reference v4.09-02; cue/snippet/scene tables in
/// show table, library tables in library table).
pub const SCENE_SLOTS: usize = 100;
pub const SNIPPET_SLOTS: usize = 100;
/// Cue inventory positions `cue/000..499` (show table tables).
pub const CUE_POSITIONS: usize = 500;
/// Library slots per bank, wire leaves `001..100`, dataset index 0..99.
pub const LIB_SLOTS: usize = 100;
/// Local archive schema tag. Desk USB images are NOT this format.
pub const ARCHIVE_SCHEMA: &str = "openfader-x32-native/1";

/// Bounded concurrency for inventory fan-out. The protocol warns that UDP
/// bursts over slow WiFi lose packets silently; keep this modest.
const FANOUT: usize = 8;
/// Per-leaf timeout inside bulk reads.
const LEAF_TIMEOUT: Duration = Duration::from_millis(700);
/// `/showdump` collection window. The emulator answers one line; a real
/// desk bursts the whole dump here.
const SHOWDUMP_WINDOW: Duration = Duration::from_millis(1200);

// ---------------------------------------------------------------------------
// state
// ---------------------------------------------------------------------------

/// One desk scene slot (wire `/-show/showfile/scene/NNN/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeSceneSlot {
    pub slot: u8,
    pub name: String,
    pub notes: String,
    pub safes: u32,
    pub hasdata: bool,
}

/// One desk snippet slot (wire `/-show/showfile/snippet/NNN/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeSnippetSlot {
    pub slot: u8,
    pub name: String,
    pub eventtyp: u32,
    pub channels: u32,
    pub auxbuses: u32,
    pub maingrps: u32,
    pub hasdata: bool,
}

/// One cue inventory position (wire `/-show/showfile/cue/NNN/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeCueSlot {
    pub position: u16,
    pub numb: i32,
    pub name: String,
    pub skip: bool,
    pub scene: i32,
    pub snippet: i32,
    pub miditype: i32,
    pub midichan: i32,
    pub midipara1: i32,
    pub midipara2: i32,
}

/// One library slot (wire `/-libs/{ch,fx,r,mon}/NNN/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeLibSlot {
    pub slot: u8,
    pub name: String,
    pub typ: i32,
    pub flags: u32,
    pub hasdata: bool,
}

/// Session-owned cache. `owner` is a `Weak<OscSession>` upgrade pin: a sync
/// view taken under a different (reconnected) session reports disconnected
/// rather than laundering stale slots. `Weak` upgrade (not a raw pointer
/// address) pins identity, so a freed session can never match a recycled
/// address. `generation` mirrors `revision` on every cache transition so the
/// generic Flutter guard (`generation` + `revision` + `connected`) stays
/// well-formed; `revision` remains the invalidation counter.
#[derive(Clone, Debug, Default)]
pub struct NativeState {
    owner: Option<Weak<OscSession>>,
    pub revision: u64,
    pub generation: u64,
    pub connected: bool,
    pub complete: bool,
    pub error: Option<String>,
    pub show_name: Option<String>,
    pub scenes: Vec<NativeSceneSlot>,
    pub snippets: Vec<NativeSnippetSlot>,
    pub cues: Vec<NativeCueSlot>,
    pub libs_supported: Option<bool>,
    pub lib_ch: Vec<NativeLibSlot>,
    pub lib_fx: Vec<NativeLibSlot>,
    pub lib_r: Vec<NativeLibSlot>,
    pub lib_mon: Vec<NativeLibSlot>,
}

impl NativeState {
    fn is_owner(&self, session: &Arc<OscSession>) -> bool {
        // `Weak` upgrade binds identity to the allocation, not the address:
        // a dropped session upgrades to `None` even if a new `Arc` later
        // reuses the same address (no pointer-`usize` ABA).
        self.owner
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|owned| Arc::ptr_eq(&owned, session))
            && self.connected
    }

    fn slot_json_scene(s: &NativeSceneSlot) -> Value {
        json!({"slot": s.slot, "name": s.name, "notes": s.notes,
               "safes": s.safes, "hasdata": s.hasdata})
    }

    fn slot_json_snippet(s: &NativeSnippetSlot) -> Value {
        json!({"slot": s.slot, "name": s.name, "eventtyp": s.eventtyp,
               "channels": s.channels, "auxbuses": s.auxbuses,
               "maingrps": s.maingrps, "hasdata": s.hasdata})
    }

    fn slot_json_cue(c: &NativeCueSlot) -> Value {
        json!({"position": c.position, "numb": c.numb, "name": c.name,
               "skip": c.skip, "scene": c.scene, "snippet": c.snippet,
               "miditype": c.miditype, "midichan": c.midichan,
               "midipara1": c.midipara1, "midipara2": c.midipara2})
    }

    fn slot_json_lib(s: &NativeLibSlot) -> Value {
        json!({"slot": s.slot, "name": s.name, "type": s.typ,
               "flags": s.flags, "hasdata": s.hasdata})
    }

    /// Lock-local inventory descriptor for the generic Flutter native
    /// state/preset views. `session` must be the live session (`None`
    /// when disconnected).
    ///
    /// `connected` tracks the live link (a session exists), while the banks
    /// and mutating operations additionally require ownership: a cache filled
    /// under an older (reconnected) session reports `connected:true` with
    /// empty banks and read-only operations until a fresh refresh re-pins
    /// the owner, rather than laundering stale slots.
    ///
    /// Generic compatibility: `generation` (mirrors `revision`), `capacity`
    /// (scene slots, the primary snapshot bank behind `native_snapshots`)
    /// and `slots` (observed scenes as `{slot,name}`, same filter as
    /// `native_snapshots_with`) satisfy the shared `NativeLibraryVm` guard
    /// shape next to the detail banks below. Nothing is invented for
    /// unobserved slots: `slots` only carries desk-observed scenes, and an
    /// empty bank never proves the desk empty.
    ///
    /// Provider-owned UI contract: `banks` carries every range, label,
    /// operation id and argument contract the generic renderer needs, so a
    /// new mixer with the same capability schema only needs its own provider
    /// integration. Each bank is
    /// `{id,title,row_key,slot_min,slot_max[,bank],rows,save,recall,rename,
    /// delete,add}` where `row_key` is `"slot"` (rows `{slot,name}`) or
    /// `"position"` (cue rows `{position,number,name}`), `slot_min/max`
    /// bound the slot/number prompt (for `position` rows the `number` must
    /// lie inside the domain), and each action is `{op[,prompt_title,
    /// confirm_title,confirm_message]}` with `{slot}`/`{name}`/`{bank}`/
    /// `{number}` placeholders. `copy_forms` carries `{id,op,title,
    /// submit_label,fields:[{key,label,min,max}]}` integer forms,
    /// `archive` carries `{export_op,inspect_op,import_op,
    /// import_confirm_title,import_confirm_message,kinds:[{id,label[,
    /// slot_min,slot_max]}]}`, `headers` carries provider-owned header
    /// lines, and `banks_note` an optional one-line availability note.
    /// Descriptors are only emitted while their operations are advertised;
    /// anything else is a malformed guard the generic side rejects.
    pub fn inventory_json(&self, session: Option<&Arc<OscSession>>) -> Value {
        let connected = session.is_some();
        let owned = session.is_some_and(|s| self.is_owner(s));
        let mut operations = vec!["inspect", "refresh"];
        if owned && self.complete {
            operations.extend([
                "scene_save",
                "scene_recall",
                "scene_rename",
                "scene_delete",
                "snippet_save",
                "snippet_recall",
                "snippet_rename",
                "snippet_delete",
                "cue_add",
                "export_archive",
                "import_archive",
                "inspect_archive",
            ]);
        }
        if owned && self.complete && self.libs_supported == Some(true) {
            operations.extend([
                "lib_save",
                "lib_recall",
                "lib_rename",
                "lib_delete",
                "libchan_copy",
            ]);
        }
        let mut scene_slots: Vec<Value> = if owned {
            let mut rows: Vec<Value> = self
                .scenes
                .iter()
                .filter(|s| s.hasdata || !s.name.is_empty())
                .map(|s| json!({"slot": s.slot, "name": s.name}))
                .collect();
            rows.sort_by(|a, b| a["slot"].as_u64().cmp(&b["slot"].as_u64()));
            rows
        } else {
            Vec::new()
        };
        // Deduplicate defensively: the cache is slot-keyed, but the generic
        // guard requires unique `slot` ids, so first-wins keeps it honest.
        {
            let mut seen = std::collections::HashSet::new();
            scene_slots.retain(|row| row["slot"].as_u64().is_some_and(|slot| seen.insert(slot)));
        }
        json!({
            "authority": "mixer",
            "provider": "x32",
            "label": "X32 native show + libraries",
            "connected": connected,
            "generation": self.generation,
            "revision": self.revision,
            "complete": owned && self.complete,
            "error": if owned { self.error.clone() } else { None },
            "refresh_title": "Read show + libraries?",
            "refresh_description": "Read the scene, snippet and cue inventory and library names from the mixer.",
            "headers": if owned { self.show_name.clone().filter(|n| !n.is_empty()).map(|n| vec![format!("Desk show: {n}")]).unwrap_or_default() } else { Vec::new() },
            "capacity": SCENE_SLOTS,
            "slots": scene_slots,
            "bank_capacity": {"scenes": SCENE_SLOTS, "snippets": SNIPPET_SLOTS,
                         "cues": CUE_POSITIONS, "libs_each": LIB_SLOTS},
            "operations": operations,
            "banks": Self::bank_descriptors(self, owned),
            "copy_forms": Self::copy_form_descriptors(self, owned),
            "archive": Self::archive_descriptor(self, owned),
            "banks_note": Self::banks_note(self, owned),
            "scenes": if owned { self.scenes.iter().map(Self::slot_json_scene).collect::<Vec<_>>() } else { vec![] },
            "snippets": if owned { self.snippets.iter().map(Self::slot_json_snippet).collect::<Vec<_>>() } else { vec![] },
            "cues": if owned { self.cues.iter().map(Self::slot_json_cue).collect::<Vec<_>>() } else { vec![] },
            "libs_supported": if owned { self.libs_supported } else { None },
            "libs": {
                "ch": if owned { self.lib_ch.iter().map(Self::slot_json_lib).collect::<Vec<_>>() } else { vec![] },
                "fx": if owned { self.lib_fx.iter().map(Self::slot_json_lib).collect::<Vec<_>>() } else { vec![] },
                "r": if owned { self.lib_r.iter().map(Self::slot_json_lib).collect::<Vec<_>>() } else { vec![] },
                "mon": if owned { self.lib_mon.iter().map(Self::slot_json_lib).collect::<Vec<_>>() } else { vec![] },
            },
            "limitations": LIMITATIONS,
        })
    }

    /// Generic renderer descriptors. Only emitted for an owned, complete
    /// cache (the same gate as the mutating operations); otherwise the
    /// generic side sees empty banks and hides the section.
    fn bank_descriptors(state: &NativeState, owned: bool) -> Vec<Value> {
        if !(owned && state.complete) {
            return Vec::new();
        }
        let mut scenes: Vec<Value> = state
            .scenes
            .iter()
            .map(|s| json!({"slot": s.slot, "name": s.name}))
            .collect();
        scenes.sort_by(|a, b| a["slot"].as_u64().cmp(&b["slot"].as_u64()));
        let mut snippets: Vec<Value> = state
            .snippets
            .iter()
            .map(|s| json!({"slot": s.slot, "name": s.name}))
            .collect();
        snippets.sort_by(|a, b| a["slot"].as_u64().cmp(&b["slot"].as_u64()));
        let mut cues: Vec<Value> = state
            .cues
            .iter()
            .map(|c| json!({"position": c.position, "number": c.numb, "name": c.name}))
            .collect();
        cues.sort_by(|a, b| a["position"].as_u64().cmp(&b["position"].as_u64()));
        let mut banks = vec![
            json!({
                "id": "scenes", "title": "Desk scenes", "row_key": "slot",
                "slot_min": 0, "slot_max": 99, "rows": scenes,
                "save": {"op": "scene_save", "prompt_title": "Save desk scene"},
                "recall": {"op": "scene_recall", "confirm_title": "Recall desk scene?",
                           "confirm_message": "Recall scene {slot} on the desk. This changes live mixer settings."},
                "rename": {"op": "scene_rename", "prompt_title": "Rename desk scene"},
                "delete": {"op": "scene_delete", "confirm_title": "Delete desk scene?",
                           "confirm_message": "Delete scene {slot} on the desk. This cannot be undone."},
            }),
            json!({
                "id": "snippets", "title": "Desk snippets", "row_key": "slot",
                "slot_min": 0, "slot_max": 99, "rows": snippets,
                "save": {"op": "snippet_save", "prompt_title": "Save desk snippet"},
                "recall": {"op": "snippet_recall", "confirm_title": "Recall desk snippet?",
                           "confirm_message": "Recall snippet {slot} on the desk. This changes live mixer settings."},
                "rename": {"op": "snippet_rename", "prompt_title": "Rename desk snippet"},
                "delete": {"op": "snippet_delete", "confirm_title": "Delete desk snippet?",
                           "confirm_message": "Delete snippet {slot} on the desk. This cannot be undone."},
            }),
            json!({
                "id": "cues", "title": "Desk cues", "row_key": "position",
                "slot_min": 100, "slot_max": 50099, "rows": cues,
                "add": {"op": "cue_add", "prompt_title": "Add cue", "number_arg": "number"},
            }),
        ];
        if state.libs_supported == Some(true) {
            for (bank, title) in [
                ("ch", "Channel presets"),
                ("fx", "FX presets"),
                ("r", "Routing presets"),
                ("mon", "Monitor presets"),
            ] {
                let slots = match bank {
                    "ch" => &state.lib_ch,
                    "fx" => &state.lib_fx,
                    "r" => &state.lib_r,
                    _ => &state.lib_mon,
                };
                let mut rows: Vec<Value> = slots
                    .iter()
                    .map(|s| json!({"slot": s.slot, "name": s.name}))
                    .collect();
                rows.sort_by(|a, b| a["slot"].as_u64().cmp(&b["slot"].as_u64()));
                banks.push(json!({
                    "id": format!("libs-{bank}"), "title": title, "bank": bank,
                    "row_key": "slot", "slot_min": 1, "slot_max": 100, "rows": rows,
                    "save": {"op": "lib_save", "prompt_title": format!("Save {bank} preset")},
                    "recall": {"op": "lib_recall", "confirm_title": "Recall library preset?",
                               "confirm_message": format!("Recall {bank} {{slot}} on the desk. This changes live mixer settings.")},
                    "rename": {"op": "lib_rename", "prompt_title": format!("Rename {bank} preset")},
                    "delete": {"op": "lib_delete", "confirm_title": "Delete library preset?",
                               "confirm_message": format!("Delete {bank} {{slot}} on the desk. This cannot be undone.")},
                }));
            }
        }
        banks
    }

    fn copy_form_descriptors(state: &NativeState, owned: bool) -> Vec<Value> {
        if !(owned && state.complete && state.libs_supported == Some(true)) {
            return Vec::new();
        }
        vec![json!({
            "id": "libchan_copy", "op": "libchan_copy",
            "title": "Copy channel preset", "submit_label": "Copy",
            "fields": [
                {"key": "src", "label": "Source preset index (0-based)", "min": 0, "max": 99},
                {"key": "dst", "label": "Destination preset index (0-based)", "min": 0, "max": 99},
            ],
        })]
    }

    /// Local archive descriptor. Archives are OpenFader-side JSON copies of
    /// observed node text, never desk USB images.
    fn archive_descriptor(state: &NativeState, owned: bool) -> Value {
        if !(owned && state.complete) {
            return Value::Null;
        }
        let _ = state;
        json!({
            "export_op": "export_archive", "inspect_op": "inspect_archive",
            "import_op": "import_archive",
            "import_confirm_title": "Import archive to mixer?",
            "import_confirm_message": "Write archived values to the mixer. A rollback backup is returned with the result.",
            "kinds": [
                {"id": "scene", "label": "current mix"},
                {"id": "routing", "label": "Routing archive"},
            ],
        })
    }

    fn banks_note(state: &NativeState, owned: bool) -> Value {
        if owned && state.complete && state.libs_supported != Some(true) {
            json!("Library presets unavailable: the desk did not answer library queries.")
        } else {
            Value::Null
        }
    }
}

// Native-operation verification limits: an empty name or hasdata=0 does
// not prove a slot empty on every firmware. Recall status acknowledges
// dispatch, not the resulting audio state. Scene/snippet copy and cue
// recall/delete have no supported OSC form here. OSC cannot transfer desk
// USB files (*.scn/*.snp/*.chn/*.efx/*.rou, CustomBootState.scn); archives
// serialize observed node text. Library operations require /-libs replies;
// the independent emulator is silent there. Its scene/snippet hasdata is
// stuck at zero even after save, so our emulator checks use independent
// names/readback and report uncertain receipts for unverified contents.
// Protocol and emulator evidence belongs in implementation notes and
// verification reports; product copy explains the user-visible boundary.
const LIMITATIONS: &str = "Native mixer entries are separate from OpenFader's local library. Local archives use OpenFader JSON and are not desk USB files. Refresh before changing a native entry.";

// ---------------------------------------------------------------------------
// wire helpers
// ---------------------------------------------------------------------------

async fn read_string(session: &Arc<OscSession>, path: &str) -> anyhow::Result<String> {
    let reply = session.query_timeout(path, LEAF_TIMEOUT).await?;
    match reply.args.as_slice() {
        [OscType::String(s)] => Ok(s.clone()),
        other => anyhow::bail!("x32 native: {path} returned unexpected shape {other:?}"),
    }
}

async fn read_int(session: &Arc<OscSession>, path: &str) -> anyhow::Result<i32> {
    let reply = session.query_timeout(path, LEAF_TIMEOUT).await?;
    match reply.args.as_slice() {
        [OscType::Int(v)] => Ok(*v),
        other => anyhow::bail!("x32 native: {path} returned unexpected shape {other:?}"),
    }
}

/// Correlated dataset status: the desk answers `/save` (etc.) addressed as
/// the operation with `,si <type> <1|0>`. Address, type echo and status bit
/// must all match the request; anything else (including silence) is an
/// error, never a success.
fn check_status(reply: &rosc::OscMessage, op: &str, dataset: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        reply.addr == format!("/{op}"),
        "x32 native: {op} reply addressed {} (expected /{op})",
        reply.addr
    );
    match reply.args.as_slice() {
        [OscType::String(echo), OscType::Int(1)] if echo == dataset => Ok(()),
        [OscType::String(echo), OscType::Int(0)] if echo == dataset => {
            anyhow::bail!("x32 native: desk refused {op} {dataset}")
        }
        other => anyhow::bail!("x32 native: {op} {dataset} unexpected reply {other:?}"),
    }
}

async fn dataset_op(
    session: &Arc<OscSession>,
    op: &str,
    args: Vec<OscType>,
    dataset: &str,
    check: Check<'_>,
) -> anyhow::Result<()> {
    anyhow::ensure!(check(), "x32 native: session changed before dispatch");
    let reply = session
        .query_with_args_timeout(&format!("/{op}"), args, QUERY_TIMEOUT, |_| true)
        .await?;
    anyhow::ensure!(check(), "x32 native: session changed during dispatch");
    check_status(&reply, op, dataset)
}

/// Bounded-concurrency fan-out over owned items (no extra executor deps).
async fn fan_out_simple<T, O, F, Fut>(items: Vec<T>, make: F) -> Vec<O>
where
    T: Send + 'static,
    O: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = O> + Send + 'static,
{
    let mut out = Vec::with_capacity(items.len());
    let mut queue: std::collections::VecDeque<T> = items.into();
    while !queue.is_empty() {
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..FANOUT {
            if let Some(item) = queue.pop_front() {
                set.spawn(make(item));
            } else {
                break;
            }
        }
        while let Some(joined) = set.join_next().await {
            if let Ok(value) = joined {
                out.push(value);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// validation (forbid unknown addresses/types/ranges before any write)
// ---------------------------------------------------------------------------

fn scene_slot(slot: i64) -> anyhow::Result<u8> {
    anyhow::ensure!(
        (0..100).contains(&slot),
        "x32 native: scene slot must be 0..99"
    );
    Ok(slot as u8)
}

fn snippet_slot(slot: i64) -> anyhow::Result<u8> {
    anyhow::ensure!(
        (0..100).contains(&slot),
        "x32 native: snippet slot must be 0..99"
    );
    Ok(slot as u8)
}

fn lib_bank(bank: &str) -> anyhow::Result<&'static str> {
    match bank {
        "ch" => Ok("ch"),
        "fx" => Ok("fx"),
        "r" => Ok("r"),
        "mon" => Ok("mon"),
        _ => anyhow::bail!("x32 native: unknown library bank {bank:?} (ch|fx|r|mon)"),
    }
}

/// Library slots are 1..100 on the wire leaves; dataset indices are 0-based.
fn lib_slot(slot: i64) -> anyhow::Result<u8> {
    anyhow::ensure!(
        (1..=100).contains(&slot),
        "x32 native: library slot must be 1..100"
    );
    Ok(slot as u8)
}

fn dataset_of(bank: &str) -> &'static str {
    match bank {
        "ch" => "libchan",
        "fx" => "libfx",
        "r" => "librout",
        "mon" => "libmon",
        _ => "libchan",
    }
}

fn validate_name(name: &str) -> anyhow::Result<()> {
    openfader_core::native_library::validate_name(name)?;
    Ok(())
}

/// Cue number `xxx.x.x` packed as an int (100 = 1.0.0, 50000 = 500.0.0).
fn cue_number(n: i64) -> anyhow::Result<i32> {
    anyhow::ensure!(
        (100..=50099).contains(&n),
        "x32 native: cue number out of range"
    );
    let major = n / 100;
    anyhow::ensure!(
        (1..=500).contains(&major),
        "x32 native: cue major out of range"
    );
    Ok(n as i32)
}

fn action_i64(action: &Value, key: &str) -> anyhow::Result<i64> {
    action[key]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("x32 native: missing integer {key:?}"))
}

fn action_str<'a>(action: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    action[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("x32 native: missing string {key:?}"))
}

// ---------------------------------------------------------------------------
// X32Driver inherent API (no new struct fields needed; state is passed in)
// ---------------------------------------------------------------------------

impl X32Driver {
    /// Lock-local inventory view for `MixerDriver::native_library`.
    pub fn native_inventory_with(&self, state: &NativeState) -> Value {
        state.inventory_json(self.session().ok().as_ref())
    }

    /// Poison-safe inventory view for receipts: a poisoned lock reads as
    /// `Null` (unavailable) rather than panicking or serving stale slots.
    fn inventory_or_null(&self, state: &Mutex<NativeState>) -> Value {
        match state.lock() {
            Ok(cache) => cache.inventory_json(self.session().ok().as_ref()),
            Err(_) => Value::Null,
        }
    }

    /// Read-only desk snapshot slots for `MixerDriver::native_snapshots`:
    /// X32 scenes with mixer data, in desk slot order. Names come from the
    /// desk inventory; nothing is invented for unobserved slots. A live
    /// session without an owned cache reads `connected:true` with no slots
    /// (engine `loading`); only a missing session reads disconnected.
    pub fn native_snapshots_with(&self, state: &NativeState) -> openfader_core::NativeSnapshots {
        let session = self.session().ok();
        if session.is_none() {
            return openfader_core::NativeSnapshots::default();
        }
        let owned = session.as_ref().is_some_and(|s| state.is_owner(s));
        if !owned {
            return openfader_core::NativeSnapshots {
                connected: true,
                ..openfader_core::NativeSnapshots::default()
            };
        }
        let mut slots: Vec<openfader_core::NativeSnapshotSlot> = state
            .scenes
            .iter()
            .filter(|s| s.hasdata || !s.name.is_empty())
            .map(|s| openfader_core::NativeSnapshotSlot {
                slot: u32::from(s.slot),
                name: s.name.clone(),
            })
            .collect();
        slots.sort_by_key(|s| s.slot);
        openfader_core::NativeSnapshots {
            complete: state.complete,
            error: state.error.clone(),
            received: !slots.is_empty() || state.complete,
            connected: true,
            current: None,
            slots,
        }
    }

    /// Guarded native operation dispatch. `expected` must equal the current
    /// inventory view (session/identity aware); every mutating op re-reads
    /// its target leaf first, correlates the desk status reply, then
    /// read-backs where the desk offers readback, and invalidates the cache
    /// on any uncertainty. `Err` is pre-mutation rejection; once bytes may
    /// have left, an uncertain receipt is returned instead of retrying.
    pub async fn native_action_with(
        &self,
        state: &Mutex<NativeState>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        let session = self.session()?;
        let valid = || {
            check()
                && state
                    .lock()
                    .map(|cache| {
                        cache.is_owner(&session)
                            && &cache.inventory_json(Some(&session)) == expected
                    })
                    .unwrap_or(false)
        };
        let op = action["op"].as_str().unwrap_or("");
        // Refresh bootstraps an unowned cache (first refresh after connect):
        // ownership is the *result* of a refresh, so entry only requires the
        // engine-verified inventory to still match under a live session.
        // Every other op additionally requires ownership (see `valid`).
        if op == "refresh" {
            anyhow::ensure!(
                check() && &self.inventory_or_null(state) == expected,
                "x32 native: session or inventory changed; nothing dispatched"
            );
            match self.native_refresh_into(state, check).await {
                Ok(inventory) => {
                    let complete = inventory["complete"] == true;
                    let message = if complete {
                        "Native show + library inventory read from the mixer.".to_string()
                    } else {
                        inventory["error"]
                            .as_str()
                            .unwrap_or("Native inventory incomplete; observed rows are partial.")
                            .to_string()
                    };
                    return Ok(openfader_core::native_library::receipt(
                        if complete { "confirmed" } else { "uncertain" },
                        message,
                        inventory,
                    ));
                }
                Err(error) => {
                    if let Ok(mut cache) = state.lock() {
                        cache.revision += 1;
                        cache.generation = cache.revision;
                        cache.complete = false;
                        cache.error = Some(format!("{error:#}"));
                    }
                    return Ok(openfader_core::native_library::receipt(
                        "uncertain",
                        format!("Refresh incomplete; retry before operating: {error:#}"),
                        self.inventory_or_null(state),
                    ));
                }
            }
        }
        if op == "inspect_archive" {
            let archive = action
                .get("archive")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("x32 native: missing archive"))?;
            let summary = validate_archive(&archive)?;
            anyhow::ensure!(valid(), "x32 native: session changed during inspection");
            return Ok(openfader_core::native_library::receipt(
                "confirmed",
                "Archive validated locally. No mixer write or recall was performed.",
                json!({"archive": summary, "inventory": expected.clone()}),
            ));
        }
        if op == "export_archive" {
            return self
                .export_archive_into(&session, state, action, expected, check)
                .await;
        }
        if op == "import_archive" {
            return self
                .import_archive_into(&session, state, action, expected, check)
                .await;
        }
        // Mutating dataset ops below: hold the shared write lane across the
        // whole pre-read/dispatch/readback sequence so no other client
        // interleaves mid-operation.
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            valid(),
            "x32 native: session or inventory changed; nothing dispatched"
        );
        let mut dispatched = false;
        let receipt = match op {
            "scene_save" => {
                self.op_scene_save(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "scene_recall" => {
                self.op_recall(&session, "scene", action, expected, check, &mut dispatched)
                    .await
            }
            "scene_rename" => {
                self.op_rename(&session, "scene", action, expected, check, &mut dispatched)
                    .await
            }
            "scene_delete" => {
                self.op_delete(&session, "scene", action, expected, check, &mut dispatched)
                    .await
            }
            "snippet_save" => {
                self.op_snippet_save(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "snippet_recall" => {
                self.op_recall(
                    &session,
                    "snippet",
                    action,
                    expected,
                    check,
                    &mut dispatched,
                )
                .await
            }
            "snippet_rename" => {
                self.op_rename(
                    &session,
                    "snippet",
                    action,
                    expected,
                    check,
                    &mut dispatched,
                )
                .await
            }
            "snippet_delete" => {
                self.op_delete(
                    &session,
                    "snippet",
                    action,
                    expected,
                    check,
                    &mut dispatched,
                )
                .await
            }
            "cue_add" => {
                self.op_cue_add(&session, action, check, &mut dispatched)
                    .await
            }
            "lib_save" => {
                self.op_lib_save(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "lib_recall" => {
                self.op_lib_recall(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "lib_rename" => {
                self.op_lib_rename(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "lib_delete" => {
                self.op_lib_delete(&session, action, expected, check, &mut dispatched)
                    .await
            }
            "libchan_copy" => {
                self.op_libchan_copy(&session, action, expected, check, &mut dispatched)
                    .await
            }
            _ => anyhow::bail!("x32 native: unsupported operation {op:?}"),
        };
        // A dispatched mutation invalidates the cache even when the outcome
        // is uncertain: the next inspect/refresh re-reads from the desk
        // instead of trusting our dispatch. Pre-dispatch rejections leave
        // the cache untouched.
        if dispatched {
            if let Ok(mut cache) = state.lock() {
                cache.revision += 1;
                cache.generation = cache.revision;
                cache.complete = false;
            }
        }
        match receipt {
            Ok((status, message)) => Ok(openfader_core::native_library::receipt(
                status,
                message,
                self.inventory_or_null(state),
            )),
            Err(error) if dispatched => Ok(openfader_core::native_library::receipt(
                "uncertain",
                format!("{error:#}. Refresh before retrying."),
                self.inventory_or_null(state),
            )),
            Err(error) => Err(error),
        }
    }
}

// ---------------------------------------------------------------------------
// dataset operations (each: validate -> pre-read -> dispatch -> correlate)
// ---------------------------------------------------------------------------

impl X32Driver {
    async fn op_scene_save(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let slot = scene_slot(action_i64(action, "slot")?)?;
        let name = action_str(action, "name")?;
        let notes = action.get("notes").and_then(Value::as_str).unwrap_or("");
        validate_name(name)?;
        anyhow::ensure!(
            !notes.contains(['\0', '\n', '"']),
            "x32 native: notes need no NUL/newline/quote"
        );
        anyhow::ensure!(notes.len() <= 63, "x32 native: notes limited to 63 bytes");
        let prior: Option<String> = expected["scenes"]
            .as_array()
            .and_then(|slots| slots.iter().find(|s| s["slot"] == slot))
            .and_then(|s| s["name"].as_str())
            .map(str::to_string);
        let live = read_string(session, &format!("/-show/showfile/scene/{slot:03}/name")).await?;
        if let Some(prior) = prior {
            // Saving over a listed slot must see the inventoried name;
            // saving to an unlisted (empty) slot accepts any live value.
            anyhow::ensure!(
                live == prior,
                "x32 native: scene name changed; refresh first"
            );
        }
        anyhow::ensure!(check(), "x32 native: session changed");
        *dispatched = true;
        dataset_op(
            session,
            "save",
            vec![
                OscType::String("scene".into()),
                OscType::Int(i32::from(slot)),
                OscType::String(name.into()),
                OscType::String(notes.into()),
            ],
            "scene",
            check,
        )
        .await?;
        // Readback: name must match what we saved (the emulator stores it;
        // hasdata stays 0 there, so names are authoritative).
        match read_string(session, &format!("/-show/showfile/scene/{slot:03}/name")).await {
            Ok(actual) if actual == name => Ok((
                "uncertain",
                format!(
                    "Scene {slot:03} save accepted by the desk (status 1) and name reads back; audio contents are not independently verified."
                ),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: scene {slot:03} name reads back {actual:?}, save unverified"
            ),
            Err(error) => {
                anyhow::bail!("x32 native: scene {slot:03} saved but name unreadable: {error:#}")
            }
        }
    }

    async fn op_recall(
        &self,
        session: &Arc<OscSession>,
        dataset: &str,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let slot = action_i64(action, "slot")?;
        let (key, leaf) = match dataset {
            "scene" => ("scenes", scene_slot(slot)? as i64),
            _ => ("snippets", snippet_slot(slot)? as i64),
        };
        let known = expected[key]
            .as_array()
            .is_some_and(|slots| slots.iter().any(|s| s["slot"] == leaf));
        anyhow::ensure!(known, "x32 native: refresh {key} first");
        *dispatched = true;
        dataset_op(
            session,
            "load",
            vec![OscType::String(dataset.into()), OscType::Int(leaf as i32)],
            dataset,
            check,
        )
        .await?;
        Ok((
            "uncertain",
            format!(
                "Desk accepted {dataset} {leaf} recall (status 1). The recall changes live audio; its full contents were not independently verified. Refresh before further operations."
            ),
        ))
    }

    async fn op_rename(
        &self,
        session: &Arc<OscSession>,
        dataset: &str,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let slot = action_i64(action, "slot")?;
        let slot = match dataset {
            "scene" => scene_slot(slot)? as i64,
            _ => snippet_slot(slot)? as i64,
        };
        let key = if dataset == "scene" {
            "scenes"
        } else {
            "snippets"
        };
        let new_name = action_str(action, "name")?;
        validate_name(new_name)?;
        let prior: String = expected[key]
            .as_array()
            .and_then(|slots| slots.iter().find(|s| s["slot"] == slot))
            .and_then(|s| s["name"].as_str())
            .ok_or_else(|| anyhow::anyhow!("x32 native: refresh {key} first"))?
            .to_string();
        let leaf = format!("/-show/showfile/{dataset}/{slot:03}/name");
        let live = read_string(session, &leaf).await?;
        anyhow::ensure!(live == prior, "x32 native: name changed; refresh first");
        anyhow::ensure!(check(), "x32 native: session changed");
        *dispatched = true;
        // The independent emulator does not implement /rename (silent);
        // a real desk answers /rename with correlated status.
        let reply = session
            .query_with_args_timeout(
                "/rename",
                vec![
                    OscType::String(dataset.into()),
                    OscType::Int(slot as i32),
                    OscType::String(new_name.into()),
                ],
                QUERY_TIMEOUT,
                |_| true,
            )
            .await?;
        anyhow::ensure!(check(), "x32 native: session changed during dispatch");
        check_status(&reply, "rename", dataset)?;
        match read_string(session, &leaf).await {
            Ok(actual) if actual == new_name => Ok((
                "confirmed",
                format!("{dataset} {slot} rename read back from the mixer."),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: {dataset} {slot} name reads back {actual:?}, rename unverified"
            ),
            Err(error) => anyhow::bail!("x32 native: rename sent but unreadable: {error:#}"),
        }
    }

    async fn op_delete(
        &self,
        session: &Arc<OscSession>,
        dataset: &str,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let slot = action_i64(action, "slot")?;
        let slot = match dataset {
            "scene" => scene_slot(slot)? as i64,
            _ => snippet_slot(slot)? as i64,
        };
        let key = if dataset == "scene" {
            "scenes"
        } else {
            "snippets"
        };
        let prior: String = expected[key]
            .as_array()
            .and_then(|slots| slots.iter().find(|s| s["slot"] == slot))
            .and_then(|s| s["name"].as_str())
            .ok_or_else(|| anyhow::anyhow!("x32 native: refresh {key} first"))?
            .to_string();
        let leaf = format!("/-show/showfile/{dataset}/{slot:03}/name");
        let live = read_string(session, &leaf).await?;
        anyhow::ensure!(live == prior, "x32 native: name changed; refresh first");
        anyhow::ensure!(check(), "x32 native: session changed");
        *dispatched = true;
        dataset_op(
            session,
            "delete",
            vec![OscType::String(dataset.into()), OscType::Int(slot as i32)],
            dataset,
            check,
        )
        .await?;
        match read_string(session, &leaf).await {
            Ok(actual) if actual.is_empty() || actual == "-" => Ok((
                "uncertain",
                format!(
                    "{dataset} {slot} delete accepted; name cleared on readback but full erasure is unverified."
                ),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: {dataset} {slot} name still reads back {actual:?} after delete"
            ),
            Err(_) => Ok((
                "uncertain",
                format!("{dataset} {slot} delete accepted; slot unreadable afterwards."),
            )),
        }
    }

    async fn op_snippet_save(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let slot = snippet_slot(action_i64(action, "slot")?)?;
        let name = action_str(action, "name")?;
        validate_name(name)?;
        let prior: Option<String> = expected["snippets"]
            .as_array()
            .and_then(|slots| slots.iter().find(|s| s["slot"] == slot))
            .and_then(|s| s["name"].as_str())
            .map(str::to_string);
        // Snippet filters live on the slot leaves; the desk saves the
        // current mix filtered by them. Optional caller filters are written
        // first (validated ranges), verified, then saved.
        let base = format!("/-show/showfile/snippet/{slot:03}");
        let mut staged = Vec::new();
        for (leaf, max) in [
            ("eventtyp", 0x1FF_FFFF_i64),
            ("channels", i64::from(u32::MAX)),
            ("auxbuses", i64::from(u32::MAX)),
            ("maingrps", 0xFFFF_i64),
        ] {
            if let Some(v) = action.get(leaf).and_then(Value::as_i64) {
                anyhow::ensure!(
                    (0..=max).contains(&v),
                    "x32 native: snippet {leaf} out of range"
                );
                staged.push((leaf, v as i32));
            }
        }
        let live = read_string(session, &format!("{base}/name")).await?;
        if let Some(prior) = prior {
            anyhow::ensure!(
                live == prior,
                "x32 native: snippet name changed; refresh first"
            );
        }
        *dispatched = true;
        for (leaf, value) in &staged {
            session
                .send(&format!("{base}/{leaf}"), vec![OscType::Int(*value)])
                .await?;
            let actual = read_int(session, &format!("{base}/{leaf}")).await?;
            anyhow::ensure!(
                actual == *value,
                "x32 native: snippet filter {leaf} did not read back; save not dispatched"
            );
            anyhow::ensure!(check(), "x32 native: session changed");
        }
        *dispatched = true;
        dataset_op(
            session,
            "save",
            vec![
                OscType::String("snippet".into()),
                OscType::Int(i32::from(slot)),
                OscType::String(name.into()),
            ],
            "snippet",
            check,
        )
        .await?;
        match read_string(session, &format!("{base}/name")).await {
            Ok(actual) if actual == name => Ok((
                "uncertain",
                format!(
                    "Snippet {slot:03} save accepted and name reads back; filtered audio contents are not independently verified."
                ),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: snippet {slot:03} name reads back {actual:?}, save unverified"
            ),
            Err(error) => anyhow::bail!("x32 native: snippet saved but unreadable: {error:#}"),
        }
    }

    async fn op_cue_add(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let number = cue_number(action_i64(action, "number")?)?;
        let name = action_str(action, "name")?;
        validate_name(name)?;
        *dispatched = true;
        dataset_op(
            session,
            "add",
            vec![
                OscType::String("cue".into()),
                OscType::Int(number),
                OscType::String(name.into()),
            ],
            "cue",
            check,
        )
        .await?;
        Ok((
            "uncertain",
            "Cue add accepted by the desk (status 1). Cue positions shift on real desks; refresh to see the resulting list. Cue recall and cue delete have no OSC form.".to_string(),
        ))
    }

    fn expected_lib<'a>(
        &self,
        expected: &'a Value,
        bank: &str,
        slot: u8,
    ) -> anyhow::Result<&'a Value> {
        expected["libs"][bank]
            .as_array()
            .and_then(|slots| slots.iter().find(|s| s["slot"] == slot))
            .ok_or_else(|| anyhow::anyhow!("x32 native: refresh libraries first"))
    }

    async fn op_lib_save(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let bank = lib_bank(action_str(action, "bank")?)?;
        let slot = lib_slot(action_i64(action, "slot")?)?;
        let dataset = dataset_of(bank);
        let name = action_str(action, "name")?;
        validate_name(name)?;
        // Saving to an unlisted (empty) library slot is the normal case:
        // only a listed slot must still show its inventoried name.
        let prior_name: &str = self
            .expected_lib(expected, bank, slot)
            .ok()
            .and_then(|prior| prior["name"].as_str())
            .unwrap_or("");
        let leaf = format!("/-libs/{bank}/{slot:03}/name");
        let live = read_string(session, &leaf).await?;
        if !prior_name.is_empty() {
            anyhow::ensure!(
                live == prior_name,
                "x32 native: library name changed; refresh first"
            );
        }
        anyhow::ensure!(check(), "x32 native: session changed");
        let args = match bank {
            "ch" => {
                let from = action.get("from").and_then(Value::as_i64).unwrap_or(0);
                anyhow::ensure!((0..71).contains(&from), "x32 native: channel source 0..70");
                vec![
                    OscType::String(dataset.into()),
                    OscType::Int(i32::from(slot - 1)),
                    OscType::String(name.into()),
                    OscType::Int(from as i32),
                ]
            }
            "fx" => {
                let from = action.get("from").and_then(Value::as_i64).unwrap_or(0);
                anyhow::ensure!((0..8).contains(&from), "x32 native: fx source 0..7");
                vec![
                    OscType::String(dataset.into()),
                    OscType::Int(i32::from(slot - 1)),
                    OscType::String(name.into()),
                    OscType::Int(from as i32),
                ]
            }
            _ => vec![
                OscType::String(dataset.into()),
                OscType::Int(i32::from(slot - 1)),
                OscType::String(name.into()),
            ],
        };
        *dispatched = true;
        dataset_op(session, "save", args, dataset, check).await?;
        match read_string(session, &leaf).await {
            Ok(actual) if actual == name => Ok((
                "uncertain",
                format!(
                    "Library {bank} {slot:03} save accepted and name reads back; preset contents are not independently verified."
                ),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: library {bank} {slot:03} name reads back {actual:?}, save unverified"
            ),
            Err(error) => anyhow::bail!("x32 native: library saved but unreadable: {error:#}"),
        }
    }

    async fn op_lib_recall(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let bank = lib_bank(action_str(action, "bank")?)?;
        let slot = lib_slot(action_i64(action, "slot")?)?;
        let dataset = dataset_of(bank);
        let prior = self.expected_lib(expected, bank, slot)?;
        let prior_name = prior["name"].as_str().unwrap_or("");
        let live = read_string(session, &format!("/-libs/{bank}/{slot:03}/name")).await?;
        anyhow::ensure!(
            live == prior_name,
            "x32 native: library name changed; refresh first"
        );
        anyhow::ensure!(check(), "x32 native: session changed");
        let idx = i32::from(slot - 1);
        let args = match bank {
            "ch" => {
                let target = action.get("target").and_then(Value::as_i64).unwrap_or(0);
                anyhow::ensure!(
                    (0..72).contains(&target),
                    "x32 native: channel target 0..71"
                );
                let scope = action.get("scope").and_then(Value::as_i64).unwrap_or(63);
                anyhow::ensure!((0..=63).contains(&scope), "x32 native: scope 0..63");
                if action.get("scope").is_some() {
                    vec![
                        OscType::String(dataset.into()),
                        OscType::Int(idx),
                        OscType::Int(target as i32),
                        OscType::Int(scope as i32),
                    ]
                } else {
                    vec![
                        OscType::String(dataset.into()),
                        OscType::Int(idx),
                        OscType::Int(target as i32),
                    ]
                }
            }
            "fx" => {
                let target = action.get("target").and_then(Value::as_i64).unwrap_or(0);
                anyhow::ensure!((0..8).contains(&target), "x32 native: fx target 0..7");
                vec![
                    OscType::String(dataset.into()),
                    OscType::Int(idx),
                    OscType::Int(target as i32),
                ]
            }
            _ => vec![OscType::String(dataset.into()), OscType::Int(idx)],
        };
        *dispatched = true;
        dataset_op(session, "load", args, dataset, check).await?;
        Ok((
            "uncertain",
            format!(
                "Desk accepted {dataset} {slot:03} recall (status 1). The recall changes live audio; its contents were not independently verified. Refresh before further operations."
            ),
        ))
    }

    async fn op_lib_rename(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let bank = lib_bank(action_str(action, "bank")?)?;
        let slot = lib_slot(action_i64(action, "slot")?)?;
        let dataset = dataset_of(bank);
        let new_name = action_str(action, "name")?;
        validate_name(new_name)?;
        let prior = self.expected_lib(expected, bank, slot)?;
        let prior_name = prior["name"].as_str().unwrap_or("");
        let leaf = format!("/-libs/{bank}/{slot:03}/name");
        let live = read_string(session, &leaf).await?;
        anyhow::ensure!(
            live == prior_name,
            "x32 native: library name changed; refresh first"
        );
        anyhow::ensure!(check(), "x32 native: session changed");
        *dispatched = true;
        let reply = session
            .query_with_args_timeout(
                "/rename",
                vec![
                    OscType::String(dataset.into()),
                    OscType::Int(i32::from(slot - 1)),
                    OscType::String(new_name.into()),
                ],
                QUERY_TIMEOUT,
                |_| true,
            )
            .await?;
        anyhow::ensure!(check(), "x32 native: session changed during dispatch");
        check_status(&reply, "rename", dataset)?;
        match read_string(session, &leaf).await {
            Ok(actual) if actual == new_name => Ok((
                "confirmed",
                format!("Library {bank} {slot:03} rename read back from the mixer."),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: library {bank} {slot:03} reads back {actual:?}, rename unverified"
            ),
            Err(error) => anyhow::bail!("x32 native: rename sent but unreadable: {error:#}"),
        }
    }

    async fn op_lib_delete(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        let bank = lib_bank(action_str(action, "bank")?)?;
        let slot = lib_slot(action_i64(action, "slot")?)?;
        let dataset = dataset_of(bank);
        let prior = self.expected_lib(expected, bank, slot)?;
        let prior_name = prior["name"].as_str().unwrap_or("");
        let leaf = format!("/-libs/{bank}/{slot:03}/name");
        let live = read_string(session, &leaf).await?;
        anyhow::ensure!(
            live == prior_name,
            "x32 native: library name changed; refresh first"
        );
        anyhow::ensure!(check(), "x32 native: session changed");
        *dispatched = true;
        dataset_op(
            session,
            "delete",
            vec![
                OscType::String(dataset.into()),
                OscType::Int(i32::from(slot - 1)),
            ],
            dataset,
            check,
        )
        .await?;
        match read_string(session, &leaf).await {
            Ok(actual) if actual.is_empty() || actual == "-" => Ok((
                "uncertain",
                format!(
                    "Library {bank} {slot:03} delete accepted; name cleared on readback but full erasure is unverified."
                ),
            )),
            Ok(actual) => anyhow::bail!(
                "x32 native: library {bank} {slot:03} still reads back {actual:?} after delete"
            ),
            Err(_) => Ok((
                "uncertain",
                format!("Library {bank} {slot:03} delete accepted; slot unreadable afterwards."),
            )),
        }
    }

    async fn op_libchan_copy(
        &self,
        session: &Arc<OscSession>,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
        dispatched: &mut bool,
    ) -> anyhow::Result<(&'static str, String)> {
        // Protocol reference v4.09-02, dataset operations: /copy ,sii
        // libchan source-index destination-index. These are library slots,
        // not live channel indices; no processing mask is documented.
        let src = action_i64(action, "src")?;
        let dst = action_i64(action, "dst")?;
        anyhow::ensure!(
            (0..LIB_SLOTS as i64).contains(&src),
            "x32 native: copy source preset index 0..99"
        );
        anyhow::ensure!(
            (0..LIB_SLOTS as i64).contains(&dst),
            "x32 native: copy destination preset index 0..99"
        );
        anyhow::ensure!(
            src != dst,
            "x32 native: copy source and destination must differ"
        );
        anyhow::ensure!(
            action.get("mask").is_none(),
            "x32 native: preset copy has no mask; reopen the copy form"
        );
        let source = self.expected_lib(expected, "ch", (src + 1) as u8)?;
        let source_name = source["name"].as_str().unwrap_or("");
        let source_leaf = format!("/-libs/ch/{:03}/name", src + 1);
        anyhow::ensure!(
            read_string(session, &source_leaf).await? == source_name,
            "x32 native: source preset changed; refresh first"
        );
        let destination_leaf = format!("/-libs/ch/{:03}/name", dst + 1);
        let destination_name = read_string(session, &destination_leaf).await?;
        if let Ok(destination) = self.expected_lib(expected, "ch", (dst + 1) as u8) {
            anyhow::ensure!(
                destination["name"].as_str() == Some(destination_name.as_str()),
                "x32 native: destination preset changed; refresh first"
            );
        } else {
            anyhow::ensure!(
                (destination_name.is_empty() || destination_name == "-")
                    && read_int(session, &format!("/-libs/ch/{:03}/hasdata", dst + 1)).await? == 0,
                "x32 native: destination preset became occupied; refresh first"
            );
        }
        anyhow::ensure!(check(), "x32 native: session changed before copy");
        *dispatched = true;
        dataset_op(
            session,
            "copy",
            vec![
                OscType::String("libchan".into()),
                OscType::Int(src as i32),
                OscType::Int(dst as i32),
            ],
            "libchan",
            check,
        )
        .await?;
        Ok((
            "uncertain",
            "Desk accepted channel-preset copy (status 1); its contents were not independently verified. Refresh before further operations.".to_string(),
        ))
    }
}
