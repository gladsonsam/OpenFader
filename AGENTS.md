# OpenFader agent rules

Flutter mixer app (`app`) over an embedded Rust engine (`crates/`).
Brand is **OpenFader**, one word. License GPL-3.0-or-later.
The product tagline is **Control your mixer from any device**. Keep it consistent
across the README, website, package descriptions and GitHub About. Marketing
copy must be direct and factual; avoid metaphors, jokes and lifestyle slogans.

## Layout

- `app/lib/main.dart` — entry; `BootGate` runs staged boot, then `AppShell`.
- `app/lib/boot/` — `boot_controller.dart` (staged boot), `launch_screen.dart`,
  `fake_sim_engine.dart` (offline sim backend), `ffi_engine.dart` (real engine).
- `app/lib/shell/app_router.dart` — navigation and the single shell poll loop;
  sibling `*_store.dart` adapters implement the screen contracts.
- `app/lib/screens/` — `start.dart` (Connections front door), `channel.dart`
  + `channel_toolbar.dart`, `connection.dart`, `fx/meters/routing/scenes/settings/debug.dart`.
- `app/lib/mixer/` — mixer `page.dart`, `controller.dart`, `store.dart`,
  shared tokens in `design.dart`, widgets in `components/`.
- `app/lib/state/app_store.dart` — composition of the single store; sibling
  `app_store_*.dart` parts separate session, strips, processing, appearance,
  telemetry, destinations, libraries and presets by responsibility.
- `app/lib/core/output_*.dart` — injected output processing/preset contracts;
  output screens and widgets consume provider descriptors.
- `app/lib/core/mixer_provider_policy.dart` — typed neutral contracts and named
  provider policies consumed by generic state and screens.
- `app/lib/palette.dart` — immutable `ConsolePalette` theme extension. Resolve
  with `ConsolePalette.of(context)` in widgets; pass snapshots into painters and
  compare them in `shouldRepaint`. UI colors must not be global mutable state.
- `app/lib/{theme.dart,design.dart,u4law.dart}` — theme build and
  `ChannelIdentityColors` (tape/bank), shared geometry/type (`AppDesign`),
  fader law. `ConsoleDensity` (`ui/density.dart`) carries the large-targets
  setting; resolve with `ConsoleDensity.of(context)`, never a static. Never hardcode colors; never touch the fader law casually
  (protocol values depend on it).
- `app/lib/engine.dart` — `Cmds` protocol + `EngineState`. `FakeSimEngine`
  must speak the same protocol as the native engine.
- `app/assets/brand/` — approved logo SVG/PNGs + opaque app icon source.
- `crates/` — `engine core audio transport flutter_bridge proto-xair proto-dl32s sim serve cli`.
- `crates/engine/src/engine/providers/` — driver-owned scene and selector
  adapters, including their validation, writes and readback.
- `crates/engine/src/provider_library_validation.rs` — registered native archive schemas.
- `crates/engine/src/drivers.rs` — central driver registry and hooks, also consumed by CLI.
- `crates/proto-dl32s/src/lib.rs` — driver composition; sibling modules own
  lifecycle, inputs, assignments, preamps, metadata, telemetry, FX, destinations
  and output processing. Keep protocol mappings beside their owning module.
- `scripts/` — `ci-build-app.py`, `flutter-rust-ios.sh`, `flutter-rust-macos.sh`
  (Xcode build phases), `sync-version.mjs` (keeps `VERSION` + manifests in sync).
- `.github/workflows/ci.yml` — version check, `cargo check/clippy/test`, `flutter analyze`.
- Root `README.md` is the product one-liner. Keep the root clean: no new
  top-level docs files or directories without asking.

## Toolchain

- Use the Flutter and Rust versions pinned by CI. Install them with your
  platform's normal tooling and make them available on `PATH`.
- Node 24 for `scripts/sync-version.mjs`.

## Working loop

- One screen at a time: implement, visually verify in the running app, then
  commit. On Linux, the Linux desktop app satisfies the visual verification
  requirement. Use the viewport sizes relevant to the change and compare
  screenshots against the design mock at its specified dimensions when a mock
  is available.
- The app boots into the Connections start screen. Use Work offline to reach
  the mixer for visual checks that do not require a live connection.
- For emulator driver work, use `scripts/mixer-lab/lab.py` and the workflow in
  `scripts/mixer-lab/README.md`. It runs pinned Sidecar fixtures and saves
  independent command/readback evidence. Stop lab emulators before repository
  tests because discovery tests use the same ports. Keep missing integrations
  blocked in its matrix; emulator passes do not establish hardware support or
  replace the existing gates or visual verification.

## Visual standards (binding)

Read this before changing a screen, panel or mixer strip.

- Reserve mixer height for channel strips and live controls. Keep secondary
  labels, layout management and error details in Tools, Settings or a detail
  surface. Use the existing toolbar for a compact, actionable status indicator.
  Add a full-width row only while a task needs it, and check fader travel at
  the shortest supported window.
- Keep channel identity visible as strips get shorter. Give spare strip
  height to usable fader travel first. Add EQ and dynamics previews only
  after the available height can support them alongside the fader.
- Visual bounds and hit areas are separate. Input keeps at least
  `AppDesign.touchTarget` (44); painted content stays inside the space it is
  given. A row that cannot hold both is a layout bug: shrink the visual body,
  then drop the row, then show the action unavailable with a reason. Never let
  a border, outline, focus ring, label or minimum height grow into the next
  row or the enclosing strip edge.
- Size from the allocated box (`LayoutBuilder`), not a fixed guess. Use the
  provider's real channel count, `ConsoleDensity.of(context)` for target
  density, and the shortest relevant window. If the surface cannot fit the
  control, change the composition, not the target size.
- Inset rows from their surface. A strip that places rows itself keeps its
  `AppDesign.spaceXs` inset on both sides and its existing inter-row gap; do
  not expand a row to the divider or add a second inset. Use
  `AppDesign.panelPadding`, `controlPadding` and `fieldPadding` for surfaces
  and groups. Keep spacing equal on both sides; one-sided padding is only for a
  deliberate optical alignment.
- Use shared tokens and roles: `AppDesign.space*`, `ConsoleTextRole`,
  `ConsolePalette`, theme shape/radius, `ConsoleButton` and
  `ConsoleIconButton`. Do not add page-local numeric spacing, sizes, colors or
  font sizes when a token exists. `scripts/ui_guard.py` already rejects raw
  Material controls, dialogs, numeric font sizes, color literals and numeric
  breakpoints.
- Labels name the action or value in at most 5 words and 36 characters. Prefer
  one or two words; the surface supplies context. Shared-width labels use
  `maxLines: 1` with ellipsis, or wrap in their own lane. Move explanation to a
  subtitle, tooltip, helper line or dialog body; never a sentence in a button,
  tab, chip or strip label. The guard rejects longer literal action labels.
- Semantics are part of the layout: every action and toggle exposes its role
  and name, a disabled control exposes why it is disabled, and the visible body
  is not the only pointer target. Do not rely on color alone.
- Use icons that depict the action or object. Avoid sparkle and magic-wand
  icons, including for templates and automatic actions.
- Verify with realistic content, not placeholder strings: long channel,
  preset, destination and provider names; the narrow window; large targets on
  and off; the shortest supported height. Inspect the default window and at
  least one narrow window for every UI change.

### Linux visual capture

`scripts/visual_qa.py` launches the built Linux bundle on an isolated Xvfb
display with its own HOME/XDG state, enters the offline mixer, resizes the
window and writes cropped PNGs:

```sh
# after the usual Linux build
python3 scripts/visual_qa.py --offline
# or build the native host and bundle first
FLUTTER=/path/to/flutter python3 scripts/visual_qa.py --build --offline
```

It uses the existing local Xvfb, xdotool, scrot and dbus-run-session tools and
adds no service. Defaults: 1280x800, 1024x700 and 900x640 under
`/tmp/openfader-visual-qa/`. Add `--sizes 1280x800,700x900` for other sizes, or
omit `--offline` to capture Connections. The capture is evidence for a person
to inspect; it is not a screenshot-diff suite. If the window never appears,
check the GL/EGL stack and `app.log` rather than reporting a visual pass.

## Gates (binding)

- `flutter analyze` in `app` must be clean — no exceptions.
- `python3 scripts/ui_guard.py check` must pass. Screens use the `ui/` library
  (ConsoleButton variants, ConsoleDialog helpers, ConsoleText roles, palette).
  After removing violations run `update` to lower the baseline; never raise it.
- `cargo test --workspace --locked` must stay green. It guards the desk
  protocol; do not expand it without being asked.
- Do NOT write frontend tests (widget/interaction/screenshot-diff suites)
  and do not gate on them. MVP verification includes clean analysis, app
  launch, connection checks, and manual visual verification. This lifts
  only at the owner's word.

## State rules

- The single shell-owned 80 ms poll loop is the only meter ticker;
  pages own no timers.
- Hot path (fader/mute/pan drags) updates per-strip `ValueNotifier`s in
  place — never `notifyListeners()` there. Global notifies are for
  structural changes only (strips, banks, connection).
- Screens render through store contracts (`ChannelStore`, `ConnectionStore`);
  `AppShell` implements them over the engine. Constructor injection only —
  screens never import `state/app_store.dart`.

## Mixer independence (binding)

- Generic application, UI, state, engine, audio and shared code consumes
  provider/driver capability contracts and descriptors. Use the connected
  provider's actual channel counts, ranges and native option sets. Do not
  assume a default mixer, branch on a branded model, or bake in 18/32-channel,
  FX or other model-specific layouts in shared paths.
- Existing vendor-specific branches in shared paths are migration debt, not a
  precedent for new coupling. Migrate them toward provider contracts as the
  affected paths are changed; do not claim unverified hardware support.
- An unsupported or unknown provider must not silently inherit another mixer's
  fader law, control ranges or native options. Register and advertise the law
  and capabilities; where absent, make affected controls and readouts
  unavailable. Do not casually modify existing laws.
- Keep protocol mappings, native identifiers, quirks, discovery ports and
  validation beside the owning driver or adapter. Model selection, registry
  labels, explicitly named simulation fixtures and hardware diagnostics may
  identify a model. Adding a mixer should require a driver/provider integration,
  not rewrites of shared screens or features.
- Recording, import, export and manual virtual soundcheck must work independently
  of a network mixer, subject to actual OS driver, sample-rate and hardware
  channel limits. Snapshots, native files and routing are optional capabilities;
  automatic routing remains gated by a verified profile. When a capability is
  unavailable, expose its reason visibly. Never report fake success, invent
  mappings or rename vendor-specific behavior as generic.
- For each relevant change or audit, examine semantic coupling beyond brand
  searches, including defaults, counts, ranges, option sets and assumptions in
  shared flows. Validate against multiple differing descriptors or fixtures
  using the existing gates; do not add frontend tests or expand desk protocol
  tests without the owner's instruction.

## Integration accounting (binding)

- Keep public UI and website disclosure minimal: show the status and information
  needed to use a feature; reveal extra explanation on request. Keep engineering
  evidence, internal gates and audit methodology in developer reports. Ship only
  referenced website assets; keep design experiments outside public assets.
- Actions in the app and website must use clearly styled buttons or controls,
  never a plain underlined label. Ordinary links within prose may remain links.
  Use "Testing needed" for integrations awaiting device tests; do not imply that
  testing is actively underway by calling them "In testing".

- The feature catalogue is `scripts/mixer-lab/integration_features.json`.
  Each registered provider owns `crates/<provider>/integration.json`, including
  its model inventory and an explicit disposition for every provider feature.
  Host recording features belong to `crates/audio/integration.json`; they must
  not be repeated as mixer-dependent capabilities. Public compatibility compares
  mixers only; host recording tracking stays in developer reports. Planned or
  rejected models stay visible in `scripts/mixer-lab/integration_targets.json`.
- Before adding or changing a capability, model, driver hook or shared flow,
  inspect the affected matrix cells and update their implementation disposition,
  source ownership and acceptance criteria. A new feature needs an explicit
  disposition for every provider. New engine hooks and model identity literals
  in declared guards must be classified; do not bypass the inventory check.
- Run `python3 scripts/mixer-lab/integration_matrix.py check`. Use `report`
  for the matrix, `tasks` for open work and `task <model>:<feature>` for an agent
  brief. Shared implementation can be reused; verification never transfers
  automatically between models, firmware, emulator and physical hardware.
- Hook presence, a registered driver, a runnable emulator and a passing generic
  suite are not feature completion. Mark unreviewed behaviour, partial wiring,
  unsupported features and missing evidence explicitly. Never fill gaps with
  inferred support, invented mappings or a sibling model's defaults.
- A completed cell requires its acceptance checks and current, checksummed,
  source-bound evidence for that exact model and required verification types.
  Record the physical model and firmware where applicable, and the platform
  used for manual visual verification. Keep unavailable hardware and emulator
  evidence blocked. Existing gates and the prohibition on frontend suites
  remain binding. Check a proposed
  completion with `integration_matrix.py --evidence <records.json> check
  --complete <model>:<feature>` before claiming it is done.
