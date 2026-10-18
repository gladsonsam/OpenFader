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

## Capability parity (rule)

Every frontend (browser via the relay, desktop and mobile via Tauri, CLI, future apps) must be able to
use the FULL capability of the mixer. Therefore all behaviour lives in ONE transport-agnostic engine
crate (`crates/engine`): driver sessions, state/param cache + poller + focus, meters/spectrum/GR,
scenes/shows, locks, roles/access policy, client roster, capabilities. The engine speaks exactly one
protocol: `Cmd` in, `Event` out (the JSON frames documented in `crates/engine`).
`crates/serve` (WebSocket + static files) and `src-tauri` (IPC) are thin ADAPTERS that only move
`Cmd`/`Event` JSON; they contain no mixer logic. The UI has one `Backend` over a tiny `Transport`
(WS or Tauri IPC) that carries the same JSON. A feature that is not in the engine does not exist.
Guard: golden frame files shared by the Rust tests and the TypeScript parser tests.

## Topology: one engine owns each desk connection

The engine is an embeddable library with three first-class ways to reach it: in-process (Tauri IPC for
the desktop/mobile apps; default, no listening port), network (`openmix-serve`: WebSocket + static
files, headless venue server), and direct embedding (CLI, MCP server). Rule: exactly ONE engine owns a
desk connection (a desk limits remote clients and each engine runs its own poller). Everything else
ATTACHES to the owner over the same `Cmd`/`Event` protocol: a standalone app owns its desk; in a venue
with a server, the server owns it and apps/CLI/MCP/browsers attach (shared state, locks, roles); a CLI
or MCP with no server running embeds the engine and becomes the owner. Attached tools run under the
engine's role policy (e.g. MCP as a restricted role: capped faders, no scene recall without confirmation).
Fallback if mobile IPC throughput measures badly (MP0 spike): the native app starts the engine on
127.0.0.1 and the webview uses the same WebSocket transport as the browser.
