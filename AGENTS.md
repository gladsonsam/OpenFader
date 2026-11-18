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
- `app/lib/{theme.dart,design.dart,u4law.dart}` — shared geometry/type (`U4`/`AppDesign`),
  fader law. Never hardcode colors; never touch the fader law casually
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

- One screen at a time: implement, verify on **both** simulators
  (iPad Pro 13-inch + iPhone 18 Pro), screenshot-compare against the design
  mock at its specified dimensions, then commit.
- For iOS simulator checks, build with
  `flutter build ios --simulator --debug` in `app`, then use `xcrun simctl`
  to install, launch and capture screenshots. Use equivalent platform
  tooling for other targets.
- The app boots into the Connections start screen; use Work offline to
  reach the mixer on sims.
- For emulator driver work, use `scripts/mixer-lab/lab.py` and the workflow in
  `scripts/mixer-lab/README.md`. It runs pinned Sidecar fixtures and saves
  independent command/readback evidence. Stop lab emulators before repository
  tests because discovery tests use the same ports. Keep missing integrations
  blocked in its matrix; emulator passes do not establish hardware support or
  replace the existing gates and simulator checks.

## Gates (binding)

- `flutter analyze` in `app` must be clean — no exceptions.
- `python3 scripts/ui_guard.py check` must pass. Screens use the `ui/` library
  (ConsoleButton variants, ConsoleDialog helpers, ConsoleText roles, palette).
  After removing violations run `update` to lower the baseline; never raise it.
- `cargo test --workspace --locked` must stay green. It guards the desk
  protocol; do not expand it without being asked.
- Do NOT write frontend tests (widget/interaction/screenshot-diff suites)
  and do not gate on them. MVP check is: analyze clean + app boots +
  connects on sims. This lifts only at the owner's word.

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
  Record physical model/firmware and both required simulator identities where
  applicable. Keep unavailable hardware/emulators/simulators blocked; existing
  gates and the prohibition on frontend suites remain binding. Check a proposed
  completion with `integration_matrix.py --evidence <records.json> check
  --complete <model>:<feature>` before claiming it is done.
