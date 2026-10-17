# openmix desktop shell (Tauri 2)

Package `openmix-app`. Thin IPC layer over `openmix-core` + `sim`;
`xair`/`dl32s` drivers attach when an address is given. No UI logic lives here —
state stays in the `Mutex<Inner>` behind the `MixerDriver` trait.

Command parity with the `openmix-serve` WebSocket API (`crates/serve`):
WS `connect`/`fader`/`mute`/`param` map to `connect`/`set_fader`/`set_mute`/
`set_param` (same number|bool|string encoding); the WS `state` push is a pull
`get_state`; the WS `meters` tick is a pull `get_meters`; the WS per-tick
`spectrum`/`gr` frames are pulls `get_spectrum`/`get_gr` (same 64 bins,
same first-16-channel cap — but poll-based: the UI polls at 5 Hz since
invoke has no push). Beyond the WS API,
the shell owns a server-side scene/show library and the main fader.

## Commands

- `connect(driver, addr)` — `driver`: `sim|xair|dl32s`. `sim` ignores `addr`;
  `xair`/`dl32s` require a LAN address and open a real session (async trait,
  static `AnyDriver` dispatch — same shape as `openmix-cli`). Clears the meter
  cache; the scene library carries over.
- `set_fader(ch, value)` — `ch` 0-based strip, `value` 0.0–1.0 (validated;
  range errors come from the driver for unknown strips).
- `set_mute(ch, muted)`.
- `set_param(ch, field, value)` — `value` is number|boolean|string, mapped to
  `ParamValue::Float|Bool|Text` exactly like the WS `param` command
  (`"fader"`/`"mute"`/`"name"` dispatch to the strip setters).
- `get_state` — full mixer snapshot: `{driver, connected, strips:
  [{ch, name, fader, muted}], main_fader}` (`main_fader` is `null` when the
  driver has none; snapshot failure yields empty `strips`, like `serve`).
- `set_main_fader(value)` / `get_main_fader()` — 0.0–1.0, driver-backed
  (sim persists 0.8 default).
- `capture_scene(name)` — snapshot live state into the server-side show
  library (upsert; includes main fader when the driver has one). Returns
  `{name, strips, main_fader}`.
- `recall_scene(name, fade_ms?)` — fader fade over `fade_ms` (default 400;
  0 jumps). Pre-unmutes scene-unmuted strips, snaps to exact targets, then
  applies mutes and main.
- `list_scenes()` / `get_scene(name)` / `delete_scene(name)` — library ops;
  `get_scene` returns `{name, main_fader, strips: [{ch, name, fader, muted}]}`.
- `get_meters` — live `driver.poll_meters()` frame, throttled: fresh frame at
  most every 50 ms, faster polls get the cached frame. Fresh frames also feed
  the shared RTA history behind `get_spectrum` (return shape unchanged).
- `get_spectrum(ch)` — 64-bin spectrum for one channel (`ch` 0-based, first
  16 only): `{ch, bins}` mirroring the WS `spectrum` frame (same
  `RtaProcessor`: trailing 100 ms of meter history, mean-removed,
  zero-padded to 128, peak-normalized; empty `bins` past the live strip
  count). Poll-based — the UI polls at 5 Hz (200 ms), the same rate serve
  emits `gr` frames; invoke has no push.
- `get_gr(ch)` — gate/comp gain reduction for one channel (`ch` 0-based,
  first 16 only): `{ch, gate, comp}` (dB reduction `>= 0`) mirroring the WS
  `gr` frame (same `gate_gr`/`comp_gr` reads: xair DYN cache, 0.0 fallback
  when the cache hasn't filled; sim/dl32s always 0.0). Poll-based — the UI
  polls at 5 Hz like serve's every-second-tick `gr` push.

## Run desktop

```sh
cd src-tauri
cargo install tauri-cli --version '^2'   # or: npx -y @tauri-apps/cli
cargo tauri dev
cargo tauri build                        # unsigned local bundle; no signing configured
```

Frontend is `../../apps/web/dist` (Svelte build): 8+ strips from `get_state`,
driver dropdown, scenes backed by `capture_scene`/`recall_scene`, meter bridge
polled via `get_meters`.

> If `apps/web/dist` is missing (never built): run `npm run build` in
> `apps/web` (needs `node_modules`), or temporarily point `frontendDist` back
> at `../ui` (legacy vanilla JS UI, faders/mutes/meters only — no scenes or
> main fader).

## Mobile (later)

```sh
cargo tauri android init   # then: cargo tauri android dev
cargo tauri ios init       # then: cargo tauri ios dev
```

Needs Android Studio / Xcode + a real device on the mixer LAN (touch-fader
latency must be measured on hardware). No mobile targets are configured yet.

## Why browser-direct UDP is impossible

Browsers cannot open raw UDP sockets, and the mixers speak UDP (X-Air: OSC on
10024; DL32S: Master Fader protocol). So a web page can never talk to the desk
directly. Options: this Tauri shell (sockets in Rust, Svelte UI) or the
LAN bridge — Rust UDP driver + WebSocket relay (`openmix-serve`) that
the browser page connects to.

## Status — full engine (commands + Svelte dist); desktop compile still blocked on system libs

`src/main.rs` implements the full engine: generic `set_param`, pull
`get_state` (all strips incl names/mutes, plus main), server-side
scene/show ops (`capture_scene`/`recall_scene`/`list_scenes`/`get_scene`/
`delete_scene`), main fader (`set_main_fader`/`get_main_fader`), live
driver meters via `poll_meters` (the stub's synthesized meters and 8-channel
cache are gone), and pull `get_spectrum`/`get_gr` mirroring the serve
`spectrum`/`gr` ticks (same 64 bins, first-16-channel cap, `RtaProcessor`
history shared with `get_meters` polls, same DYN-cache reads — reset on
`connect`, like serve's per-connection RTA). `tauri.conf.json` points `frontendDist` at
`../../apps/web/dist` (see fallback note above). The driver-facing logic
type-checks: verified by compiling the identical enum + command bodies
against the workspace crates in a scratch harness (`cargo check` clean plus
a runtime smoke: connect → fader/mute/param → get_state asserts → main
fader → capture/recall/delete scene → meters non-empty → spectrum 64 bins
in [0,1] with the 16ch cap → gr idle zeros with the 16ch cap → reconnect
resets the RTA).

What remains unverified is the Tauri macro/API surface itself, because the
crate never reaches type-check here:

`cargo check --manifest-path src-tauri/Cargo.toml` (2026-10-17,
cargo 1.99, tauri-cli 2.12.3) fails in `libdbus-sys-0.2.7/build.rs`:

> pkg-config exited with status code 1 … Package dbus-1 was not found …
> The system library `dbus-1` required by crate `libdbus-sys` was not found.

Fix: `sudo apt install libdbus-1-dev pkg-config` (Ubuntu) — then expect the
usual Tauri Linux set next (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
`librsvg2-dev`). Workspace itself is unaffected:
`cargo check -p openmix-core` passes (`src-tauri` is in workspace `exclude`).
