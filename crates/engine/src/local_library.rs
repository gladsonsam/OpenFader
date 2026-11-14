//! Engine-local durable library kernel: one versioned combined document
//! (`scenes.json` v2 through v10) holding the numbered scene slots AND the named show
//! library, with strict whole-document load and atomic commits.
//!
//! Single strict transaction (no silent repair):
//!
//! - The document is `{version: 2|3|4|5|6|7|8|9|10, scenes: [...], shows: [...]}`. Version 3
//!   adds provider-qualified FX insert and mute-group state; version 4 adds global automix state; version 5 adds output taps; version 6 adds monitor settings; version 7 adds RTA settings; version 8 adds output EQ; version 9 adds stereo link settings; version 10 adds full named snapshots. Documents without
//!   those fields remain v2 so older builds can still read them. Version 1
//!   (`{version: 1, scenes: [...]}`) is the legacy numbered-only shape: it
//!   still loads, but it is never rewritten on load — it migrates to v2
//!   only on the next successful mutation commit.
//! - Load is whole-document strict: unknown/newer schema versions, unknown
//!   fields, duplicate IDs/names, integer overflow, non-finite or
//!   out-of-domain normalized faders, and malformed required values all
//!   fail the load. A failed load latches a visible library error, empties
//!   the in-memory libraries, keeps the source file byte-identical, and
//!   BLOCKS every later write that would overwrite it (only an explicit
//!   `reset` — or a restart, which re-reads the file — recovers).
//! - Missing file means empty (ready). Permission/decode errors latch like
//!   any other load failure.
//! - Commits are atomic: build the candidate, validate it, write a sibling
//!   temp file (`create_new` + `write_all` + `flush` + `sync_all`), rename
//!   it over the library, sync the directory where supported — and only
//!   then swap the in-memory libraries and publish success. Pre-replace
//!   failures preserve disk and memory; a post-replace directory-sync/stat
//!   failure swaps memory (the rename landed) but latches `uncertain` and
//!   blocks further writes: never a silent rollback, never a silent reset.
//! - Structural validation retains provider parameter IDs and names. Known
//!   provider-native stored domains also use their archive schema adapter;
//!   recall validates against the destination driver. The byte/count caps
//!   below are explicit resource guards that ERROR, never silent truncation.
//!
//! The current-number marker stays session-only (loading never recalls
//! audio); this kernel never touches the mixer.

#[path = "provider_library_validation.rs"]
mod provider_library_validation;

use provider_library_validation::policy_for_provider;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use openfader_core::{show::Scene, ParamValue, StripId};
use serde::Deserialize;
use serde_json::Value;

use super::engine::{
    StoredAutomix, StoredFx, StoredFxInsert, StoredLinkSettings, StoredMonitor, StoredMonitorValue,
    StoredMuteGroupMembership, StoredMuteGroups, StoredOutputEq, StoredOutputEqTarget,
    StoredOutputTaps, StoredRoute, StoredRta, StoredScene, StoredShow as Show, StoredStrip,
    SCENE_MAX, SCENE_MIN,
};

/// Combined library file name under the engine data dir.
pub const LIBRARY_FILE_NAME: &str = "scenes.json";
/// Current combined document version.
pub const LIBRARY_VERSION: u64 = 11;
const LIBRARY_VERSION_V10: u64 = 10;
const LIBRARY_VERSION_V9: u64 = 9;
const LIBRARY_VERSION_V8: u64 = 8;
const LIBRARY_VERSION_V7: u64 = 7;
const LIBRARY_VERSION_V6: u64 = 6;
const LIBRARY_VERSION_V5: u64 = 5;
const LIBRARY_VERSION_V4: u64 = 4;
const LIBRARY_VERSION_V3: u64 = 3;
const LIBRARY_VERSION_V2: u64 = 2;
/// Legacy numbered-only document version (`{version: 1, scenes}`).
const LIBRARY_VERSION_V1: u64 = 1;

/// Generous explicit resource guards. Every bound ERRORS (never silent
/// truncation); each is far above any real desk (largest topology: sim at
/// 77 strips; 999 numbered slots is the protocol range itself).
pub const MAX_FILE_BYTES: usize = 8 << 20;
pub const MAX_NUMBERED_SCENES: usize = 999;
pub const MAX_SHOWS: usize = 512;
pub const MAX_SCENES_PER_SHOW: usize = 512;
pub const MAX_STRIPS_PER_SCENE: usize = 2048;
pub const MAX_PARAMS_PER_STRIP: usize = 4096;
pub const MAX_NOTES_PER_SCENE: usize = 256;
pub const MAX_FX_ROWS_PER_SCENE: usize = 256;
pub const MAX_FX_PARAMS_PER_SLOT: usize = 256;
pub const MAX_ROUTES_PER_SCENE: usize = 2048;
pub const MAX_NAME_LEN: usize = 256;
pub const MAX_PARAM_ID_LEN: usize = 128;
pub const MAX_PARAM_STRING_LEN: usize = 4096;
pub const MAX_NOTE_KEY_LEN: usize = 128;
pub const MAX_NOTE_VALUE_LEN: usize = 65536;
pub const MAX_FX_TYPE_LEN: usize = 128;
pub const MAX_ROUTE_GROUP_LEN: usize = 64;

/// Where the library persists (`memory` = data-less in-memory commands).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryMode {
    Memory,
    Device,
}

impl LibraryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryMode::Memory => "memory",
            LibraryMode::Device => "device",
        }
    }
}

/// Durability state: `ready` serves and accepts writes; `error` (load
/// failure) and `uncertain` (post-replace sync/stat failure) are latched
/// and block every overwriting write until an explicit reset or restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryStatus {
    Ready,
    Error,
    Uncertain,
}

impl LibraryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryStatus::Ready => "ready",
            LibraryStatus::Error => "error",
            LibraryStatus::Uncertain => "uncertain",
        }
    }
}

/// Engine-held durability health (revision lives on the engine atomically).
#[derive(Clone, Debug)]
pub struct LibraryHealth {
    pub mode: LibraryMode,
    pub state: LibraryStatus,
    pub error: Option<String>,
    pub bytes: Option<u64>,
}

/// Synchronous read-only snapshot for the status getter (and the
/// `local_library_status` event): the same shape the FRB boot gate reads
/// before publishing the singleton.
#[derive(Clone, Debug)]
pub struct LibraryStatusView {
    pub mode: String,
    pub status: String,
    pub revision: u64,
    pub error: Option<String>,
    pub bytes: Option<u64>,
}

/// Whole-document load outcome: validated libraries plus health. On any
/// failure the libraries are empty, the error is latched, and the source
/// file is untouched.
pub struct LoadOutcome {
    pub numbered: HashMap<u16, StoredScene>,
    pub shows: Vec<Show>,
    pub health: LibraryHealth,
    pub doc_version: Option<u64>,
}

/// Library file path under the data dir.
pub fn library_file(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join(LIBRARY_FILE_NAME))
}

/// Current bytes owned by the embedded engine, including interrupted
/// library-write artifacts and remembered addresses. Unknown files are
/// excluded. A missing directory is an empty store; unreadable metadata
/// remains an explicit accounting error rather than a fabricated zero.
pub fn storage_bytes(data_dir: Option<&Path>) -> Result<Option<u64>, String> {
    let Some(dir) = data_dir else {
        return Ok(None);
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(0)),
        Err(e) => return Err(format!("engine storage {} unreadable: {e}", dir.display())),
    };
    let mut bytes = 0u64;
    for entry in entries {
        let entry = entry.map_err(|e| format!("engine storage entry unreadable: {e}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name != LIBRARY_FILE_NAME
            && name != "desks.json"
            && !name.starts_with(".scenes.json.tmp-")
            && !name.starts_with(".desks.json.tmp-")
        {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| {
            format!(
                "engine storage {} metadata unavailable: {e}",
                entry.path().display()
            )
        })?;
        if !metadata.is_file() {
            return Err(format!(
                "engine storage {} is not a file",
                entry.path().display()
            ));
        }
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or_else(|| "engine storage byte count overflow".to_string())?;
    }
    Ok(Some(bytes))
}

/// Strict whole-document load (see module docs). Missing file means empty;
/// anything else unreadable/undecodable/invalid latches `error`.
pub fn load_library(data_dir: Option<&Path>) -> LoadOutcome {
    let empty = |mode: LibraryMode| LoadOutcome {
        numbered: HashMap::new(),
        shows: Vec::new(),
        health: LibraryHealth {
            mode,
            state: LibraryStatus::Ready,
            error: None,
            bytes: None,
        },
        doc_version: None,
    };
    let Some(path) = library_file(data_dir) else {
        return empty(LibraryMode::Memory);
    };
    let fail = |error: String, bytes: Option<u64>| LoadOutcome {
        numbered: HashMap::new(),
        shows: Vec::new(),
        health: LibraryHealth {
            mode: LibraryMode::Device,
            state: LibraryStatus::Error,
            error: Some(error),
            bytes,
        },
        doc_version: None,
    };
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return empty(LibraryMode::Device),
        Err(e) => {
            return fail(
                format!("local library {} unreadable: {e}", path.display()),
                None,
            );
        }
    };
    let len = raw.len() as u64;
    if raw.len() > MAX_FILE_BYTES {
        return fail(
            format!(
                "local library {} too large ({} bytes > {MAX_FILE_BYTES})",
                path.display(),
                raw.len()
            ),
            Some(len),
        );
    }
    let doc: Value = match serde_json::from_slice(&raw) {
        Ok(doc) => doc,
        Err(e) => {
            return fail(
                format!("local library {} is not JSON: {e}", path.display()),
                Some(len),
            );
        }
    };
    match parse_library_doc(&doc) {
        Ok((numbered, shows, version)) => LoadOutcome {
            numbered,
            shows,
            health: LibraryHealth {
                mode: LibraryMode::Device,
                state: LibraryStatus::Ready,
                error: None,
                bytes: Some(len),
            },
            doc_version: Some(version),
        },
        Err(e) => fail(
            format!("local library {} invalid: {e}", path.display()),
            Some(len),
        ),
    }
}

// ================= strict document model =================

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumberedSceneDoc {
    number: u16,
    name: String,
    #[serde(default)]
    saved_at: u64,
    #[serde(default)]
    main_fader: Option<f64>,
    #[serde(default)]
    strips: Vec<NumberedStripDoc>,
    #[serde(default)]
    fx: Vec<FxDoc>,
    #[serde(default)]
    routing: Vec<RouteDoc>,
    #[serde(default)]
    mute_groups: Option<MuteGroupsDoc>,
    #[serde(default)]
    automix: Option<AutomixDoc>,
    #[serde(default)]
    output_taps: Option<OutputTapsDoc>,
    #[serde(default)]
    monitor: Option<MonitorDoc>,
    #[serde(default)]
    rta: Option<RtaDoc>,
    #[serde(default)]
    output_eq: Option<OutputEqDoc>,
    #[serde(default)]
    link_settings: Option<LinkSettingsDoc>,
    #[serde(default)]
    native_processors: Option<openfader_core::native_processor::NativeProcessorSnapshot>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkSettingsDoc {
    provider: String,
    bus_links: [bool; 3],
    policies: [bool; 4],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputEqDoc {
    provider: String,
    targets: Vec<OutputEqTargetDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputEqTargetDoc {
    target: String,
    mode: u8,
    gains_db: Vec<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RtaDoc {
    provider: String,
    detector: u8,
    decay: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MonitorDoc {
    provider: String,
    values: Vec<MonitorValueDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MonitorValueDoc {
    id: String,
    value: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputTapsDoc {
    provider: String,
    values: Vec<OutputTapDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputTapDoc {
    target: String,
    source: String,
    tap: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutomixDoc {
    provider: String,
    values: [bool; 4],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MuteGroupsDoc {
    provider: String,
    masters: Vec<bool>,
    memberships: Vec<MuteGroupMembershipDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MuteGroupMembershipDoc {
    strip: u16,
    key: String,
    mask: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumberedStripDoc {
    strip: u16,
    #[serde(default)]
    key: Option<String>,
    name: String,
    fader: f64,
    mute: bool,
    #[serde(default)]
    params: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FxDoc {
    slot: u8,
    #[serde(rename = "type")]
    fx_type: String,
    #[serde(default = "default_true")]
    on: bool,
    #[serde(default)]
    bypass: bool,
    #[serde(default)]
    params: BTreeMap<String, f64>,
    #[serde(default)]
    insert: Option<FxInsertDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FxInsertDoc {
    provider: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteDoc {
    #[serde(default)]
    target: String,
    strip: u16,
    #[serde(default)]
    key: Option<String>,
    group: String,
    index: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShowDoc {
    name: String,
    #[serde(default)]
    scenes: Vec<NamedSceneDoc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedSceneDoc {
    name: String,
    #[serde(default)]
    main_fader: Option<f64>,
    #[serde(default)]
    strips: Vec<NamedStripDoc>,
    #[serde(default)]
    notes: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedStripDoc {
    strip: u16,
    name: String,
    fader: f64,
    mute: bool,
    #[serde(default)]
    params: BTreeMap<String, Value>,
}

fn default_true() -> bool {
    true
}

type ParsedLibrary = (HashMap<u16, StoredScene>, Vec<Show>, u64);

fn parse_library_doc(doc: &Value) -> Result<ParsedLibrary, String> {
    let obj = doc
        .as_object()
        .ok_or_else(|| "document must be a JSON object".to_string())?;
    let version = obj
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "document needs integer \"version\" 1..=10".to_string())?;
    if !matches!(
        version,
        LIBRARY_VERSION_V1
            | LIBRARY_VERSION_V2
            | LIBRARY_VERSION_V3
            | LIBRARY_VERSION_V4
            | LIBRARY_VERSION_V5
            | LIBRARY_VERSION_V6
            | LIBRARY_VERSION_V7
            | LIBRARY_VERSION_V8
            | LIBRARY_VERSION_V9
            | LIBRARY_VERSION_V10
            | LIBRARY_VERSION
    ) {
        return Err(format!(
            "unsupported library version {version} (engine reads v1..v{LIBRARY_VERSION})"
        ));
    }
    if version == LIBRARY_VERSION_V1 {
        for key in obj.keys() {
            if key != "version" && key != "scenes" {
                return Err(format!(
                    "unknown document field {key:?} (v1 holds version+scenes)"
                ));
            }
        }
        let scenes = obj
            .get("scenes")
            .and_then(Value::as_array)
            .ok_or_else(|| "v1 document needs a \"scenes\" array".to_string())?;
        if scenes.len() > MAX_NUMBERED_SCENES {
            return Err(format!(
                "too many numbered scenes ({} > {MAX_NUMBERED_SCENES})",
                scenes.len()
            ));
        }
        let numbered = parse_numbered(scenes, version)?;
        return Ok((numbered, Vec::new(), version));
    }
    for key in obj.keys() {
        if key != "version" && key != "scenes" && key != "shows" {
            return Err(format!(
                "unknown document field {key:?} (v2 holds version+scenes+shows)"
            ));
        }
    }
    let scenes = obj
        .get("scenes")
        .and_then(Value::as_array)
        .ok_or_else(|| "v2 document needs a \"scenes\" array".to_string())?;
    if scenes.len() > MAX_NUMBERED_SCENES {
        return Err(format!(
            "too many numbered scenes ({} > {MAX_NUMBERED_SCENES})",
            scenes.len()
        ));
    }
    let named = obj
        .get("shows")
        .and_then(Value::as_array)
        .ok_or_else(|| "v2 document needs a \"shows\" array".to_string())?;
    if named.len() > MAX_SHOWS {
        return Err(format!("too many shows ({} > {MAX_SHOWS})", named.len()));
    }
    let numbered = parse_numbered(scenes, version)?;
    let shows = parse_shows(named, version)?;
    Ok((numbered, shows, version))
}

fn parse_numbered(scenes: &[Value], version: u64) -> Result<HashMap<u16, StoredScene>, String> {
    let mut out = HashMap::new();
    for (i, raw) in scenes.iter().enumerate() {
        if version < LIBRARY_VERSION_V3
            && raw
                .as_object()
                .is_some_and(|scene| scene.contains_key("mute_groups"))
        {
            return Err(format!(
                "scenes[{i}]: mute groups require library version 3"
            ));
        }
        if version < LIBRARY_VERSION_V3
            && raw.get("fx").and_then(Value::as_array).is_some_and(|fx| {
                fx.iter().any(|row| {
                    row.as_object()
                        .is_some_and(|row| row.contains_key("insert"))
                })
            })
        {
            return Err(format!("scenes[{i}]: FX insert requires library version 3"));
        }
        if version < LIBRARY_VERSION && raw.as_object().is_some_and(|s| s.contains_key("native_processors")) {
            return Err(format!("scenes[{i}]: native processors require library version 11"));
        }
        let doc: NumberedSceneDoc =
            serde_json::from_value(raw.clone()).map_err(|e| format!("scenes[{i}]: {e}"))?;
        if !(SCENE_MIN..=SCENE_MAX).contains(&doc.number) {
            return Err(format!(
                "scenes[{i}]: number {} out of range {SCENE_MIN:03}..={SCENE_MAX:03}",
                doc.number
            ));
        }
        if out.contains_key(&doc.number) {
            return Err(format!(
                "scenes[{i}]: duplicate scene number {}",
                doc.number
            ));
        }
        if version < LIBRARY_VERSION_V4 && doc.automix.is_some() {
            return Err(format!("scenes[{i}]: automix requires library version 4"));
        }
        if version < LIBRARY_VERSION_V5
            && raw
                .as_object()
                .is_some_and(|s| s.contains_key("output_taps"))
        {
            return Err(format!(
                "scenes[{i}]: output taps require library version 5"
            ));
        }
        if version < LIBRARY_VERSION_V6
            && raw.as_object().is_some_and(|s| s.contains_key("monitor"))
        {
            return Err(format!("scenes[{i}]: monitor requires library version 6"));
        }
        if version < LIBRARY_VERSION_V7 && raw.as_object().is_some_and(|s| s.contains_key("rta")) {
            return Err(format!("scenes[{i}]: rta requires library version 7"));
        }
        if version < LIBRARY_VERSION_V8
            && raw.as_object().is_some_and(|s| s.contains_key("output_eq"))
        {
            return Err(format!("scenes[{i}]: output EQ requires library version 8"));
        }
        if version < LIBRARY_VERSION_V9
            && raw
                .as_object()
                .is_some_and(|s| s.contains_key("link_settings"))
        {
            return Err(format!(
                "scenes[{i}]: link settings require library version 9"
            ));
        }
        let link_settings = doc.link_settings.map(|link| StoredLinkSettings {
            provider: link.provider,
            settings: openfader_core::link_settings::StereoLinkSettings {
                bus_links: link.bus_links,
                policies: link.policies,
            },
        });
        if let Some(link) = &link_settings {
            validate_stored_link_settings(link, &format!("scenes[{i}].link_settings"))?;
        }
        if let Some(native) = &doc.native_processors {
            provider_library_validation::native_processors(native, &format!("scenes[{i}].native_processors"))?;
        }
        let output_eq = doc.output_eq.map(|eq| StoredOutputEq {
            provider: eq.provider,
            targets: eq
                .targets
                .into_iter()
                .map(|t| StoredOutputEqTarget {
                    target: t.target,
                    mode: t.mode,
                    gains_db: t.gains_db,
                })
                .collect(),
        });
        if let Some(eq) = &output_eq {
            validate_stored_output_eq(eq, &format!("scenes[{i}].output_eq"))?;
        }
        let rta = doc.rta.map(|r| StoredRta {
            provider: r.provider,
            detector: r.detector,
            decay: r.decay,
        });
        if let Some(r) = &rta {
            validate_stored_rta(r, &format!("scenes[{i}].rta"))?;
        }
        let monitor = doc.monitor.map(|m| StoredMonitor {
            provider: m.provider,
            values: m
                .values
                .into_iter()
                .map(|v| StoredMonitorValue {
                    id: v.id,
                    value: v.value,
                })
                .collect(),
        });
        if let Some(m) = &monitor {
            validate_stored_monitor(m, &format!("scenes[{i}].monitor"))?;
        }
        let output_taps = doc.output_taps.map(|t| StoredOutputTaps {
            provider: t.provider,
            values: t
                .values
                .into_iter()
                .map(|v| openfader_core::routing::OutputTapState {
                    target: v.target,
                    source: v.source,
                    tap: v.tap,
                })
                .collect(),
        });
        if let Some(t) = &output_taps {
            validate_output_taps(t, &format!("scenes[{i}].output_taps"))?;
        }
        let automix = doc.automix.as_ref().map(|a| StoredAutomix {
            provider: a.provider.clone(),
            values: a.values,
        });
        if let Some(a) = &automix {
            check_name(&a.provider, &format!("scenes[{i}].automix.provider"))?;
        }
        let name = check_name(&doc.name, &format!("scenes[{i}].name"))?;
        let mute_groups = doc.mute_groups.as_ref().map(|groups| StoredMuteGroups {
            provider: groups.provider.clone(),
            masters: groups.masters.clone(),
            memberships: groups
                .memberships
                .iter()
                .map(|member| StoredMuteGroupMembership {
                    strip: member.strip,
                    key: member.key.clone(),
                    mask: member.mask,
                })
                .collect(),
        });
        if let Some(groups) = &mute_groups {
            validate_mute_groups(groups, &format!("scenes[{i}].mute_groups"))?;
        }
        let main_fader = match doc.main_fader {
            Some(v) => Some(check_fader(v, &format!("scenes[{i}].main_fader"))?),
            None => None,
        };
        if doc.strips.len() > MAX_STRIPS_PER_SCENE {
            return Err(format!(
                "scenes[{i}]: too many strips ({} > {MAX_STRIPS_PER_SCENE})",
                doc.strips.len()
            ));
        }
        let mut seen_strips = std::collections::HashSet::new();
        let mut strips = Vec::with_capacity(doc.strips.len());
        for (j, st) in doc.strips.iter().enumerate() {
            let what = format!("scenes[{i}].strips[{j}]");
            if !seen_strips.insert(st.strip) {
                return Err(format!("{what}: duplicate strip {}", st.strip));
            }
            check_key(st.key.as_deref(), &format!("{what}.key"))?;
            check_name_len(&st.name, MAX_NAME_LEN, &format!("{what}.name"))?;
            let fader = check_fader(st.fader, &format!("{what}.fader"))?;
            let params = check_primitive_params(&st.params, &format!("{what}.params"))?;
            strips.push(StoredStrip {
                strip: st.strip,
                key: st.key.clone(),
                name: st.name.clone(),
                fader,
                mute: st.mute,
                params,
            });
        }
        if doc.fx.len() > MAX_FX_ROWS_PER_SCENE {
            return Err(format!(
                "scenes[{i}]: too many fx rows ({} > {MAX_FX_ROWS_PER_SCENE})",
                doc.fx.len()
            ));
        }
        let mut fx = Vec::with_capacity(doc.fx.len());
        for (j, f) in doc.fx.iter().enumerate() {
            let what = format!("scenes[{i}].fx[{j}]");
            if let Some(insert) = &f.insert {
                check_name(&insert.provider, &format!("{what}.insert.provider"))?;
                check_name_len(
                    &insert.provider,
                    MAX_FX_TYPE_LEN,
                    &format!("{what}.insert.provider"),
                )?;
            }
            if f.slot == 0 {
                return Err(format!("{what}: slot 0 invalid (slots start at 1)"));
            }
            check_name(&f.fx_type, &format!("{what}.type"))?;
            if f.fx_type.len() > MAX_FX_TYPE_LEN {
                return Err(format!(
                    "{what}.type too long ({} > {MAX_FX_TYPE_LEN})",
                    f.fx_type.len()
                ));
            }
            if f.params.len() > MAX_FX_PARAMS_PER_SLOT {
                return Err(format!(
                    "{what}: too many params ({} > {MAX_FX_PARAMS_PER_SLOT})",
                    f.params.len()
                ));
            }
            let mut params = BTreeMap::new();
            for (id, v) in &f.params {
                check_param_id(id, &what)?;
                params.insert(
                    id.clone(),
                    check_fader(*v, &format!("{what}.params[{id:?}]"))?,
                );
            }
            fx.push(StoredFx {
                slot: f.slot,
                fx_type: f.fx_type.clone(),
                on: f.on,
                bypass: f.bypass,
                params,
                insert: f.insert.as_ref().map(|insert| StoredFxInsert {
                    provider: insert.provider.clone(),
                    enabled: insert.enabled,
                }),
            });
        }
        if doc.routing.len() > MAX_ROUTES_PER_SCENE {
            return Err(format!(
                "scenes[{i}]: too many routing rows ({} > {MAX_ROUTES_PER_SCENE})",
                doc.routing.len()
            ));
        }
        let mut routing = Vec::with_capacity(doc.routing.len());
        for (j, r) in doc.routing.iter().enumerate() {
            let what = format!("scenes[{i}].routing[{j}]");
            check_key(r.key.as_deref(), &format!("{what}.key"))?;
            if !r.target.is_empty() {
                check_name(&r.target, &format!("{what}.target"))?;
            }
            if !r.group.is_empty() || r.index != 0 {
                check_name(&r.group, &format!("{what}.group"))?;
                if r.index == 0 {
                    return Err(format!(
                        "{what}.index must be positive for a patched source"
                    ));
                }
            }
            if r.group.len() > MAX_ROUTE_GROUP_LEN {
                return Err(format!(
                    "{what}.group too long ({} > {MAX_ROUTE_GROUP_LEN})",
                    r.group.len()
                ));
            }
            routing.push(StoredRoute {
                target: r.target.clone(),
                strip: r.strip,
                key: r.key.clone(),
                group: r.group.clone(),
                index: r.index,
            });
        }
        out.insert(
            doc.number,
            StoredScene {
                number: doc.number,
                name,
                saved_at: doc.saved_at,
                main_fader,
                strips,
                fx,
                routing,
                mute_groups,
                automix,
                output_taps,
                monitor,
                rta,
                output_eq,
                link_settings,
                native_processors: doc.native_processors,
            },
        );
    }
    Ok(out)
}

fn parse_shows(named: &[Value], version: u64) -> Result<Vec<Show>, String> {
    let mut out = Vec::with_capacity(named.len());
    let mut seen_shows = std::collections::HashSet::new();
    for (i, raw) in named.iter().enumerate() {
        let mut adapted = raw.clone();
        let mut full_scenes = HashMap::new();
        if let Some(rows) = adapted.get_mut("scenes").and_then(Value::as_array_mut) {
            for (j, row) in rows.iter_mut().enumerate() {
                if row.as_object().is_some_and(|o| o.contains_key("snapshot")) {
                    if version < LIBRARY_VERSION_V10 {
                        return Err(format!(
                            "shows[{i}].scenes[{j}]: snapshot requires version 10"
                        ));
                    }
                    let obj = row.as_object().unwrap();
                    if obj.len() != 2 || !obj.contains_key("name") {
                        return Err("full named scene must contain only name and snapshot".into());
                    }
                    let name = obj
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or("named scene needs name")?
                        .to_owned();
                    let mut snapshot = obj["snapshot"]
                        .as_object()
                        .ok_or("snapshot must be an object")?
                        .clone();
                    if snapshot.contains_key("number") || snapshot.contains_key("name") {
                        return Err("snapshot must not contain identity".into());
                    }
                    // Placeholder is used only by the shared numbered decoder.
                    snapshot.insert("number".into(), serde_json::json!(1));
                    snapshot.insert("name".into(), serde_json::json!(name));
                    let mut parsed = parse_numbered(&[Value::Object(snapshot)], version)?;
                    full_scenes.insert(j, parsed.remove(&1).unwrap());
                    *row = serde_json::json!({"name": name});
                }
            }
        }
        let doc: ShowDoc =
            serde_json::from_value(adapted).map_err(|e| format!("shows[{i}]: {e}"))?;
        let name = check_name(&doc.name, &format!("shows[{i}].name"))?;
        if !seen_shows.insert(name.clone()) {
            return Err(format!("shows[{i}]: duplicate show name {name:?}"));
        }
        if doc.scenes.len() > MAX_SCENES_PER_SHOW {
            return Err(format!(
                "shows[{i}]: too many scenes ({} > {MAX_SCENES_PER_SHOW})",
                doc.scenes.len()
            ));
        }
        let mut scenes = Vec::with_capacity(doc.scenes.len());
        let mut seen_scenes = std::collections::HashSet::new();
        for (j, sc) in doc.scenes.iter().enumerate() {
            let what = format!("shows[{i}].scenes[{j}]");
            let scene_name = check_name(&sc.name, &format!("{what}.name"))?;
            if !seen_scenes.insert(scene_name.clone()) {
                return Err(format!("{what}: duplicate scene name {scene_name:?}"));
            }
            if let Some(full) = full_scenes.remove(&j) {
                scenes.push(super::engine::StoredNamedScene::from_full(full));
                continue;
            }
            let main_fader = match sc.main_fader {
                Some(v) => Some(check_fader(v, &format!("{what}.main_fader"))?),
                None => None,
            };
            if sc.strips.len() > MAX_STRIPS_PER_SCENE {
                return Err(format!(
                    "{what}: too many strips ({} > {MAX_STRIPS_PER_SCENE})",
                    sc.strips.len()
                ));
            }
            let mut seen_strips = std::collections::HashSet::new();
            let mut strips = Vec::with_capacity(sc.strips.len());
            for (k, st) in sc.strips.iter().enumerate() {
                let swhat = format!("{what}.strips[{k}]");
                if !seen_strips.insert(st.strip) {
                    return Err(format!("{swhat}: duplicate strip {}", st.strip));
                }
                check_name_len(&st.name, MAX_NAME_LEN, &format!("{swhat}.name"))?;
                let fader = check_fader(st.fader, &format!("{swhat}.fader"))?;
                let params = check_named_params(&st.params, &swhat)?;
                strips.push(openfader_core::show::SceneStrip {
                    strip: StripId(st.strip),
                    name: st.name.clone(),
                    fader,
                    mute: st.mute,
                    params,
                });
            }
            if sc.notes.len() > MAX_NOTES_PER_SCENE {
                return Err(format!(
                    "{what}: too many notes ({} > {MAX_NOTES_PER_SCENE})",
                    sc.notes.len()
                ));
            }
            let mut notes = HashMap::with_capacity(sc.notes.len());
            for (key, value) in &sc.notes {
                if key.trim().is_empty() {
                    return Err(format!("{what}: note key must be non-empty"));
                }
                if key.len() > MAX_NOTE_KEY_LEN {
                    return Err(format!(
                        "{what}: note key too long ({} > {MAX_NOTE_KEY_LEN})",
                        key.len()
                    ));
                }
                if value.len() > MAX_NOTE_VALUE_LEN {
                    return Err(format!(
                        "{what}: note value too long ({} > {MAX_NOTE_VALUE_LEN})",
                        value.len()
                    ));
                }
                notes.insert(key.clone(), value.clone());
            }
            scenes.push(
                Scene {
                    name: scene_name,
                    strips,
                    main_fader,
                    notes,
                }
                .into(),
            );
        }
        out.push(Show { name, scenes });
    }
    Ok(out)
}

fn check_name(raw: &str, what: &str) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Err(format!("{what} must be non-empty"));
    }
    check_name_len(raw, MAX_NAME_LEN, what)?;
    Ok(raw.to_string())
}

fn check_name_len(raw: &str, max: usize, what: &str) -> Result<(), String> {
    if raw.len() > max {
        return Err(format!("{what} too long ({} > {max})", raw.len()));
    }
    Ok(())
}

fn check_key(key: Option<&str>, what: &str) -> Result<(), String> {
    if let Some(k) = key {
        if k.trim().is_empty() {
            return Err(format!("{what} must be null or a non-empty strip key"));
        }
        if k.len() > MAX_PARAM_ID_LEN {
            return Err(format!(
                "{what} too long ({} > {MAX_PARAM_ID_LEN})",
                k.len()
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_stored_link_settings(
    link: &StoredLinkSettings,
    what: &str,
) -> Result<(), String> {
    check_name(&link.provider, &format!("{what}.provider"))?;
    check_name_len(&link.provider, MAX_FX_TYPE_LEN, &format!("{what}.provider"))
}

pub(crate) fn validate_stored_output_eq(eq: &StoredOutputEq, what: &str) -> Result<(), String> {
    check_name(&eq.provider, &format!("{what}.provider"))?;
    check_name_len(&eq.provider, MAX_FX_TYPE_LEN, &format!("{what}.provider"))?;
    if eq.targets.is_empty() || eq.targets.len() > 128 {
        return Err(format!("{what}: invalid target count"));
    }
    let mut seen = std::collections::HashSet::new();
    for row in &eq.targets {
        check_name(&row.target, &format!("{what}.target"))?;
        check_name_len(&row.target, MAX_PARAM_ID_LEN, &format!("{what}.target"))?;
        if !seen.insert(row.target.as_str())
            || row.gains_db.is_empty()
            || row.gains_db.len() > 128
            || row.gains_db.iter().any(|g| !g.is_finite())
        {
            return Err(format!("{what}: invalid target or gains"));
        }
    }
    policy_for_provider(&eq.provider).output_eq(eq, what)?;
    Ok(())
}

pub(crate) fn validate_stored_rta(rta: &StoredRta, what: &str) -> Result<(), String> {
    check_name(&rta.provider, &format!("{what}.provider"))?;
    check_name_len(&rta.provider, MAX_FX_TYPE_LEN, &format!("{what}.provider"))?;
    if !rta.decay.is_finite() {
        return Err(format!("{what}: invalid RTA settings"));
    }
    policy_for_provider(&rta.provider).rta(rta, what)
}

pub(crate) fn validate_stored_monitor(monitor: &StoredMonitor, what: &str) -> Result<(), String> {
    check_name(&monitor.provider, &format!("{what}.provider"))?;
    check_name_len(
        &monitor.provider,
        MAX_FX_TYPE_LEN,
        &format!("{what}.provider"),
    )?;
    if monitor.values.is_empty() || monitor.values.len() > 128 {
        return Err(format!("{what}: monitor roster out of range"));
    }
    let mut ids = std::collections::HashSet::new();
    for row in &monitor.values {
        check_name(&row.id, &format!("{what}.id"))?;
        check_name_len(&row.id, MAX_PARAM_ID_LEN, &format!("{what}.id"))?;
        if !ids.insert(&row.id) || !row.value.is_finite() {
            return Err(format!("{what}: duplicate ID or nonfinite value"));
        }
    }
    policy_for_provider(&monitor.provider).monitor(monitor, what)?;
    Ok(())
}

fn validate_output_taps(taps: &StoredOutputTaps, what: &str) -> Result<(), String> {
    check_name(&taps.provider, &format!("{what}.provider"))?;
    check_name_len(&taps.provider, MAX_FX_TYPE_LEN, &format!("{what}.provider"))?;
    if taps.values.len() > MAX_ROUTES_PER_SCENE {
        return Err(format!("{what}: too many values"));
    }
    let mut targets = std::collections::HashSet::new();
    for value in &taps.values {
        check_name(&value.target, &format!("{what}.target"))?;
        check_name(&value.source, &format!("{what}.source"))?;
        if !targets.insert(&value.target) {
            return Err(format!("{what}: duplicate target"));
        }
    }
    policy_for_provider(&taps.provider).output_taps(taps, what)
}

fn validate_mute_groups(groups: &StoredMuteGroups, what: &str) -> Result<(), String> {
    check_name(&groups.provider, &format!("{what}.provider"))?;
    check_name_len(
        &groups.provider,
        MAX_FX_TYPE_LEN,
        &format!("{what}.provider"),
    )?;
    let count = groups.masters.len();
    if count == 0 || count > 32 {
        return Err(format!("{what}: group count must be 1..=32"));
    }
    if groups.memberships.len() > MAX_STRIPS_PER_SCENE {
        return Err(format!("{what}: too many memberships"));
    }
    let policy = policy_for_provider(&groups.provider);
    policy.mute_groups(groups, what)?;
    let max_mask = if count == 32 {
        u32::MAX
    } else {
        (1u32 << count) - 1
    };
    let mut strips = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    for member in &groups.memberships {
        check_key(Some(&member.key), &format!("{what}.key"))?;
        if openfader_core::parse_strip_key(&member.key).is_none()
            && !policy.accepts_extra_strip_key(&member.key)
        {
            return Err(format!("{what}: invalid strip key {:?}", member.key));
        }
        if !strips.insert(member.strip) || !keys.insert(&member.key) {
            return Err(format!("{what}: duplicate strip or key"));
        }
        if member.mask > max_mask {
            return Err(format!("{what}: membership mask out of range"));
        }
    }
    Ok(())
}

fn check_fader(v: f64, what: &str) -> Result<f32, String> {
    let f = v as f32;
    if !f.is_finite() {
        return Err(format!("{what} must be a finite number"));
    }
    if !(0.0..=1.0).contains(&f) {
        return Err(format!("{what} {f} out of range 0.0..=1.0"));
    }
    Ok(f)
}

fn check_param_id(id: &str, what: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err(format!("{what}: param id must be non-empty"));
    }
    if id.len() > MAX_PARAM_ID_LEN {
        return Err(format!(
            "{what}: param id too long ({} > {MAX_PARAM_ID_LEN})",
            id.len()
        ));
    }
    Ok(())
}

/// Structural primitive validation only: numbers must be finite, bools and
/// (length-capped) strings pass. Provider param IDs are never dropped for
/// belonging to another driver; recall validates against the destination
/// driver. Raw values are preserved verbatim.
fn check_primitive_params(
    params: &BTreeMap<String, Value>,
    what: &str,
) -> Result<HashMap<String, Value>, String> {
    if params.len() > MAX_PARAMS_PER_STRIP {
        return Err(format!(
            "{what}: too many params ({} > {MAX_PARAMS_PER_STRIP})",
            params.len()
        ));
    }
    let mut out = HashMap::with_capacity(params.len());
    for (id, v) in params {
        check_param_id(id, what)?;
        check_primitive_value(v, &format!("{what}[{id:?}]"))?;
        out.insert(id.clone(), v.clone());
    }
    Ok(out)
}

fn check_primitive_value(v: &Value, what: &str) -> Result<(), String> {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if !f.is_finite() {
                return Err(format!("{what} must be a finite number"));
            }
            Ok(())
        }
        Value::Bool(_) => Ok(()),
        Value::String(s) => {
            if s.len() > MAX_PARAM_STRING_LEN {
                return Err(format!(
                    "{what} too long ({} > {MAX_PARAM_STRING_LEN})",
                    s.len()
                ));
            }
            Ok(())
        }
        _ => Err(format!("{what} must be number|bool|string")),
    }
}

/// Named-show params convert to [`ParamValue`] (numbers narrow to f32 with
/// a finiteness check; engineering units keep their raw scale — only the
/// strip `fader`/`main_fader` leaves are normalized 0.0..=1.0).
fn check_named_params(
    params: &BTreeMap<String, Value>,
    what: &str,
) -> Result<HashMap<String, ParamValue>, String> {
    if params.len() > MAX_PARAMS_PER_STRIP {
        return Err(format!(
            "{what}: too many params ({} > {MAX_PARAMS_PER_STRIP})",
            params.len()
        ));
    }
    let mut out = HashMap::with_capacity(params.len());
    for (id, v) in params {
        check_param_id(id, what)?;
        let pv = match v {
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(f64::NAN) as f32;
                if !f.is_finite() {
                    return Err(format!("{what}[{id:?}] must be a finite number"));
                }
                ParamValue::Float(f)
            }
            Value::Bool(b) => ParamValue::Bool(*b),
            Value::String(s) => {
                if s.len() > MAX_PARAM_STRING_LEN {
                    return Err(format!(
                        "{what}[{id:?}] too long ({} > {MAX_PARAM_STRING_LEN})",
                        s.len()
                    ));
                }
                ParamValue::Text(s.clone())
            }
            _ => return Err(format!("{what}[{id:?}] must be number|bool|string")),
        };
        out.insert(id.clone(), pv);
    }
    Ok(out)
}

// ================= candidate validation + rendering =================

/// Validate a commit candidate built from live memory (same strictness as
/// load, minus file-shape concerns). Runs before every durable write so a
/// corrupt in-memory state can never reach the disk.
pub fn validate_candidate(
    numbered: &HashMap<u16, StoredScene>,
    shows: &[Show],
) -> Result<(), String> {
    if numbered.len() > MAX_NUMBERED_SCENES {
        return Err(format!(
            "too many numbered scenes ({} > {MAX_NUMBERED_SCENES})",
            numbered.len()
        ));
    }
    for (n, s) in numbered {
        let what = format!("scenes[{n:03}]");
        if !(SCENE_MIN..=SCENE_MAX).contains(n) {
            return Err(format!("{what}: number out of range"));
        }
        if s.number != *n {
            return Err(format!("{what}: key/number mismatch"));
        }
        check_name(&s.name, &format!("{what}.name"))?;
        if let Some(native) = &s.native_processors {
            provider_library_validation::native_processors(native, &format!("{what}.native_processors"))?;
        }
        if let Some(link) = &s.link_settings {
            validate_stored_link_settings(link, &format!("{what}.link_settings"))?;
        }
        if let Some(eq) = &s.output_eq {
            validate_stored_output_eq(eq, &format!("{what}.output_eq"))?;
        }
        if let Some(r) = &s.rta {
            validate_stored_rta(r, &format!("{what}.rta"))?;
        }
        if let Some(m) = &s.monitor {
            validate_stored_monitor(m, &format!("{what}.monitor"))?;
        }
        if let Some(t) = &s.output_taps {
            validate_output_taps(t, &format!("{what}.output_taps"))?;
        }
        if let Some(a) = &s.automix {
            check_name(&a.provider, &format!("{what}.automix.provider"))?;
        }
        if let Some(groups) = &s.mute_groups {
            validate_mute_groups(groups, &format!("{what}.mute_groups"))?;
        }
        if let Some(m) = s.main_fader {
            if !m.is_finite() || !(0.0..=1.0).contains(&m) {
                return Err(format!("{what}.main_fader out of range 0.0..=1.0"));
            }
        }
        if s.strips.len() > MAX_STRIPS_PER_SCENE {
            return Err(format!("{what}: too many strips"));
        }
        let mut seen = std::collections::HashSet::new();
        for st in &s.strips {
            if !seen.insert(st.strip) {
                return Err(format!("{what}: duplicate strip {}", st.strip));
            }
            check_key(st.key.as_deref(), &format!("{what}.strip key"))?;
            check_name_len(&st.name, MAX_NAME_LEN, &format!("{what}.strip name"))?;
            if !st.fader.is_finite() || !(0.0..=1.0).contains(&st.fader) {
                return Err(format!("{what}: strip {} fader out of range", st.strip));
            }
            if st.params.len() > MAX_PARAMS_PER_STRIP {
                return Err(format!("{what}: too many params"));
            }
            for (id, v) in &st.params {
                check_param_id(id, &what)?;
                check_primitive_value(v, &format!("{what}[{id:?}]"))?;
            }
        }
        if s.fx.len() > MAX_FX_ROWS_PER_SCENE {
            return Err(format!("{what}: too many fx rows"));
        }
        for f in &s.fx {
            if f.slot == 0 {
                return Err(format!("{what}: fx slot 0 invalid"));
            }
            check_name(&f.fx_type, &format!("{what}.fx type"))?;
            if let Some(insert) = &f.insert {
                check_name_len(
                    &insert.provider,
                    MAX_FX_TYPE_LEN,
                    &format!("{what}.fx insert provider"),
                )?;
                check_name(&insert.provider, &format!("{what}.fx insert provider"))?;
            }
            if f.params.len() > MAX_FX_PARAMS_PER_SLOT {
                return Err(format!("{what}: too many fx params"));
            }
            for (id, v) in &f.params {
                check_param_id(id, &what)?;
                if !v.is_finite() || !(0.0..=1.0).contains(v) {
                    return Err(format!("{what}: fx param {id:?} out of range"));
                }
            }
        }
        if s.routing.len() > MAX_ROUTES_PER_SCENE {
            return Err(format!("{what}: too many routing rows"));
        }
        for r in &s.routing {
            check_key(r.key.as_deref(), &format!("{what}.routing key"))?;
            if !r.target.is_empty() {
                check_name(&r.target, &format!("{what}.routing target"))?;
            }
            if !r.group.is_empty() || r.index != 0 {
                check_name(&r.group, &format!("{what}.routing group"))?;
                if r.index == 0 {
                    return Err(format!("{what}: patched source index must be positive"));
                }
            }
        }
    }
    if shows.len() > MAX_SHOWS {
        return Err(format!("too many shows ({} > {MAX_SHOWS})", shows.len()));
    }
    let mut seen_shows = std::collections::HashSet::new();
    for show in shows {
        let sname = check_name(&show.name, "show name")?;
        if !seen_shows.insert(sname) {
            return Err(format!("duplicate show name {:?}", show.name));
        }
        if show.scenes.len() > MAX_SCENES_PER_SHOW {
            return Err(format!("show {:?}: too many scenes", show.name));
        }
        let mut seen_scenes = std::collections::HashSet::new();
        for scene in &show.scenes {
            let cname = check_name(&scene.name, "scene name")?;
            if !seen_scenes.insert(cname) {
                return Err(format!(
                    "show {:?}: duplicate scene name {:?}",
                    show.name, scene.name
                ));
            }
            if let Some(m) = scene.main_fader {
                if !m.is_finite() || !(0.0..=1.0).contains(&m) {
                    return Err(format!(
                        "show {:?} scene {:?}: main_fader out of range",
                        show.name, scene.name
                    ));
                }
            }
            if let Some(full) = scene.full() {
                let mut one = HashMap::new();
                one.insert(1, full.clone());
                validate_candidate(&one, &[])?;
                // Also exercise the strict decoder used on reload.
                parse_numbered(&[render_numbered(full)], LIBRARY_VERSION)?;
            }
            if scene.strips.len() > MAX_STRIPS_PER_SCENE {
                return Err(format!(
                    "show {:?} scene {:?}: too many strips",
                    show.name, scene.name
                ));
            }
            let mut seen = std::collections::HashSet::new();
            for st in &scene.strips {
                if !seen.insert(st.strip.0) {
                    return Err(format!(
                        "show {:?} scene {:?}: duplicate strip {}",
                        show.name, scene.name, st.strip.0
                    ));
                }
                check_name_len(&st.name, MAX_NAME_LEN, "strip name")?;
                if !st.fader.is_finite() || !(0.0..=1.0).contains(&st.fader) {
                    return Err(format!(
                        "show {:?} scene {:?}: strip {} fader out of range",
                        show.name, scene.name, st.strip.0
                    ));
                }
                if st.params.len() > MAX_PARAMS_PER_STRIP {
                    return Err(format!(
                        "show {:?} scene {:?}: too many params",
                        show.name, scene.name
                    ));
                }
                for id in st.params.keys() {
                    check_param_id(id, "scene param")?;
                }
                for pv in st.params.values() {
                    if let ParamValue::Float(f) = pv {
                        if !f.is_finite() {
                            return Err(format!(
                                "show {:?} scene {:?}: non-finite param",
                                show.name, scene.name
                            ));
                        }
                    }
                }
            }
            if scene.notes.len() > MAX_NOTES_PER_SCENE {
                return Err(format!(
                    "show {:?} scene {:?}: too many notes",
                    show.name, scene.name
                ));
            }
        }
    }
    Ok(())
}

fn param_value_json(pv: &ParamValue) -> Value {
    match pv {
        ParamValue::Float(v) => serde_json::json!(v),
        ParamValue::Bool(b) => serde_json::json!(b),
        ParamValue::Text(s) => serde_json::json!(s),
    }
}

/// Render v10 for full named snapshots, v9 for link settings, v8 for output EQ, v7 for RTA settings, v6 for monitor settings, v5 for output taps, v4 for automix, v3 for FX insert or mute-group state, otherwise retain v2.
/// Numbered slots and parameter keys are sorted for stable bytes.
pub fn build_library_doc(numbered: &HashMap<u16, StoredScene>, shows: &[Show]) -> Value {
    let mut slots: Vec<&StoredScene> = numbered.values().collect();
    slots.sort_by_key(|s| s.number);
    let has_native = slots.iter().any(|s| s.native_processors.is_some()) || shows.iter().any(|show|
        show.scenes.iter().any(|s| s.full().is_some_and(|f| f.native_processors.is_some())));
    let version = if has_native {
        LIBRARY_VERSION
    } else if shows
        .iter()
        .any(|show| show.scenes.iter().any(|s| s.full().is_some()))
    {
        LIBRARY_VERSION_V10
    } else if slots.iter().any(|s| s.link_settings.is_some()) {
        LIBRARY_VERSION_V9
    } else if slots.iter().any(|s| s.output_eq.is_some()) {
        LIBRARY_VERSION_V8
    } else if slots.iter().any(|s| s.rta.is_some()) {
        LIBRARY_VERSION_V7
    } else if slots.iter().any(|s| s.monitor.is_some()) {
        LIBRARY_VERSION_V6
    } else if slots.iter().any(|s| s.output_taps.is_some()) {
        LIBRARY_VERSION_V5
    } else if slots.iter().any(|s| s.automix.is_some()) {
        LIBRARY_VERSION_V4
    } else if slots
        .iter()
        .any(|s| s.mute_groups.is_some() || s.fx.iter().any(|f| f.insert.is_some()))
    {
        LIBRARY_VERSION_V3
    } else {
        LIBRARY_VERSION_V2
    };
    let scenes: Vec<Value> = slots.iter().map(|s| render_numbered(s)).collect();
    let show_docs: Vec<Value> = shows
        .iter()
        .map(|show| {
            serde_json::json!({
                "name": show.name,
                "scenes": show.scenes.iter().map(|scene| {
                    if let Some(full) = scene.full() {
                        let mut snapshot = render_numbered(full);
                        let obj = snapshot.as_object_mut().unwrap();
                        obj.remove("number"); obj.remove("name");
                        return serde_json::json!({"name": scene.name, "snapshot": snapshot});
                    }
                    serde_json::json!({
                    "name": scene.name,
                    "main_fader": scene.main_fader,
                    "strips": scene.strips.iter().map(|st| {
                        let mut params = BTreeMap::new();
                        for (id, pv) in &st.params { params.insert(id.clone(), param_value_json(pv)); }
                        serde_json::json!({"strip": st.strip.0, "name": st.name, "fader": st.fader, "mute": st.mute, "params": params})
                    }).collect::<Vec<_>>(),
                    "notes": scene.notes,
                }) }).collect::<Vec<_>>(),
            })
        }).collect();
    serde_json::json!({ "version": version, "scenes": scenes, "shows": show_docs })
}

fn render_numbered(s: &StoredScene) -> Value {
    fn params_sorted(p: &HashMap<String, Value>) -> BTreeMap<&String, &Value> {
        p.iter().collect()
    }
    let mut scene = serde_json::json!({
        "number": s.number,
        "name": s.name,
        "saved_at": s.saved_at,
        "main_fader": s.main_fader,
        "strips": s.strips.iter().map(|st| serde_json::json!({
            "strip": st.strip,
            "key": st.key,
            "name": st.name,
            "fader": st.fader,
            "mute": st.mute,
            "params": params_sorted(&st.params),
        })).collect::<Vec<_>>(),
        "fx": s.fx.iter().map(|f| {
            let mut row = serde_json::json!({
                "slot": f.slot,
                "type": f.fx_type,
                "on": f.on,
                "bypass": f.bypass,
                "params": f.params,
            });
            if let Some(insert) = &f.insert {
                row["insert"] = serde_json::json!({"provider":insert.provider,"enabled":insert.enabled});
            }
            row
        }).collect::<Vec<_>>(),
        "routing": s.routing.iter().map(|r| {
            let mut row = serde_json::json!({
                "strip": r.strip, "key": r.key, "group": r.group, "index": r.index,
            });
            if !r.target.is_empty() { row["target"] = serde_json::json!(r.target); }
            row
        }).collect::<Vec<_>>(),
    });
    if let Some(native) = &s.native_processors {
        scene["native_processors"] = serde_json::to_value(native).expect("native processors serialize");
    }
    if let Some(link) = &s.link_settings {
        scene["link_settings"] = serde_json::json!({"provider": link.provider, "bus_links": link.settings.bus_links, "policies": link.settings.policies});
    }
    if let Some(eq) = &s.output_eq {
        scene["output_eq"] = serde_json::json!({"provider":eq.provider,"targets":eq.targets.iter().map(|t| serde_json::json!({"target":t.target,"mode":t.mode,"gains_db":t.gains_db})).collect::<Vec<_>>()});
    }
    if let Some(r) = &s.rta {
        scene["rta"] =
            serde_json::json!({"provider":r.provider,"detector":r.detector,"decay":r.decay});
    }
    if let Some(m) = &s.monitor {
        scene["monitor"] = serde_json::json!({"provider":m.provider,"values":m.values.iter().map(|v|serde_json::json!({"id":v.id,"value":v.value})).collect::<Vec<_>>()});
    }
    if let Some(t) = &s.output_taps {
        scene["output_taps"] = serde_json::json!({"provider": t.provider, "values": t.values.iter().map(|v| serde_json::json!({"target": v.target, "source": v.source, "tap": v.tap})).collect::<Vec<_>>()});
    }
    if let Some(a) = &s.automix {
        scene["automix"] = serde_json::json!({"provider": a.provider, "values": a.values});
    }
    if let Some(groups) = &s.mute_groups {
        scene["mute_groups"] = serde_json::json!({
            "provider": groups.provider,
            "masters": groups.masters,
            "memberships": groups.memberships.iter().map(|member| serde_json::json!({
                "strip": member.strip, "key": member.key, "mask": member.mask,
            })).collect::<Vec<_>>(),
        });
    }
    scene
}

// ================= atomic file commit =================

/// Post-commit file outcome. `Uncertain` means the rename landed but the
/// directory sync or the size stat failed: memory was still swapped (it
/// matches the disk content) yet durability is unproven, so the latch
/// blocks further writes until reset or restart.
pub enum CommitResult {
    Committed { bytes: u64 },
    Uncertain { bytes: Option<u64>, error: String },
}

/// Atomic durable commit of a validated candidate (device mode only;
/// memory mode swaps without I/O). Pre-replace failures return `Err` with
/// disk and memory untouched.
pub fn commit_file(
    data_dir: &Path,
    numbered: &HashMap<u16, StoredScene>,
    shows: &[Show],
    temp_id: u64,
) -> Result<CommitResult, String> {
    validate_candidate(numbered, shows)?;
    let doc = build_library_doc(numbered, shows);
    let bytes =
        serde_json::to_vec_pretty(&doc).map_err(|e| format!("library serialize failed: {e}"))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "library candidate too large ({} bytes > {MAX_FILE_BYTES})",
            bytes.len()
        ));
    }
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        return Err(format!(
            "library dir {} not creatable: {e}",
            data_dir.display()
        ));
    }
    let target = data_dir.join(LIBRARY_FILE_NAME);
    let mut temp_path: Option<PathBuf> = None;
    for attempt in 0..8u64 {
        let candidate = data_dir.join(format!(
            ".{LIBRARY_FILE_NAME}.tmp-{}-{temp_id}-{attempt}",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => {
                temp_path = Some(candidate);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "library temp {} not creatable: {e}",
                    candidate.display()
                ));
            }
        }
    }
    let Some(temp) = temp_path else {
        return Err("library temp file not creatable (name collision)".to_string());
    };
    let remove_temp = |temp: &Path| {
        if let Err(e) = std::fs::remove_file(temp) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return format!("; temp cleanup {} failed: {e}", temp.display());
            }
        }
        String::new()
    };
    if let Err(e) = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new().write(true).open(&temp)?;
        f.write_all(&bytes)?;
        f.flush()?;
        f.sync_all()?;
        Ok(())
    })() {
        let tail = remove_temp(&temp);
        return Err(format!(
            "library temp write {} failed: {e}{tail}",
            temp.display()
        ));
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        let tail = remove_temp(&temp);
        return Err(format!(
            "library rename {} -> {} failed: {e}{tail}",
            temp.display(),
            target.display()
        ));
    }
    if let Err(e) = sync_dir(data_dir) {
        return Ok(CommitResult::Uncertain {
            bytes: None,
            error: format!("library dir sync {} failed: {e}", data_dir.display()),
        });
    }
    match std::fs::metadata(&target).map(|m| m.len()) {
        Ok(len) => Ok(CommitResult::Committed { bytes: len }),
        Err(e) => Ok(CommitResult::Uncertain {
            bytes: None,
            error: format!("library stat {} failed: {e}", target.display()),
        }),
    }
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    let f = std::fs::File::open(dir)?;
    f.sync_all()
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}
