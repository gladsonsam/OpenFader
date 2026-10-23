# OpenFader agent rules

Flutter mixer app (`apps/app`) over an embedded Rust engine (`crates/`).
Brand is **OpenFader**, one word. License GPL-3.0-or-later.

## Layout

- `apps/app/lib/main.dart` — entry; `BootGate` runs staged boot, then `AppShell`.
- `apps/app/lib/boot/` — `boot_controller.dart` (staged boot), `launch_screen.dart`,
  `fake_sim_engine.dart` (offline sim backend), `ffi_engine.dart` (real engine).
- `apps/app/lib/shell/app_router.dart` — navigation + all store implementations.
- `apps/app/lib/screens/` — `start.dart` (Connections front door), `channel.dart`
  + `channel_toolbar.dart`, `connection.dart`, `fx/meters/routing/scenes/settings/debug.dart`.
- `apps/app/lib/mixer/` — mixer `page.dart`, `controller.dart`, `store.dart`,
  shared tokens in `design.dart`, widgets in `components/`.
- `apps/app/lib/state/app_store.dart` — the single store (engine reads, `sendRaw` writes).
- `apps/app/lib/{theme.dart,design.dart,u4law.dart}` — shared tokens (`U4`/`AppDesign`),
  fader law. Never hardcode colors; never touch the fader law casually
  (protocol values depend on it).
- `apps/app/lib/engine.dart` — `Cmds` protocol + `EngineState`. `FakeSimEngine`
  must speak the same protocol as the native engine.
- `apps/app/assets/brand/` — approved logo SVG/PNGs + opaque app icon source.
- `crates/` — `engine core transport flutter_bridge proto-xair proto-dl32s sim serve cli`.
- `scripts/` — `ci-build-app.py`, `flutter-rust-ios.sh`, `flutter-rust-macos.sh`
  (Xcode build phases), `sync-version.mjs` (keeps `VERSION` + manifests in sync).
- `.github/workflows/ci.yml` — version check, `cargo check/clippy/test`, `flutter analyze`.
- Root `README.md` is the product one-liner. Keep the root clean: no new
  top-level docs files or directories without asking.

## Toolchain

- Flutter 3.47.7 at `~/flutter-3.47/bin` — not on `PATH`; export it per command.
  CI pins the same version.
- Rust stable workspace (`cargo test --workspace` from root). `gh` CLI is
  authenticated for GitHub ops. No global git identity is configured —
  commit with repo-local `-c user.name/user.email` flags.
- Node 24 for `scripts/sync-version.mjs`.

## Working loop

- One screen at a time: implement, verify on **both** simulators
  (iPad Pro 13-inch + iPhone 18 Pro), screenshot-compare against the design
  mock at its specified dimensions, then commit.
- Simulators: build with `flutter build ios --simulator --debug` in
  `apps/app`, install/launch via simctl or the MobileBuildMCP tools,
  capture with `xcrun simctl io <udid> screenshot`.
- The app boots into the Connections start screen; use Work offline to
  reach the mixer on sims.

## Gates (binding)

- `flutter analyze` in `apps/app` must be clean — no exceptions.
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
