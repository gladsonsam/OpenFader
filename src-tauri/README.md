# openmix desktop shell (Tauri 2 stub)

Package `openmix-app`. Thin IPC layer over `openmix-core` + `sim`;
`xair`/`dl32s` drivers attach when an address is given. No UI logic lives here —
state stays in the `Mutex<Inner>` behind the `MixerDriver` trait.

## Commands

- `connect(driver, addr)` — `driver`: `sim|xair|dl32s`. `sim` ignores `addr`;
  `xair`/`dl32s` require a LAN address and open a real session (async trait,
  static `AnyDriver` dispatch — same shape as `openmix-cli`).
- `set_fader(ch, value)` — `ch` 0–7, `value` 0.0–1.0.
- `set_mute(ch, muted)`.
- `get_meters` — throttled: fresh frame at most every 50 ms, faster polls get
  the cached frame. Stub synthesis: `fader * (0.6 + 0.4·sin)`, muted → 0.

## Run desktop

```sh
cd src-tauri
cargo install tauri-cli --version '^2'   # or: npx -y @tauri-apps/cli
cargo tauri dev
cargo tauri build                        # unsigned local bundle; no signing configured
```

Frontend is `../ui/index.html` (vanilla JS, 60 lines): 8 faders + mutes,
driver dropdown, addr field, connect button, canvas meters polled at 100 ms.

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
directly. Options: this Tauri shell (sockets in Rust, canvas UI) or a future
LAN bridge — Rust UDP driver + WebSocket relay (`transport` crate stub) that
the browser page connects to. The bridge is future work, not this stub.

## Status — async-trait migration done, desktop compile still blocked on system libs

`src/main.rs` now targets the async `MixerDriver` trait (static `AnyDriver`
dispatch; `connect`/`set_fader`/`set_mute` are `async` Tauri commands). The
driver-facing logic type-checks: it was verified by compiling the identical
enum + command bodies against the workspace crates in a scratch harness
(`cargo check` clean, only dead-code warnings). What remains unverified is the
Tauri macro/API surface itself, because the crate never reaches type-check here:

`cargo check --manifest-path src-tauri/Cargo.toml` (2026-10-17,
cargo 1.99, tauri-cli 2.12.3) fails in `libdbus-sys-0.2.7/build.rs`:

> pkg-config exited with status code 1 … Package dbus-1 was not found …
> The system library `dbus-1` required by crate `libdbus-sys` was not found.

Fix: `sudo apt install libdbus-1-dev pkg-config` (Ubuntu) — then expect the
usual Tauri Linux set next (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
`librsvg2-dev`). Workspace itself is unaffected:
`cargo check -p openmix-core` passes (`src-tauri` is in workspace `exclude`).
