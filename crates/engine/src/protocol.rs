//! [`Cmd`] in, [`Event`] out: the single protocol every adapter speaks.
//!
//! The JSON is byte-compatible with the `openmix-serve` WebSocket protocol
//! (see `crates/engine/README.md`): same `cmd`/`type` tags, same field
//! names, same shapes. Existing clients must not notice the move.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn default_driver() -> String {
    "sim".to_string()
}

/// Client -> engine command.
///
/// Deserializes both `{"cmd":"focus","ch":N}` and
/// `{"cmd":"focus","chs":[...]}` into [`Cmd::Focus`]; `set_main_fader` is
/// accepted as an alias of `main_fader` (the Tauri command name).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
    Connect {
        #[serde(default = "default_driver")]
        driver: String,
        addr: Option<String>,
    },
    Fader {
        ch: u16,
        value: f32,
        /// Stable strip key (`ch/01`, `bus/01`, …, `main`): accepted on
        /// every strip-targeted command and authoritative when it resolves
        /// against the active desk topology (desk-switch survival). `ch`
        /// stays required for wire compat; old clients never send `key`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    #[serde(alias = "set_main_fader")]
    MainFader {
        value: f32,
    },
    /// R2 Main LR master mute (`{"cmd":"main_mute","muted":true}`): all
    /// drivers with a main master (sim in-memory, X-Air `/lr/mix/on`
    /// rig-verified, DL32S native). Honest `unsupported` elsewhere.
    MainMute {
        muted: bool,
    },
    Mute {
        ch: u16,
        muted: bool,
        /// Stable strip key (see [`Cmd::Fader`]): authoritative when it
        /// resolves; `ch` stays required for wire compat.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 per-strip solo (`{"cmd":"solo","ch":0,"soloed":true}`, monitor
    /// bus only — never touches the FOH mix). Sim in-memory; X-Air
    /// `/-stat/solosw/NN` 1-based for channel strips (write + bank
    /// readback + clear live-verified XR18 FW 1.20 2026-10-18); honest
    /// `unsupported` elsewhere.
    Solo {
        ch: u16,
        soloed: bool,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    Param {
        ch: u16,
        field: String,
        value: serde_json::Value,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    Focus {
        ch: Option<u16>,
        chs: Option<Vec<serde_json::Value>>,
    },
    Lock {
        ch: u16,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    Unlock {
        ch: u16,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    ListShows,
    CaptureScene {
        show: String,
        name: String,
    },
    RecallScene {
        show: String,
        name: String,
        #[serde(default)]
        fade_ms: Option<u64>,
    },
    DeleteScene {
        show: String,
        name: String,
    },
    /// R2 devices/connection: `{"cmd":"scan"}` re-runs discovery and
    /// broadcasts a fresh [`Event::Devices`] frame to every client.
    Scan,
    /// R2: connect one discovered device (`{"cmd":"connect_device",
    /// "id":"mon"}`). On sim this re-links the local model; on hardware
    /// drivers it dials the device address (UNVERIFIED-ON-RIG).
    ConnectDevice {
        id: String,
    },
    /// R2: verify one desk by IP without switching the link
    /// (`{"cmd":"probe","addr":"192.168.40.123"}`): directed unicast
    /// `/xinfo` (cross-subnet desks never answer broadcast discovery, but
    /// they DO answer unicast). On a reply the desk is upserted into the
    /// device list (`via: "manual"`) and a fresh [`Event::Devices`] frame
    /// goes out; silence is an honest error naming the address. Never
    /// touches the current link. Additive: older clients never send it.
    Probe {
        addr: String,
    },
    /// R2: drop the desk link (`{"cmd":"disconnect"}`); the engine keeps
    /// serving the last model read-only until the next connect.
    Disconnect,
    /// R2: request the [`Event::Info`] identity block (also sent on every
    /// connect).
    Info,
    /// R2 numbered scene save (`{"cmd":"scene_save","n":1,"name":"Set 1"}`).
    /// Captures the FULL param-plane cache (every catalogue value of every
    /// strip + main), not just fader/mute.
    SceneSave {
        n: u16,
        name: String,
    },
    /// R2 numbered scene recall (`{"cmd":"scene_recall","n":1,
    /// "fade_ms":400}`). Faders ride the existing fade; every other param
    /// is applied through `set_param` in catalogue order. The optional
    /// `scope` (`{"strips":[0,1],"groups":["eq","mute"],
    /// "globals":["main"]}`) restricts the recall to those strips /
    /// param groups / globals (`None` = full plane).
    SceneRecall {
        n: u16,
        #[serde(default)]
        fade_ms: Option<u64>,
        #[serde(default)]
        scope: Option<crate::engine::RecallScope>,
    },
    /// R2: rename a numbered scene slot.
    SceneRename {
        n: u16,
        name: String,
    },
    /// R2: delete one numbered scene slot.
    SceneDelete {
        n: u16,
    },
    /// R2: delete the whole numbered scene library.
    SceneClear,
    /// R2: switch an FX slot's design type
    /// (`{"cmd":"fx_set_type","slot":1,"type":"Hall"}`).
    FxSetType {
        slot: u8,
        #[serde(rename = "type", alias = "fx_type")]
        fx_type: String,
    },
    /// R2: set one normalized FX param by id
    /// (`{"cmd":"fx_param","slot":1,"id":"mix","value":0.8}`).
    FxParam {
        slot: u8,
        id: String,
        value: f32,
    },
    /// R2: bypass switch per FX slot.
    FxBypass {
        slot: u8,
        bypass: bool,
    },
    /// R2: on switch per FX slot.
    FxOn {
        slot: u8,
        on: bool,
    },
    /// R2: patch an input strip to a source socket
    /// (`{"cmd":"patch_set","strip":0,"group":"Lcl","index":1}`).
    PatchSet {
        strip: u16,
        group: String,
        index: u8,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2: clear one strip's patch (source reads back unpatched).
    PatchClear {
        strip: u16,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2: switch to offline editing on the named driver model
    /// (`{"cmd":"offline","model":"xair"}`): the engine runs the sim model
    /// keyed to that desk's topology and records every write as a change.
    Offline {
        model: String,
    },
    /// R2: push the recorded offline change list to the newly connected
    /// desk (`{"cmd":"offline_push","confirm":true}`).
    OfflinePush {
        #[serde(default)]
        confirm: bool,
    },
    /// R2: discard the recorded offline change list.
    OfflineDiscard,
    /// Realtime negotiation (MR1): `{"cmd":"hello","binary_meters":true}`.
    /// Stores the per-client binary-meter preference and unicasts a
    /// [`Event::Hello`] ack. Missing `binary_meters` just re-acks the
    /// current preference (default off: JSON `meters`/`gr`/`spectrum`).
    /// R2 adds `"taps":true`: the client additionally wants per-strip meter
    /// taps (JSON `taps` frames, or WS BINARY type 3 when negotiated).
    Hello {
        binary_meters: Option<bool>,
        #[serde(default)]
        taps: Option<bool>,
    },
    /// R2 mix-bus master fader (`{"cmd":"bus_fader","bus":0,"value":0.75}`):
    /// 0-based bus over `0..bus_count()` (sim/XR18 bus strips, DL32S aux
    /// masters). Honest `unsupported` where the driver has no bus masters.
    BusFader {
        bus: u8,
        value: f32,
    },
    /// R2 mix-bus master mute (`{"cmd":"bus_mute","bus":0,"muted":true}`).
    BusMute {
        bus: u8,
        muted: bool,
    },
    /// R2 subgroup master fader (`{"cmd":"subgroup_fader","group":0,
    /// "value":0.75}`): 0-based over `0..subgroup_count()` (sim in-memory,
    /// DL32S native; XR18 has no subgroups — honest `unsupported`).
    SubgroupFader {
        group: u8,
        value: f32,
    },
    /// R2 subgroup master mute.
    SubgroupMute {
        group: u8,
        muted: bool,
    },
    /// R2 channel -> subgroup membership (`{"cmd":"subgroup_assign","ch":0,
    /// "group":0,"member":true}`).
    SubgroupAssign {
        ch: u16,
        group: u8,
        member: bool,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 mute-group master mute (`{"cmd":"mute_group","group":0,
    /// "muted":true}`): 0-based over `0..mute_group_count()` (sim
    /// in-memory, DL32S native, X-Air `/config/mute/1..4` masters on XR18
    /// FW 1.20 — assign path stays `unsupported` there, no desk leaf).
    MuteGroup {
        group: u8,
        muted: bool,
    },
    /// R2 channel -> mute-group membership.
    MuteGroupAssign {
        ch: u16,
        group: u8,
        member: bool,
        /// Stable strip key (see [`Cmd::Fader`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 desk file download (`{"cmd":"file_download",
    /// "filename":"Show.zip"}`): replies with [`Event::File`] carrying
    /// base64 `data` (DL32S native, sim in-memory; XR18: unsupported).
    FileDownload {
        filename: String,
    },
    /// R2 desk file upload (`{"cmd":"file_upload","filename":"Show.zip",
    /// "data":"<base64>","file_type":0}`): replies with an [`Event::File`]
    /// ack (`data` absent, `size` = bytes written).
    FileUpload {
        filename: String,
        data: String,
        file_type: u32,
    },
    /// R2: clear all soloed channels on the desk (fire-and-forget, no echo).
    /// X-Air `/-action/clearsolo` (rig-verified answering on XR18 FW 1.20);
    /// other drivers: honest `unsupported`.
    ClearSolo,
}

impl Cmd {
    /// The `cmd` tag string, for unknown-command diagnostics.
    pub fn known_tags() -> &'static [&'static str] {
        &[
            "connect",
            "fader",
            "main_fader",
            "set_main_fader",
            "main_mute",
            "mute",
            "solo",
            "param",
            "focus",
            "lock",
            "unlock",
            "list_shows",
            "capture_scene",
            "recall_scene",
            "delete_scene",
            "hello",
            "scan",
            "connect_device",
            "probe",
            "disconnect",
            "info",
            "scene_save",
            "scene_recall",
            "scene_rename",
            "scene_delete",
            "scene_clear",
            "fx_set_type",
            "fx_param",
            "fx_bypass",
            "fx_on",
            "patch_set",
            "patch_clear",
            "offline",
            "offline_push",
            "offline_discard",
            "bus_fader",
            "bus_mute",
            "subgroup_fader",
            "subgroup_mute",
            "subgroup_assign",
            "mute_group",
            "mute_group_assign",
            "file_download",
            "file_upload",
            "clear_solo",
        ]
    }
}

/// One mixer strip in a [`Event::State`] frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateStrip {
    pub ch: u16,
    /// Stable strip key (`ch/01`, …, `main`): `None` on old frames; the
    /// engine populates it from the active topology on every emit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub name: String,
    pub fader: f32,
    pub muted: bool,
}

/// One strip in a captured scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneStrip {
    pub ch: u16,
    /// Stable strip key (see [`StateStrip::key`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub name: String,
    pub fader: f32,
    pub muted: bool,
}

/// One named scene in a [`Event::Shows`] frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub main_fader: Option<f32>,
    pub strips: Vec<SceneStrip>,
}

/// One show (ordered scene list) in a [`Event::Shows`] frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Show {
    pub name: String,
    pub scenes: Vec<Scene>,
}

/// One `caps` catalogue entry (canonical catalogue row). `scope` is the
/// legacy single-scope tag (kept for back-compat); `scopes` is the v2 strip-
/// kind set this id applies to (the poller filters on it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: String,
    pub kind: String,
    pub scope: String,
    pub scopes: Vec<String>,
    pub min: serde_json::Value,
    pub max: serde_json::Value,
    pub unit: String,
    pub default: serde_json::Value,
    pub label: String,
}

/// One `caps.topology` family: an ordered strip family in the global strip
/// index space. `params_scope` lists the strip kinds whose catalogue ids
/// apply here (today always `[kind]`). `strip_keys` lists the stable
/// per-strip keys in index order (`ch/01`, …, `main`); empty on old frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyFamily {
    pub key: String,
    pub kind: String,
    pub label: String,
    pub first: usize,
    pub count: usize,
    pub name_prefix: String,
    pub color_default: u8,
    pub params_scope: Vec<String>,
    /// Stable per-strip keys in index order (see [`StateStrip::key`]).
    /// `#[serde(default)]` so old `caps` fixtures still parse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strip_keys: Vec<String>,
}

/// One `caps.fx` param row (R2): a design FX type's param table entry.
/// Ranges are display-only (from the design, never desk documents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxParamCaps {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub scale: String,
}

/// One `caps.fx` FX type row (R2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxTypeCaps {
    pub id: String,
    pub label: String,
    pub params: Vec<FxParamCaps>,
}

/// `caps.fx` (R2): FX slot family of the active driver (`slots` = slot
/// count, `types` = design type tables on sim, desk-native subset on XR18).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FxCaps {
    #[serde(default)]
    pub slots: usize,
    #[serde(default)]
    pub types: Vec<FxTypeCaps>,
}

/// One `caps.groups` row (R2): addressable master-section counts behind
/// the bus/subgroup/mute-group commands. Zero = honest `unsupported` on
/// the active driver (never faked).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupsCaps {
    #[serde(default)]
    pub buses: u8,
    #[serde(default)]
    pub subgroups: u8,
    #[serde(default)]
    pub mute_groups: u8,
    /// Per-channel assign-leaf support: false where the desk has no
    /// assign leaf (X-Air: no subgroup/mute-group membership leaves —
    /// masters only). The UI collapses assign toggles there (never faked).
    #[serde(default = "default_true")]
    pub mg_assign: bool,
    #[serde(default = "default_true")]
    pub sub_assign: bool,
}

fn default_true() -> bool {
    true
}

/// `caps.files` (R2): desk file-transfer support behind `file_download` /
/// `file_upload` (`supported:false` on sim? no — sim models it in-memory;
/// false on XR18, whose OSC map has no file path).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesCaps {
    #[serde(default)]
    pub supported: bool,
}
/// One `caps.routing` patch group (R2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingGroupCaps {
    pub key: String,
    pub label: String,
    pub count: usize,
}

/// `caps.routing` (R2): source-patch support of the active driver.
/// `supported:false` (XR18: `insrc` is live-silent on FW 1.20; DL32S: no
/// patch path) means the UI hides patching; `source` stays a read-only
/// label there.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingCaps {
    #[serde(default)]
    pub groups: Vec<RoutingGroupCaps>,
    #[serde(default)]
    pub per_strip: bool,
    #[serde(default)]
    pub supported: bool,
}

/// One `devices` list entry (R2): a discovered console or I/O box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDesc {
    pub id: String,
    pub name: String,
    pub model: String,
    pub ip: String,
    pub kind: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fw: Option<String>,
    /// Where the row came from: `broadcast` (LAN discovery), `linked`
    /// (the relay's configured `--addr`, verified by direct query), or
    /// `manual` (a UI manual-add probe). Absent on older frames; the UI
    /// treats a missing source as undisclosed, never as broadcast.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub via: Option<String>,
}

/// One `scenes` library entry (R2): a numbered slot (`001..` display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEntry {
    pub number: u16,
    pub name: String,
    pub current: bool,
    #[serde(default)]
    pub saved_at: u64,
}

/// One `fx` slot state row (R2). `params` maps param id -> normalized
/// 0.0..=1.0 value in the slot type's table order; `ret_label` is the
/// design's return label (`Ret 1/2` …); `level` is the slot's return
/// meter 0..1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxSlotState {
    pub slot: u8,
    #[serde(rename = "type")]
    pub fx_type: String,
    pub on: bool,
    pub bypass: bool,
    pub ret_label: String,
    pub level: f32,
    pub params: BTreeMap<String, f32>,
}

/// One `routing` patch row (R2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEntry {
    pub strip: u16,
    /// Stable strip key (see [`StateStrip::key`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub group: String,
    pub index: u8,
    pub label: String,
}

/// One `offline_diff` change row (R2): a param write recorded while
/// offline (`from` = desk value at offline-entry, `to` = edited value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineChange {
    pub strip: u16,
    /// Stable strip key (see [`StateStrip::key`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub id: String,
    pub from: serde_json::Value,
    pub to: serde_json::Value,
}

/// Engine -> client event.
///
/// Field declaration order matches the historical `serde_json::json!`
/// construction order (`type` first), and `Option` fields skip serializing
/// when `None` so frames without a writer tag carry no `by` key at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    You {
        id: String,
    },
    State {
        driver: String,
        strips: Vec<StateStrip>,
        main_fader: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        by: Option<String>,
    },
    Param {
        ch: u16,
        field: String,
        value: serde_json::Value,
        /// Stable strip key (see [`StateStrip::key`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        by: Option<String>,
    },
    Shows {
        shows: Vec<Show>,
    },
    Meters {
        levels: Vec<f32>,
    },
    Spectrum {
        ch: u16,
        bins: Vec<f32>,
    },
    Gr {
        ch: u16,
        gate: f32,
        comp: f32,
    },
    Caps {
        driver: String,
        params: Vec<ParamDef>,
        unsupported: Vec<String>,
        #[serde(default)]
        topology: Vec<TopologyFamily>,
        #[serde(default)]
        meter_taps: Vec<String>,
        /// R2 FX family (`slots` + type tables). `#[serde(default)]` so old
        /// `caps` fixtures still parse.
        #[serde(default)]
        fx: FxCaps,
        /// R2 routing support block. Same back-compat default.
        #[serde(default)]
        routing: RoutingCaps,
        /// R2 master-section counts (buses/subgroups/mute groups). Same
        /// back-compat default (all zero = unsupported).
        #[serde(default)]
        groups: GroupsCaps,
        /// R2 file-transfer support. Same back-compat default.
        #[serde(default)]
        files: FilesCaps,
    },
    Params {
        ch: u16,
        values: BTreeMap<String, serde_json::Value>,
        /// Stable strip key (see [`StateStrip::key`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    Locks {
        holds: BTreeMap<String, String>,
    },
    Clients {
        count: usize,
        writers: Vec<String>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        locked_by: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        ch: Option<u16>,
    },
    Recalled {
        overrode_locks: Vec<u16>,
    },
    /// R2 devices event: `{"type":"devices","list":[{id,name,model,ip,
    /// kind,state,fw?}...],"scanned_at":<unix secs>}`. Broadcast after
    /// every `scan` (and unicast in the attach snapshot).
    Devices {
        list: Vec<DeviceDesc>,
        scanned_at: u64,
    },
    /// R2 link status: `{"type":"link","state":"connected|disconnected|
    /// offline","name","ip","fs":48000,"latency_ms":1.2}`. Re-emitted on
    /// every link change and every ~2 s with a fresh round-trip
    /// measurement (OSC query timing on hardware, deterministic jitter
    /// on sim).
    Link {
        state: String,
        name: String,
        ip: String,
        fs: u32,
        latency_ms: f32,
    },
    /// R2 desk identity: model, firmware, sample rate, console name.
    /// Sent on connect and as the `info` reply.
    Info {
        model: String,
        firmware: String,
        sample_rate: u32,
        console_name: String,
        driver: String,
    },
    /// R2 numbered scene library: slots `001..` with names, the current
    /// slot flag and save timestamps. Broadcast after every scene
    /// mutation; unicast in the attach snapshot.
    Scenes {
        scenes: Vec<SceneEntry>,
    },
    /// R2 FX rack state: one row per slot (type/on/bypass/return/level/
    /// params). Broadcast on every FX mutation; diffs ride the same
    /// frame (the whole rack is small).
    Fx {
        slots: Vec<FxSlotState>,
    },
    /// R2 routing patches: one row per patched input strip.
    Routing {
        patches: Vec<PatchEntry>,
    },
    /// R2 offline change report: emitted on connect while an offline
    /// change list is pending. Nothing is pushed until the client sends
    /// `offline_push` with `confirm:true` (or drops it with
    /// `offline_discard`).
    OfflineDiff {
        changes: Vec<OfflineChange>,
        count: usize,
    },
    /// R2 meter taps: one frame per tap per tick (`in` pre-fader,
    /// `post` post-fader 0..1; `gr_gate`/`gr_comp` dB reduction `>= 0`).
    /// Old clients ignore the frame; taps-aware clients negotiate it
    /// with `hello {"taps":true}` (JSON) or take WS BINARY type 3.
    Taps {
        tap: String,
        levels: Vec<f32>,
    },
    /// R2 mix-bus master echo (`bus_fader`/`bus_mute`): full master state
    /// after the write (both fields re-read from the driver).
    Bus {
        bus: u8,
        fader: f32,
        muted: bool,
        /// Stable master key (`bus/01`): `None` on old frames.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 subgroup master echo (`subgroup_fader`/`subgroup_mute`).
    Subgroup {
        group: u8,
        fader: f32,
        muted: bool,
        /// Stable master key (`subgroup/1`): `None` on old frames.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 subgroup-membership echo (`subgroup_assign`).
    SubgroupAssign {
        ch: u16,
        group: u8,
        member: bool,
        /// Stable strip key (see [`StateStrip::key`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 mute-group master echo (`mute_group`).
    MuteGroup {
        group: u8,
        muted: bool,
        /// Stable master key (`mgroup/1`): `None` on old frames.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 mute-group-membership echo (`mute_group_assign`).
    MuteGroupAssign {
        ch: u16,
        group: u8,
        member: bool,
        /// Stable strip key (see [`StateStrip::key`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 Main LR master echo (`main_mute`): full master state after the
    /// write (fader + mute re-read from the driver). Also unicast in the
    /// attach snapshot so fresh pages learn the master without a write.
    Main {
        fader: Option<f32>,
        muted: Option<bool>,
    },
    /// R2 per-strip solo echo (`solo`): the confirmed solo state after the
    /// write. Sparse in the attach snapshot (only soloed strips emit).
    Solo {
        ch: u16,
        soloed: bool,
        /// Stable strip key (see [`StateStrip::key`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// R2 file frame: `file_download` replies carry base64 `data` (`size`
    /// = decoded bytes); `file_upload` replies are acks (`data` absent,
    /// `size` = bytes written).
    File {
        filename: String,
        size: usize,
        file_type: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
/// Realtime negotiation ack (MR1): unicast reply to [`Cmd::Hello`],
    /// carrying the client's current binary-meter preference. Never
    /// broadcast. Old clients never see it (they never send `hello`).
    /// R2 adds `taps`: the client's meter-tap preference (skipped on the
    /// wire while false, so the MR1 ack stays byte-identical).
    Hello {
        binary_meters: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        taps: bool,
    },
}

impl Event {
    /// The `type` tag string.
    pub fn tag(&self) -> &'static str {
        match self {
            Event::You { .. } => "you",
            Event::State { .. } => "state",
            Event::Param { .. } => "param",
            Event::Shows { .. } => "shows",
            Event::Meters { .. } => "meters",
            Event::Spectrum { .. } => "spectrum",
            Event::Gr { .. } => "gr",
            Event::Caps { .. } => "caps",
            Event::Params { .. } => "params",
            Event::Locks { .. } => "locks",
            Event::Clients { .. } => "clients",
            Event::Error { .. } => "error",
            Event::Recalled { .. } => "recalled",
            Event::Hello { .. } => "hello",
            Event::Devices { .. } => "devices",
            Event::Link { .. } => "link",
            Event::Info { .. } => "info",
            Event::Scenes { .. } => "scenes",
            Event::Fx { .. } => "fx",
            Event::Routing { .. } => "routing",
            Event::OfflineDiff { .. } => "offline_diff",
            Event::Taps { .. } => "taps",
            Event::Bus { .. } => "bus",
            Event::Subgroup { .. } => "subgroup",
            Event::SubgroupAssign { .. } => "subgroup_assign",
            Event::MuteGroup { .. } => "mute_group",
            Event::MuteGroupAssign { .. } => "mute_group_assign",
            Event::Main { .. } => "main",
            Event::Solo { .. } => "solo",
            Event::File { .. } => "file",
        }
    }

    /// Serialize to the wire JSON string (what adapters put on the socket).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Event is always serializable")
    }

    /// Encode a meter-class event as a compact WS BINARY frame (MR1).
    ///
    /// Layout (little-endian, documented in `crates/engine/README.md`):
    /// `[u8 type (1=meters, 2=gr, 3=taps), u8 version=1, u16 n, u32 seq,
    /// u32 t_ms]`, then for meters `n × u16` (linear 0..1 → 0..65535),
    /// for gr `n × (u8 ch, u8 gate dB×4, u8 comp dB×4)` (currently one
    /// entry per frame: each per-channel `Gr` event encodes as `n=1`),
    /// for taps `[u8 tap id]` then `n × u16` (tap-native values mapped to
    /// 0..1 first: `in`/`post` are already 0..1, `gr_gate`/`gr_comp` dB
    /// are divided by [`TAPS_GR_DB_FULLSCALE`] = 24 dB and clamped).
    /// Returns `None` for every other event: binary is an encoding choice
    /// for meter fan-out only, so the adapter falls back to JSON text for
    /// control frames (and for `spectrum`, which stays JSON).
    ///
    /// `seq` is a per-client monotonic frame counter (adapter-owned),
    /// `t_ms` the engine monotonic clock in ms (`Engine::now_ms`).
    pub fn to_binary(&self, seq: u32, t_ms: u32) -> Option<Vec<u8>> {
        match self {
            Event::Meters { levels } => {
                let n = levels.len().min(u16::MAX as usize);
                let mut out = Vec::with_capacity(BIN_HEADER_LEN + n * 2);
                out.push(BIN_TYPE_METERS);
                out.push(BIN_VERSION);
                out.extend_from_slice(&(n as u16).to_le_bytes());
                out.extend_from_slice(&seq.to_le_bytes());
                out.extend_from_slice(&t_ms.to_le_bytes());
                for level in levels.iter().take(n) {
                    out.extend_from_slice(&level_to_u16(*level).to_le_bytes());
                }
                Some(out)
            }
            Event::Gr { ch, gate, comp } => {
                let mut out = Vec::with_capacity(BIN_HEADER_LEN + 3);
                out.push(BIN_TYPE_GR);
                out.push(BIN_VERSION);
                out.extend_from_slice(&1u16.to_le_bytes());
                out.extend_from_slice(&seq.to_le_bytes());
                out.extend_from_slice(&t_ms.to_le_bytes());
                out.push((*ch).min(u16::from(u8::MAX)) as u8);
                out.push(db_to_u8(*gate));
                out.push(db_to_u8(*comp));
                Some(out)
            }
            Event::Taps { tap, levels } => {
                let n = levels.len().min(u16::MAX as usize);
                let mut out = Vec::with_capacity(BIN_HEADER_LEN + 1 + n * 2);
                out.push(BIN_TYPE_TAPS);
                out.push(BIN_VERSION);
                out.extend_from_slice(&(n as u16).to_le_bytes());
                out.extend_from_slice(&seq.to_le_bytes());
                out.extend_from_slice(&t_ms.to_le_bytes());
                out.push(tap_id(tap));
                for level in levels.iter().take(n) {
                    out.extend_from_slice(
                        &taps_level_to_u16(tap, *level).to_le_bytes(),
                    );
                }
                Some(out)
            }
            _ => None,
        }
    }
}

/// Parse one wire JSON line into a [`Cmd`].
///
/// Unknown `cmd` tags keep the historical `unknown cmd "bogus"` message;
/// other shape errors become `invalid command: ...`. Both are plain errors
/// for the adapter to wrap in an [`Event::Error`] unicast — the connection
/// stays open either way.
pub fn parse_cmd(text: &str) -> Result<Cmd, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    if let Some(tag) = v.get("cmd").and_then(serde_json::Value::as_str) {
        if !Cmd::known_tags().contains(&tag) {
            return Err(format!("unknown cmd {tag:?}"));
        }
    }
    serde_json::from_value(v).map_err(|e| format!("invalid command: {e}"))
}

/// `skip_serializing_if` for additive bools that default false (keeps
/// older frames byte-identical while the flag is off).
pub fn is_false(v: &bool) -> bool {
    !*v
}

/// Binary meter frame: `meters` payload type byte.
pub const BIN_TYPE_METERS: u8 = 1;
/// Binary meter frame: `gr` payload type byte.
pub const BIN_TYPE_GR: u8 = 2;
/// Binary meter frame: `taps` payload type byte (R2).
pub const BIN_TYPE_TAPS: u8 = 3;
/// Binary meter frame layout version (bump on any layout change).
pub const BIN_VERSION: u8 = 1;
/// Binary meter frame header length in bytes
/// (`type u8, version u8, n u16, seq u32, t_ms u32`, all LE).
pub const BIN_HEADER_LEN: usize = 12;
/// Max GR value representable in the `u8 dB×4` binary encoding
/// (255 / 4 = 63.75 dB reduction; larger values saturate).
pub const GR_DB_MAX: f32 = 63.75;
/// Full-scale dB for GR taps in binary taps frames: `gr_gate`/`gr_comp`
/// dB values are divided by this before `u16` quantization (sim GR taps
/// peak near 12 dB, so 24 dB keeps headroom without wasting codes).
pub const TAPS_GR_DB_FULLSCALE: f32 = 24.0;

/// Meter-tap wire id for binary taps frames (`in`=0, `post`=1,
/// `gr_gate`=2, `gr_comp`=3; anything else = 255).
pub fn tap_id(tap: &str) -> u8 {
    match tap {
        "in" => 0,
        "post" => 1,
        "gr_gate" => 2,
        "gr_comp" => 3,
        _ => 255,
    }
}

/// Binary taps-frame id back to a tap name (`None` for unknown ids).
pub fn tap_name(id: u8) -> Option<&'static str> {
    match id {
        0 => Some("in"),
        1 => Some("post"),
        2 => Some("gr_gate"),
        3 => Some("gr_comp"),
        _ => None,
    }
}

/// Tap-native value → `u16` binary taps sample: `in`/`post` ride
/// [`level_to_u16`]; GR dB values scale by [`TAPS_GR_DB_FULLSCALE`].
pub fn taps_level_to_u16(tap: &str, v: f32) -> u16 {
    match tap {
        "gr_gate" | "gr_comp" => level_to_u16(v.max(0.0) / TAPS_GR_DB_FULLSCALE),
        _ => level_to_u16(v),
    }
}

/// `u16` binary taps sample → tap-native value (inverse of
/// [`taps_level_to_u16`]).
pub fn u16_to_taps_level(tap: &str, q: u16) -> f32 {
    match tap {
        "gr_gate" | "gr_comp" => u16_to_level(q) * TAPS_GR_DB_FULLSCALE,
        _ => u16_to_level(q),
    }
}

/// Linear 0..1 level → `u16` binary meter sample (clamped, rounded).
pub fn level_to_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// `u16` binary meter sample → linear 0..1 level.
pub fn u16_to_level(q: u16) -> f32 {
    f32::from(q) / 65535.0
}

/// GR dB reduction (>= 0) → `u8` binary sample at 1/4 dB
/// (clamped to `0..=GR_DB_MAX`, rounded).
pub fn db_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, GR_DB_MAX) * 4.0).round() as u8
}

/// `u8` binary GR sample → dB reduction.
pub fn u8_to_db(q: u8) -> f32 {
    f32::from(q) / 4.0
}
