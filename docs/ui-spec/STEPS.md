# UI v3 implementation steps

Read `docs/ui-spec/SPEC.md` first. Each step is a goal with acceptance criteria; the implementer
decides how. A step is done only when every acceptance line is true and verified.

## Ground rules (every step)

- Work in `apps/web/src/v3/` (plus the minimal hook in `src/main.ts`). Svelte 5 runes + TypeScript.
- No new dependencies. Do not edit `crates/`, `src-tauri/`, or the existing `src/components/*`
  (import from `src/lib/*` freely; if a lib needs a change, make it additive and say so).
- Do NOT commit, push, or run `npm install`, `npm run build`, `svelte-check`, or full repo checks
  (shared box, RAM-limited). Verification you may run: `node --experimental-strip-types <file>.test.ts`
  for pure-logic tests, and `npx vite --port <free port>` only if you need to load a page.
- Every visible component must appear in the gallery (`?ui=3&gallery`) in all its states, so a
  reviewer can screenshot it. Gallery sections have stable ids: `#g-<component>`.
- Finish with a report: files created/changed, how each acceptance line was verified, open issues.
  Do not claim something works that you did not run.

## M0: Foundations

Goal: v3 skeleton that renders behind a flag, with tokens, fader law and profile schema.
- `src/main.ts` mounts v3 when URL has `ui=3`; without it the app is byte-for-byte unchanged in behaviour.
- `v3/tokens.css`: spec section 3 tokens (surfaces, text, state LEDs, 12 channel colours, type scale,
  4px spacing, radius, motion) with `dark`, `daylight`, `night` themes via `[data-theme]`.
- `v3/faderLaw.ts` + `faderLaw.test.ts`: `toDb`, `fromDb`, tick list, per-driver registry (spec section 4).
  Test: unity at 0.75 -> 0 dB, 1.0 -> +10, 0.5 -> -10, 0.25 -> -30, floor -> -inf; `fromDb(toDb(f)) ~= f`
  over 1000 samples (error < 1e-6); monotonic.
- `v3/profile.ts` + `profile.test.ts`: types, shipped defaults per role x device class (spec section 7),
  `resolveProfile(base, overrides)`, `validateProfile(unknown)` (rejects bad input without throwing),
  JSON export/import. Test: roundtrip, reject garbage, overrides layer over defaults, reset clears them.
- `v3/Gallery.svelte` with a theme switcher and a section list (empty is fine at this step).
Acceptance: both tests exit 0; `?ui=3&gallery` loads with no console errors and the theme switcher
changes `data-theme`; the plain URL still shows the old app.

## M1: Primitives

Goal: the atomic controls, each pixel-reviewed in the gallery.
- `Fader.svelte`: vertical, DOM/SVG or canvas, full-width hit area, pointer capture, taper via
  `faderLaw`, scale ticks, value box that follows the cap, channel-colour track fill, fine mode,
  double-tap to 0 dB, keyboard (arrows/PageUp/PageDown/Home/End), optional `max` cap (musician),
  `disabled`, `unknown` look. Emits `onChange(v01)`.
- `Meter.svelte`: canvas, reads `levels` imperatively via rAF, peak hold with decay, clip latch cleared
  on tap, green/amber/red gradient, stereo mode, `compact`. No reactive per-frame updates.
- `StateButton.svelte` (mute/solo/generic): >=44px, on/off/locked/pending looks, icon+text cue.
- `ColorBar.svelte`/`Scribble.svelte`, `Knob.svelte` (compact, drag + keyboard + double-tap reset),
  `GrBar.svelte` (gain-reduction bar), `Readout.svelte` (tabular mono value).
- Gallery: each component in default/active/disabled/unknown/extreme states, on all 3 themes.
Acceptance: all targets >=44px (touch) measured in gallery; fader keyboard + pointer both work;
meters animate from simulated levels in the gallery at 60 fps with 32 instances (report measured fps);
axe-style manual checks listed in the report (labels/roles/focus ring).

## M2: Strip and section registry

Goal: the composable strip from spec section 6.
- `sections/registry.ts` with the catalogue; one component per section; `Strip.svelte` renders
  sections from a profile; unknown-state styling when no trusted data.
- Wires to the real data shapes (`Strip` from backend.ts, `bands.ts`, GR events, sends cache) via props;
  gallery uses fixture data. Colour per channel from profile.
Acceptance: gallery shows strips at narrow/normal/wide, with full chain, tablet set and phone set;
toggling a section in the gallery re-flows without layout jump; fixture "no data" strip shows the
unknown look, never invented numbers.

## M3: Mix view (live)

Goal: the main screen working against the sim driver.
- `MixView.svelte`: strip bank (scrollable/paged), layer rail, master panel, console status strip,
  view nav shell, responsive per device class (spec section 7), `ResizeObserver`-driven.
- Real wiring: connect (sim by default as in current app), `onState`, `onMeters`, `setFader`, `setMute`,
  solo (local), clip latch, mute-enable master, fine mode.
Acceptance: at 1440x900, 820x1180 and 390x844 the layout matches the class defaults; moving a fader
updates state and the dB readout using the X-Air law; meters move; 16 and 32 strip banks both usable;
no horizontal page scroll; screenshots attached to report.

## M4: Channel view

Goal: selected strip detail (EQ graph, dynamics, input, sends, pan) reusing `EqGraph` logic from
`lib/eqCurve.ts`. Desktop docks above the bank; phone full screen. Edits go through `setParam`
and the local mirror.
Acceptance: dragging an EQ node changes the curve and sends `param`; GR shows live; Escape/back returns.

## M5: Sends on fader + bus rail

Goal: port the v2 behaviour: tap a bus -> all faders become sends, whole surface tints in the bus
colour, banner with EXIT, auto-cue, copy LR, persisted send cache via `lib/sends.ts`.
Acceptance: entering/exiting is one gesture each; no state leaks into main faders; reload keeps sends.

## M6: Roles: Operator and Musician

Goal: role-shaped UIs (spec sections 1, 5). `Musician` screen (port `MusicianView` behaviour with v3
primitives, fader cap, CALL FOH, single-tap exit, kiosk-friendly URL `?ui=3&role=musician&bus=N`);
`Operator` preset (curated strips, scenes, no processing).
Acceptance: hidden, not just disabled, controls for each role; musician cannot exceed the cap by any
gesture or keyboard; role switch needs a deliberate action.

## M7: Customisation

Goal: the profile editor from spec section 7a.
- In-place Edit layout mode: reorder strips, toggle sections, strip width, channel colours, layers,
  theme/density; export/import JSON; reset to role default; persists under `openmix.profile.v1.*`.
Acceptance: unit tests for override layering and migration; changing a section in the editor updates
Mix live; export then import on a clean profile reproduces the layout; role limits enforced.

## M8: Remaining views

Goal: Overview, FX, Routing, Shows/scenes, Setup/onboarding, clients roster, lock badges ported onto v3
tokens and primitives (reuse logic from existing components; do not copy old visuals).
Acceptance: every existing view has a v3 equivalent or an explicit, documented omission.

## M9: Polish and cutover readiness

Goal: accessibility pass, perf budget, `prefers-reduced-motion`, theme QA, docs. Flip default is the
owner's decision, not part of this step.
Acceptance: written a11y checklist with results; perf numbers; list of known gaps.

## MS: Param plane (Rust: catalogue, driver support, relay frames)  [inserted after M3 review]

Why: the relay only reports fader/mute/name + meters. The XR18 driver only implements fader, mute,
name, gain, phantom, usbreturn, trim, polarity, hpf_on, hpf_freq; EQ/gate/comp/pan/sends are not
implemented and DL32S only has fader/mute/name. The UI therefore cannot show or safely edit them.
This milestone adds a param plane, then MS2 (TypeScript) consumes it.

Rules: repo `no-guessing rule`: every OSC address you add must cite its source in a doc comment and
be marked UNVERIFIED-ON-RIG until the owner confirms it on the XR18 (add a checklist section to
docs/LIVE-RIG.md). NEVER send anything to real hardware: tests use the fake-desk loopback pattern
already used in proto-xair and the sim driver. Memory rules: run cargo ONE command at a time, first
`pgrep -af 'cargo|rustc'`; only targeted `cargo test -p <crate>` / `cargo build -p <crate>` (no
workspace-wide test, no release builds). Do not touch the relay on port 8080 (use your own port for
tests, e.g. 8091).

Goals:
1. core: `crates/core/src/params.rs` canonical param catalogue: id, kind (float/bool/enum/text),
   scope (channel / bus / main), range+unit, default, label. Cover the ids in apps/web/src/lib/sections.ts
   (gain, phantom, usbreturn, trim, polarity, pan, hpf_on, hpf_freq, gate_on, comp_on, eq_on,
   eqN_gain/freq/q/type for N=1..4, auxN_level/auxN_mute N=1..8, fxN_level/fxN_mute N=1..4) with
   ranges that match the real hardware (X-Air EQ gain is +-15 dB not +-20: put the true range in the
   catalogue and list any discrepancy with sections.ts in the report; the UI will follow the catalogue).
2. core trait: `MixerDriver::capabilities()` returns which catalogue ids this driver supports
   (default = the fader/mute/name set); drivers override. `get_param`/`set_param` stay the I/O path.
3. sim: implement the whole catalogue with per-strip state (get/set), and a way for tests to mutate
   state as if from the desk (to exercise change detection).
4. proto-xair: implement get/set for EQ (on + 4 bands gain/freq/q/type), gate_on, comp_on, pan, and
   aux/FX sends (level + mute) using the X-Air OSC addresses from the public X-Air/X32 OSC references
   already cited in docs/xair-protocol.md (extend that doc with a table: id -> address -> wire type ->
   mapping), including the log/linear wire<->engineering value mappings; loopback tests against a
   strict fake desk like the existing ones; mark everything UNVERIFIED-ON-RIG. DL32S: capabilities =
   fader/mute/name only (unchanged behaviour).
5. relay (crates/serve): (a) on driver connect send `{"type":"caps","driver":..,"params":[{id,kind,scope,min,max,unit,default,label}...]}`
   to every client (and again to a newly connected client); (b) a background param poller that, for
   supported params only, reads every channel's catalogue params at a modest rate (spread out, e.g.
   <= 20 queries/s total, round-robin, never starving fader/meter traffic), caches them, and
   broadcasts `{"type":"params","ch":N,"values":{id:value,...}}` only when values changed (diff), plus a
   full cache snapshot to each newly connected client; (c) after any client `param` write, refresh that
   param immediately and broadcast the confirmed value (`by` = writer); a param whose read returns an
   unsupported/timeout error is dropped from the poll set for that driver and reported once in
   `caps.unsupported` (no log spam). Existing frames (state, meters, param echo, locks, shows) are
   unchanged. Document the new frames in the crates/serve module doc and apps/web/src/lib/backend.ts
   header comment (comment-only edit there).
6. Tests: Rust unit/integration tests for catalogue consistency (ids unique, ranges sane, default in
   range), sim get/set roundtrip for every id, diff-broadcast logic (no frame when unchanged, one frame
   on change), snapshot-on-connect, unsupported-param dropout; plus extend apps/web/e2e-smoke.mjs with
   sections `caps` and `params` (run it against YOUR relay on :8091 with `OPENMIX_WS=ws://127.0.0.1:8091/ws node apps/web/e2e-smoke.mjs`).

Acceptance (reviewer re-runs these): `cargo test -p openmix-core`, then `-p openmix-sim`, then
`-p proto-xair` (check actual package names in each Cargo.toml), then `-p openmix-serve`, each green
and run one at a time; the smoke run against your own sim relay passes including caps/params
sections and shows caps listing the full catalogue for sim and only fader/mute/name for a dl32s-driver
relay (or documented equivalent); a recorded WS transcript (first ~30 frames after connect) pasted in
the report; docs/xair-protocol.md table and the LIVE-RIG.md checklist exist. Commit nothing. Report:
files changed, test output per crate, transcript, discrepancies (ranges), UNVERIFIED list.

## ME: Engine extraction (Rust, then UI transport)  [architecture fix after the mobile review]

Problem: the session layer lives in `crates/serve` (2017 lines) and `src-tauri/src/main.rs` re-implements
a subset ("mirrors serve"); the web UI has two backends. New features (param plane) exist only in the
relay, so native apps cannot use the full mixer. See ARCHITECTURE.md "Capability parity".

ME1 (Rust, crates only): create `crates/engine` (`openmix-engine`) and MOVE (not rewrite) all session
logic out of `crates/serve`: connect/driver registry, strip state cache, param plane (caps, poller,
focus, diff, pending confirm), meters/spectrum/GR aggregation, scenes/shows, locks + TTL sweeper,
access policy (CheckedDriver), client roster/writers, `by` attribution. Public API: `Engine::new`,
`engine.attach() -> ClientHandle { id, send(Cmd), events: Stream<Event> }` (+ detach), `Cmd`/`Event`
serde enums whose JSON is BYTE-COMPATIBLE with the existing WS protocol. `crates/serve` shrinks to an
axum adapter (WS upgrade, static files, CLI) that only converts JSON <-> Cmd/Event.
Acceptance: all existing tests pass UNCHANGED (serve's 11 WS integration tests, core, sim, xair, dl32s);
smoke `apps/web/e2e-smoke.mjs` 15/15 against the new serve on its own port; serve's lib no longer
contains polling/cache/scene/lock logic (grep-verifiable); engine has its own unit/integration tests
driving it in-process (no sockets) with the same scenarios as serve's WS tests; golden frame files in
`crates/engine/tests/golden/*.json` generated by a Rust test; `crates/engine/README.md` is the single
protocol spec. cargo: one command at a time, targeted -p only.

ME2 (after M4 lands; UI + Tauri): one `Backend` implementation over a `Transport` interface
(`send(json)`, `onMessage(cb)`, `onStatus(cb)`); `WsTransport` and `TauriTransport`
(`invoke('engine_cmd')` + `listen('engine_event')`); delete the hand-written `TauriBackend`; src-tauri
becomes a thin adapter over the engine (Tauri v2 `Channel`/events), no mixer logic; node test parsing
every golden frame with the TS parser. src-tauri cannot be compiled on this box (libdbus-1 missing), so
keep the adapter ~100 lines and verify it on CI (see MP).

MP (mobile, after ME2 + v3 feature-complete): CI builds: Android APK on ubuntu runner, iOS on a macOS
runner; iOS NSLocalNetworkUsageDescription/Bonjour, Android network permissions; signing out of scope.

## MR: Realtime path (after ME1; UI part after M4/ME2)  [added after latency review]

Binding text: SPEC section 2a "Realtime budget". Findings that motivate it: the relay downsamples
meters to 100 ms ticks and sends JSON text; the v3 UI throttles GR to 5 Hz because it went through
reactive state; write priority over the poller and TCP_NODELAY are unverified.

MR1 (Rust, engine/serve/transport): (a) write priority: fader/mute/param writes and meter fan-out
preempt the poller (separate lanes; poller yields, measured); (b) meters: engine emits at source rate
with seq + monotonic timestamp, adapter forwards without downsampling; add a binary meter frame
(WS binary message: header [type=1, seq u32, t_ms u32, n u16] then n x u16 levels or u8 dB-scaled;
document in crates/engine/README.md); keep the JSON `meters` frame available behind a client opt-in
for old clients (default for old UI), new `hello` capability negotiation `{"cmd":"hello","binary_meters":true}`;
same for gain-reduction (`gr`) and later RTA; (c) latest-only delivery for slow clients (bounded
queue of 1 per stream, count drops, expose in a `stats` frame/endpoint); (d) TCP_NODELAY on accepted
sockets, no permessage-deflate; (e) a Rust bench/test that measures write-path latency p50/p95/p99
through engine -> fake UDP desk on loopback while the poller and meters are saturated, asserting
p99 <= 5 ms.
MR2 (UI, v3 + lib): binary meter decode into the shared levels buffers (Float32Array / Uint8Array,
no per-frame allocation), GR through the same non-reactive pipeline at >= 20 Hz (remove the 5 Hz
coalescing), ballistics (attack/release, peak hold) per frame in the canvas painter, leading-edge
write sender (replace FaderWriter's trailing throttle), local-first fader state (no store round trip
while dragging), `?perf=1` overlay, and the latency harness `apps/web/tools/latency.mjs` (Playwright:
pointer->paint via performance marks, write RTT via echo frame with own `by`, meter age at paint from
frame timestamps, dropped frames, long tasks) with thresholds from SPEC 2a.
Acceptance: harness numbers pasted and within budget on loopback against the sim relay at default
and at saturated load (32 meters live + poller running + a second client dragging faders).

## MP0: Device spike (insert right after M4; before more polish)

Goal: prove the stack on real phones before building the remaining screens. Build the current Mix
view as a debug APK (CI ubuntu runner or a toolchain box) and on an iPhone/iPad if a Mac is available;
measure with `?perf=1` and a 240 fps phone-camera tap-to-pixel test; compare Tauri IPC against the
loopback-WebSocket fallback (see ARCHITECTURE.md Topology); record the numbers and a go/no-go in
docs/mobile.md. Needs: an Android phone (mid-range preferred), optionally an iOS device + Mac.
