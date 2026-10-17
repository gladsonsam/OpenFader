# openmix architecture (X-Air + DL32S only)

Targets: Behringer X-Air (XR12/16/18, MR18) + Mackie DL32S. Owner has both.
Stack: max Rust. Tauri 2 for desktop + mobile. Web via LAN bridge (browsers can't do raw UDP).

## Crates
- `core`: UI-free state. `MixerDriver` trait, `StripId`, `MeterFrame`, scene snapshot, offline store.
- `transport`: tokio UDP/TCP, mDNS discovery, keepalive, meter ring-buffer (50ms), WS relay for browser.
- `proto-xair`: OSC UDP 10024 codec. Known-good, documented. Build first.
- `proto-dl32s`: Master Fader protocol. Closed, needs black-box capture. Build second.
- `sim`: offline fake mixer implementing `MixerDriver` for UI dev without hardware.

## Rules
- `core` never touches UI or Tauri. All sockets in Rust backend.
- Meters: Rust task -> broadcast channel -> throttled Tauri emit -> canvas.
- Capture hygiene: own LAN only, `docs/*-protocol.md` spec first with FW versions, code from spec only. No decompile, no copied UI/assets, no license bypass.
- License: GPL-3.0-or-later (see `LICENSE`).

## MVP order
1. X-Air: discovery, connect, fader/mute/pan, meters, naming, scene snapshot, sim.
2. DL32S: capture plan -> PROTOCOL.md -> driver -> same UI.
3. Tauri desktop first, then `tauri android/ios dev` on real devices early (touch fader latency).
4. Later: other vendors, FX/RTA deferred.

See `docs/xair-protocol.md`, `docs/dl32s-protocol.md` (pending fill).
