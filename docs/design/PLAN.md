# UI "Design Faithful" plan: build the frontend exactly like `mixing-station-mixer-eq.html`

Source of truth (all in docs/design/): `mixing-station-mixer-eq.html` (the interactive design; open it over http),
`shots/01..10-*.png` (1280x720 reference screens), `src/design-template.html` (the design's markup with every
inline style: exact colours, sizes, spacing), `src/design-logic.js.txt` (its component logic: state machine,
seeded data, layouts), `src/Geist-latin.woff2` (the font, SIL OFL).
Screens: 01 Mixer, 02 Channel>HPF+EQ, 03 Channel>Compressor (Gate is the same layout), 04 Channel>Sends,
05 Channel>Input, 06 FX, 07 Routing, 08 Connection, 09 Settings, 10 Scenes (+ locked state, sends-on-fader,
fine mode, DCA / Main banks).

## Decisions (flag any you disagree with)
1. This design REPLACES the v3 visual layer. Reuse the logic: engine protocol, `lib/backend.ts`,
   `v3/state/paramStore`, faderLaw, meter pipeline, channel EQ math (`v3/view/channelLogic.ts`). Old v3 views stay until
   cutover then are deleted.
2. New tree: `apps/web/src/ui/` (entry `?ui=4`). Screens are reachable in deterministic FIXTURE MODE by URL:
   `?ui=4&fixture=1&freeze=1&screen=<mixer|ch-input|ch-eq|ch-gate|ch-comp|ch-sends|fx|routing|connection|settings|scenes>`
   (+ `&ch=2`, `&bank=ch1`, `&locked=1`, `&sof=1`, `&fine=1`). Fixture mode uses the design's own seeded data and
   frozen meters, no backend, so every screen can be built and pixel-diffed in isolation and in parallel.
3. "Exactly" is measured: a pixel-diff harness compares our screen to the design at 1280x720 (DPR 1) with animated
   regions masked. Threshold: <= 1.0% of unmasked pixels differ by more than 24/255 in any channel, per screen, and no
   layout box off by more than 1px (box check via DOM measurement against the design's boxes for key elements).
4. The design is a landscape tablet/desktop design. Other sizes (phone, portrait) are derived later from the same
   components (strips per screen, bank pager) in a separate wave; not part of exact-match acceptance.
5. Capabilities: the design shows a richer mixer than today's engine catalogue (full dynamics params, input delay, send
   pre/post, HPF slope, FX racks, routing patch, device scan, numbered scenes, DCAs, 48 channels / 16 mixes). The UI is
   built to the design against the fixtures/sim; on a real desk each control follows `caps`: unsupported controls are
   removed or collapsed (strip mini-panes collapse, tabs hide), never faked. Rust work to widen the catalogue runs in
   parallel (packages R*).
6. Font: Geist (OFL) is bundled (apps/web/public/fonts/), no CDN. Radii/sizes/colours come from the template, not from v3 tokens.
7. MODULAR (docs/design/MODULES.md, binding): the design is the DEFAULT of a module + layout + profile system. Every visual
   unit is a registered module with a manifest; every screen/page/strip is a layout JSON rendered by one generic renderer; users
   override via profiles (layouts, banks, strip anatomy, themes, behaviour), with an Edit mode, import/export, and an extension API.
8. Realtime rules (SPEC 2a) apply to every component: local-first faders, canvas meters (non-reactive), no reactive hot paths.

## Work packages (file ownership is strict; an agent edits only its own paths)
Wave 0 (start now, parallel): 
- D0 Foundations: `ui/tokens.css`, `ui/app.css`, `public/fonts/*`, `ui/AppFrame.svelte`, generic primitives
  `ui/primitives/{Button,Segmented,Toggle,Panel,IconButton,Pill}.svelte` + `ui/icons.ts` (all SVGs from the template),
  `ui/mount.ts` (`?ui=4` entry + fixture URL parsing), gallery `?ui=4&gallery`. Own: `apps/web/src/ui/{tokens.css,app.css,AppFrame.svelte,mount.ts,icons.ts,gallery/**,primitives/{Button,Segmented,Toggle,Panel,IconButton,Pill}.svelte}`, `apps/web/public/fonts/**`, hook in `src/main.ts` (`ui=4`).
- D1 Pixel-diff harness: `apps/web/tools/design-diff/**` (capture design reference per screen deterministically; capture ours
  in fixture mode; mask JSON per screen; PIL+numpy diff; report + heatmap PNGs in /tmp; box-measure check; CLI
  `node apps/web/tools/design-diff/run.mjs --screen <id> [--update-ref]`; refs cached in `docs/design/refs/`).
- D2 Data + state: `ui/fixtures/designData.ts` (exact port of the design's seeded PRNG data: banks, channels, FX, scenes,
  devices; unit-tested against values read from the design), `ui/state/uiStore.svelte.ts` (the design's state machine:
  mode/prevMode back stack, tab, bank, sel, selBand, fine, muteEnable, sof, mix, scene, spp, peakHold, locked,
  sceneConfirm, phantomArm, fxSel, patchSel, connected...) + actions (goTo, goBack...), URL param bridge for fixture mode.
  Own: `apps/web/src/ui/{fixtures/**,state/**}`.
Wave 1 (after wave 0; parallel up to 4 at a time):
- L0 Module system: `ui/modules/**` (types, registry, schema + validators, `LayoutRenderer`, ModuleContext/bindings contract, profile store
  with merge/migrate/validate/export-import, theme token overrides) + `ui/layouts/**` skeleton; implements MODULES.md sections 1-3 and 5 against fixtures
  (renderer renders the gallery module set; default layouts arrive with the screen packages). Pure parts node-tested. Wave-1/2 screen packages
  deliver MODULES (manifest + component) and their DEFAULT LAYOUT JSON, not hard-coded screens.

- P1 Signal primitives: `ui/primitives/{Fader,Meter,Knob,MuteButton,PanBar,NameBar,ScaleLabels}.svelte` pixel-matched
  (the metal fader cap with grip lines + white line, scale, green meter with red clip block and peak line, ring knobs in
  channel/colour accents, pan bar).
- H1 Header + shell: top bar (title/back/channel chip/lock banner/FX/Routing/Connect/Settings/scene dropdown), bank rail
  (right), screen router outlet. Own `ui/shell/**`.
- S7 Routing, S8 Connection, S9 Settings, S10 Scenes screens (static layouts, only generic primitives): `ui/screens/{routing,connection,settings,scenes}/**`.
Wave 2 (after P1 + H1): M1 Mixer screen + main strip + banks (`ui/screens/mixer/**`); C1 Channel shell + Input + Sends tabs + master strip
  (`ui/screens/channel/{ChannelScreen,InputTab,SendsTab,MasterStrip}.svelte`); C2 EQ tab (graph + 5 band cards + HPF)
  (`ui/screens/channel/eq/**`, reuse `v3/view/channelLogic.ts` math by import, do not edit it); C3 Gate+Compressor tab
  (transfer-curve graph + knobs + GR) (`ui/screens/channel/dyn/**`); F1 FX screen (`ui/screens/fx/**`).
Wave 3 (engine binding, sequential per area to avoid conflicts): W1 bind Mixer + master strip + header/scene to the engine
  (store <-> `lib/backend` + paramStore + caps collapse rules); W2 bind Channel tabs; W3 bind scenes/connection/fx/routing.
Rust, parallel to the UI waves but AFTER MR1 lands (shared files). Spec: CATALOGUE-V2.md (binding). R1 topology + catalogue v2 + drivers
  (strip families, colour/icon/link, full gate/comp, delay, HPF slope, generic sends with tap/pan/on, FX metadata, meter taps; sim implements all, xair
  UNVERIFIED-ON-RIG, dl32s honest); R2 engine commands/events: device scan + connect + latency, offline editing, numbered full-plane scenes, FX rack, routing patch, info.
Wave 4: Edit mode UI (E1: in-place layout editing, module palette, inspector, undo/redo), theme editor + presets (T1), banks/pages/strip-anatomy
  editors (B1), profile import/export UI + engine-served profiles (R3), realtime hookup (MR2), phone/portrait variants, a11y, perf, cleanup/cutover.

## Verification (every package)
Pure logic: node tests (`node --experimental-strip-types x.test.ts`). Visuals: `node apps/web/tools/design-diff/run.mjs
--screen <id>` score under threshold, heatmap read by the implementer. Reviewer re-runs the diff and looks at the screenshots.

## Resource rules
Box has 14 GB shared: at most 4 agents with a headless browser at once; cargo one command at a time; no builds/svelte-check;
dev server for this effort: one shared vite on :5291 (already running); never start another; `touch` a file if it serves stale output.

## Wave 3 detail: bind the finished screens to the real engine (written after Wave 2 results)
Facts: every screen exists as a pixel-verified UI on fixtures (diffs 0.0-0.45%). The screens read the D2 store (`ui/state/uiStore.svelte.ts`) which today holds
fixture data (`designData` channels, scenes, devices, fx, patch). Engine side (R1/R2): topology (families), 149-id catalogue with caps, params/focus,
scenes (full plane), devices/link/info, fx, routing, offline, meter taps, binary meters. Binding must not change a pixel in fixture mode.
- W1 WORLD INTERFACE (refactor, sequential, touches the shared store: run when no other UI package is editing it): `ui/world/types.ts` = `World`
  (strips by family/bank, per-strip name/colour/source, param get/set/known/pending, caps + `supports(id)`, meters (typed arrays), scenes, fx, routing,
  devices/link/info, offline, focus). `FixtureWorld` wraps the existing designData/meterSim; uiStore/uiLogic actions call the World instead of mutating
  fixture objects. ALL diffs must stay unchanged (0.00 regression), all state tests (161) and parity (190) pass. Fixture mode (`fixture=1`) always uses FixtureWorld.
- W2 ENGINE WORLD: `EngineWorld` over lib/backend (add a small `Transport` abstraction: WS now, Tauri IPC later, per ME2), hello {binary_meters, taps},
  caps/topology -> banks (family -> bank rail buttons, colours from catalogue `color`/design bank colours), params/pending/confirm/revert (reuse the proven v3
  paramStore logic), focus on visible strips and the open channel, meters decoded into the shared typed arrays, scenes/fx/routing/devices/offline commands,
  link status -> header dot. Honest UI: unknown (not yet received) => dimmed read-only; unsupported by caps => the module's `fallback` (strip mini panes collapse and the fader grows, tabs hide,
  settings that need a capability disappear), never faked. Selecting the world: `?ui=4&server=host:port` (engine) vs `fixture=1`.
- W3 CAPS-DRIVEN COLLAPSE + FIT: strip modules honour `requires/fallback`; height/width fit rules (SPEC 6a idea: drop optional panes when short) as layout rules in data; tests with
  dl32s-like caps (fader/mute/name only) and xr18-like caps; pixel diffs with FULL caps unchanged.
- W4 REALTIME HOOKUP (MR2): leading-edge write sender + local-first fader (already in P1), binary meters/GR/taps straight into typed arrays, `?perf=1` overlay, latency harness
  (apps/web/tools/latency.mjs): numbers within SPEC 2a budgets against the sim relay under load.
- W5 CONVERSION: after L1, convert every screen from plain composition to layout JSON + renderer (pixel diffs unchanged), one agent per screen group.
- Wave 4: Edit mode + editors (layout, strip anatomy, banks, pages, themes), profile import/export + engine-served profiles, phone/portrait variants (derived layouts), a11y, perf, ME2 (single Transport,
  Tauri adapter), MP0/MP (mobile), cutover (flip default to ui=4, delete old UI) = owner's decision.
