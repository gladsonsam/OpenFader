# Changelog

All notable changes to this project are documented here.
History below is reconstructed from `git log` (oldest → newest);
every entry names the commit(s) it comes from. No releases tagged yet —
everything is under `Unreleased`.

## Unreleased

### Drivers (`crates/proto-xair`, `crates/proto-dl32s`, `crates/transport`)

- MVP: X-Air + DL32S drivers, simulator, CLI, Tauri stub (`52f16c9`).
  OSC session/keepalive/discovery/meter plumbing in `openmix-transport`;
  X-Air address table in `proto-xair`; Mackie/DL32S frame codec + push
  caches in `transport::mackie` / `proto-dl32s`.
- X-Air mains via `/lr/*` — **hardware-verified XR18 FW 1.20** (`8f6a133`).
  X32-style `/main/st/mix/fader` gets NO reply on the desk; driver uses
  `MAIN_FADER = /lr/mix/fader` (+ `MAIN_ON`, `MAIN_NAME` by analogy —
  not yet live-queried, see "Not yet"). Loopback test
  `main_fader_uses_lr_not_main_st` uses a strict fake silent on
  `/main/st`, like the real desk.
- Hardware-verified X-Air fixes (`a2f327f`, XR18 FW 1.20, `docs/xr18-baseline.md`):
  - Buses are single-digit `/bus/1/…` — desk is silent on zero-padded
    `/bus/01/…`. **Hardware-verified.**
  - XR18 is 16 channels (`/ch/01..16` reply, `/ch/17..18` silent) plus
    stereo aux return `/rtn/aux` (fader/on/name all reply).
    **Hardware-verified.** Per-model bound (`Xr18` default / `X32Kind`).
  - Fader wire quantization: desk rounds writes to ~10-bit steps
    (write 0.5 → read back ~0.49951124, off by ~5e-4). Round-trips must
    assert with `FADER_READBACK_EPSILON = 1e-3`. **Hardware-verified.**
    Send levels and FX params round-trip bit-exact (exact asserts kept).
  - `/info` field order fix (`96ea6bb`): hardware returns
    `[V0.04, name, model, fw]` — mapping corrected. **Hardware-verified.**
- Meter subscribe — **hardware-verified** (`96ea6bb`, 2026-10-18):
  subscribe = send `/meters` with ONE string arg `/meters/1`
  (typetag `,s`); bare/empty queries to `/meters/0|1` get silence.
  Desk streams LE-i16 blobs.
- Meter renewal loop (`a01e25e`) + renewal rate **hardware-verified
  20/s** (`6faea4e`, 2026-10-18 ~01:30 AWST): with `/xremote` +
  resubscribe every 8s, 601 blobs/30s (~50ms rate confirmed).
  `METER_RENEW_INTERVAL = 8s` is the driver default.
- Meter 10s expiry — **hardware-proven** (`78c7730`, 2026-10-18 ~00:50
  AWST): one subscribe, no renewals → `[200, 0, 0, 0, 0, 0, 0]` blobs
  per 10s window over 70s; stream dies at ~10s sharp. 8s renewal
  confirmed correct with margin.
- Headamp (`e894c06` adds `headamp.rs` + driver wiring; `0a27ad5`
  marks it verified): gain + phantom reads reply
  (`/headamp/01/gain = 0.17`, `phantom = 0`; `/headamp/16/gain = 0.17`)
  and a same-value gain write was ACKed (readback 0.1667 = write path +
  wire quantization confirmed). **Owner-rig verified XR18 FW 1.20,
  2026-10-18 ~01:00 AWST.**
- FX: X-Air mapping + rack UI (`fec4191`), then labeled params from
  research — 1457-line `fx.rs` expansion + full `FxRack.svelte`
  rewrite (`51111ac`).
- Dynamics: live GR meters — `dyn_gr.rs` (gate/comp GR from `/meters/6`
  DYN blob) + driver wiring (`29a3b16`); live relay of GR frames to UI
  (`cd145fb`). NOTE: `docs/user-guide.md` still flags GR frames as
  **TODO-owner** (relay emission vs. UI wiring — confirm before a gig).
- DL32S: behavior-faithful port of the proven Python reference (frame
  layout, init sequence, push-not-poll, count==0 bulk rule,
  word-swapped strings). Covered by table/mapping tests + loopback
  fake-mixer session test. Input-strip table checked against a hardware
  dump; meters, rename encoding, file transfer deliberately out of
  scope (see "Not yet").

### Engine (`crates/core`, `crates/sim`, `crates/cli`)

- Additive engine: shows, offline sync, RTA, access control, FX
  framework (`dcb45fe`).
- RTA: real FFT + spectrum UI with fallback mode (`3487ad7`).
- Sim: full param map at detail-panel parity (`5c65c83`); sim presets
  (`ffea069` — part of the ops commit).
- CLI: set-only `fader`/`mute` commands used for the driver-path write
  proof (`fader 2 50` → desk reads 0.49951124, restored to 0.0 exact;
  `835d8a3` / `docs/xr18-baseline.md` §6). `name` is sim-only on CLI;
  X-Air `snapshot()` still unimplemented (see "Not yet").

### Serve + Docker (`crates/serve`, `Dockerfile`, `docker-compose.yml`)

- Headless WS relay + Docker, run-anywhere (`4bd92e5`): `openmix-serve`
  with `ws_api` tests, `docs/docker.md`.
- Ops: Docker verify + sim presets + live-rig checklist
  (`ffea069`): `.dockerignore`, `docker-compose.yml`, `scenes/`,
  `docs/LIVE-RIG.md`, `docs/docker.md`.
- Spectrum messages, 16ch cap (`6767896`).
- Scene ops / show sync on the relay: `list_shows` / `capture_scene` /
  `recall_scene` / `delete_scene` + `shows` broadcast (`0f78089`).
- Param echo broadcast (`6264902`); GR live relay to UI (`cd145fb`);
  client presence — `clients` count/writers broadcast, `by` writer id
  on mutation frames (`e5c114f`). Writers = ids with a mutation in the
  last 10s. Concurrency is last-write-wins (see "Not yet").

### Apps (`apps/web`, `src-tauri`, `ui/` legacy)

- Pro mixer v1 — layers, detail, scenes, Tauri+WS (`a616ca0`, legacy
  `ui/`); Svelte mixer UI as static dist (`7a3f989`); `mount()`-not-
  `new App()` `effect_orphan` fix (`4e79889`).
- TypeScript migration with strict backend contract (`83ab7fc`).
- Tauri: full engine commands + Svelte dist (`f44b241`); spectrum+GR
  parity (`fa397d1`); mobile prep — config only (`39d1ef9`, see
  "Not yet").
- Web v2 views: Mixer / Channel / SoF / Layers + safety
  (`852a727`): sends-on-fader with tint warning + `copy LR`, custom
  layers with persist, `ValueEntry`, Overview.
- PWA installable — manifest, icons, service worker (`6094891`).
- DSP graphs: EQ+RTA graphs, fat strips (`55d9f41`); fat strips live +
  shelf curves (`e56f735`); per-strip bands store + type sync
  (`b0e80d3`); meter render perf (`ad62534`).
- Offline show editor (`ebf56d6`); show sync toward serve (`c5a9a4e`);
  WS end-to-end smoke test (`d412a60`); accessibility pass, 0 warnings
  (`21020de`).
- XR18 hardware baseline FW 1.20 (`0248088`): desk identity, 84-address
  strip snapshot (restore target), verified behaviors V1–V6.
- XR18 write-test results + restore proof (`835d8a3`): 7 ops
  (ch fader/mute/rename, aux send, FX param, bus master, LR master) —
  all PASS, desk re-swept 84/84 match, left exactly as found.
- Gain-staging wizard: `GainWizard.svelte` + headamp driver support
  (`e894c06`). Only wizard so far (see "Not yet").

### Docs

- User guide (`6d2056c`) + refresh for landed features (`f8faf95`).
- `docs/LIVE-RIG.md`: per-desk hardware verification checklist.
- `docs/mobile.md`: toolchain setup + LAN permission + signing notes.
- `docs/docker.md`, `docs/xair-protocol.md` (live observation rows,
  meter subscribe/renewal/expiry sections), `docs/xr18-baseline.md`
  (phases 1–2), `apps/web/V2.md`, `src-tauri/README.md`.

## Not yet (honest gaps — do not depend on these at a gig)

- **DL32S live-desk verification:** no owner-rig run from this repo.
  Evidence is loopback fake-mixer tests + the ported reference's
  hardware dump for the input strip. Unconfirmed: meter value encoding
  (meters unsupported), `set_name` rename write encoding (unsupported —
  CLI rename is expected to fail with `set_name not supported`),
  snapshot recall/save bodies, DL16S EQ packing and DL32R layout
  (driver rejects `dl16s`/`dl32r`), GENERAL_INFO string decode, and all
  file transfer (`show.zip` download / upload need a `zip` dep —
  not ported). Source: `docs/dl32s-protocol.md` "Verified vs
  unconfirmed", `docs/LIVE-RIG.md` §4.
- **Tauri mobile needs SDKs:** `39d1ef9` is config only
  (`minSdkVersion`, `minimumSystemVersion`, `capabilities/mobile.json`).
  Native projects NOT generated on this machine (no Android SDK/NDK,
  no Xcode — Linux); owner runs `android/ios init` on a tooled machine,
  then pastes the §5 manifest/plist snippets, smoke-tests against a
  real desk, and sets up signing. Source: `docs/mobile.md` §§1–7.
- **Multi-client locking:** relay tracks presence and tags mutations
  with `by`, but concurrency is last-write-wins — "no locking or
  rejection" (`crates/serve/src/lib.rs` header docs). `docs/user-guide.md`
  marks multi-client arbitration **TODO-owner**: confirm before running
  FOH + monitors from two devices on one relay.
- **Wizards pending:** only gain staging has landed (`e894c06`). Any
  further setup/recovery wizards are still to be built.
- **Web ↔ relay show sync:** the web `ShowEditor`/`Backend` is
  `localStorage`-only over WS, not wired to the relay's scene ops —
  **TODO-owner** per `docs/user-guide.md` §5. Browser copy is the
  source of truth until that sync TODO closes; same for the
  `./scenes` Docker volume (deployment-side copy, not auto-sync).
- **X-Air extras, explicitly unconfirmed:** `/lr/mix/on` and
  `/lr/config/name` by analogy only (not live-queried);
  per-send on/off likely does not exist on X-Air (`/ch/NN/mix/MM/on`
  silent while `/level` replies — T7, no send-mute API exposed);
  `XAirDriver::snapshot()` unimplemented so CLI `list` fails against
  live hardware (T6); GR frames flagged **TODO-owner** in the user
  guide despite the relay emission (`cd145fb`).
