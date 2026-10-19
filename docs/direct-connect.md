# Direct connect: push fan-out, mobile LAN, updater, CSP

Status: implemented 2026-10-19 (workstream 4). Engine core untouched —
all changes are in `src-tauri/`, `apps/web` transport/`mobile.ts`, and
this doc. The `crates/serve` WS relay is byte-identical for browsers.

## 1. Push fan-out (Tauri emit, no more IPC polling)

Before: the webview polled `engine_poll` at ~10 Hz while the meter
ticker alone runs at 20 Hz — a constant IPC wakeup that costs latency
(push waits for the next poll) and battery on phones.

Now: the shell pushes. `src-tauri/src/lib.rs` emits every drained batch
as the `openmix-event` Tauri event (payload: array of `Event::to_json`
strings — the exact texts the WS relay puts on the socket):

- `engine_cmd` / `engine_poll` drain, then emit inline when a client
  subscribed (and still return the frames — see fallback).
- A 50 ms background ticker task drains ticker frames (meters/spectrum/GR,
  poller, discovery) that no command produced.
- `engine_subscribe_push` arms it (idempotent, spawns the ticker once).

`TauriTransport` (`apps/web/src/ui/world/transport.ts`) listens for
`openmix-event` after `engine_attach` and *before* the first `send`
(subscribe-then-hello ordering, so no frame is both pushed and returned
and the attach snapshot cannot fall between the two paths). In push mode
`engine_cmd` return values are ignored; the poll timer never starts.

Fallback: `engine_poll` is unchanged on the backend, and the transport
reverts to the old `engine_poll` loop (default 100 ms, `pollMs`
overridable) whenever the event API is unavailable — node tests with an
injected `invoke`, foreign shells. `listen: null` forces fallback.

Permissions: frontend `listen` needs `core:event:allow-listen`, granted
via `core:default` in both `capabilities/default.json` and
`capabilities/mobile.json` — no capability change was needed for push.

## 2. Mobile LAN audit (mixer UDP)

Mixer I/O is raw UDP sockets in Rust (`crates/transport`, drivers), not
a Tauri plugin, so the Tauri ACL is not involved — but the OS still
requires declarations, and discovery has to survive venue networks.

### Capabilities

| File | Platforms | Grants |
| --- | --- | --- |
| `capabilities/default.json` | all | `core:default`, `os:default`, `store:default`, `log:default` |
| `capabilities/mobile.json` | `android`, `iOS` (verified against `tauri-utils` `Target`) | `core:default` + haptics / keep-screen-on / edge-to-edge |
| `capabilities/desktop.json` (new) | `linux`, `macOS`, `windows` | `updater:default` (updater has no mobile backend) |

### Android (`src-tauri/gen/android/…`, generated — re-apply after init)

`app/src/main/AndroidManifest.xml` now declares, inside `<manifest>`:

```xml
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
<uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
```

All three are normal-class (auto-granted, no runtime prompt).
Deliberately absent: `CHANGE_WIFI_MULTICAST_STATE` (no multicast
traffic — discovery is UDP broadcast with unicast replies),
`ACCESS_FINE_LOCATION` (no SSID scans). `gen/` is git-ignored: after
`tauri android init` on a tooled machine, re-add the two lines and
confirm `android:usesCleartextTraffic` is debug-only.

### iOS

`src-tauri/Info.ios.plist` (auto-merged by the bundler — no
`tauri.conf.json` change needed) sets:

```xml
<key>NSLocalNetworkUsageDescription</key>
<string>openmix connects to your mixer (X-Air / DL32S) on the local network.</string>
```

iOS 14+ shows this prompt on first UDP use; without the key local
sockets fail silently. No multicast entitlement (no mDNS/bonjour
discovery) and no `NSBonjourServices` (none planned).

### Subnet broadcast + unicast probe flow

`crates/transport/src/discovery.rs` sends every probe to the limited
broadcast (`255.255.255.255`) **plus** each interface's directed
broadcast (`subnet_broadcasts()`), because phone WiFi stacks and venue
APs each swallow one or the other. Replies are unicast — no extra
permission, no multicast lock.

Cross-subnet desks never answer broadcast, so the UI flow degrades
gracefully (all three ride the same engine commands over IPC, no
transport work needed):

1. `scan` (`cmdScan`) — broadcast fan-out, results in the `devices`
   frame (Connection screen list).
2. Empty list on client-isolated APs / other subnets → manual add-by-IP
   (`ManualDesk.svelte` → `cmdProbeDesk(addr)`) — `probe_desk` sends a
   directed **unicast** `/xinfo` to that IP and reports the desk as
   `via: manual` (cross-subnet verified path).
3. `connect_device` (`cmdConnectDevice(id)`) — connects the selected row,
   including unicast-verified and linked (`via`) desks.

Venue AP checklist: prefer an AP/SSID with client isolation **off**
(broadcast discovery needs it); when the venue won't change it, use
manual add-by-IP — that path needs only L3 reachability, no broadcast.

## 3. Updater scaffold + CSP

- Plugin: `tauri-plugin-updater = "2"`, registered in `run()` via
  `Builder::new().build()`. Config lives in `tauri.conf.json`
  (`plugins.updater`): empty `endpoints` + a `TODO` pubkey placeholder —
  the committed state checks nothing.
- First-run behaviour today: 10 s after boot the shell builds the
  updater (empty endpoints → `EmptyEndpoints`, logged, no traffic) and
  otherwise logs the result. A found update is announced as the
  `openmix-updater` event (`{ version, current_version }`) — the shell
  never installs or restarts on its own (mid-gig restarts are the
  operator's decision). Frontend: `onUpdaterAvailable()` in
  `apps/web/src/mobile.ts`; no UI consumes it yet.
- Going live: `npx tauri signer generate`, paste the public key into
  `plugins.updater.pubkey`, point `endpoints` at the release manifest,
  set `createUpdaterArtifacts` for the shipped bundle targets, then
  build the download/install UI behind `onUpdaterAvailable()`.
  Signing/keys are owner-side (see `docs/mobile.md` §6 pattern).

- CSP (`app.security.csp`, was `null`): minimal allowlist for a local
  app that talks to a LAN —
  `default-src 'self' customprotocol: asset:; script-src 'self';`
  `style-src 'self' 'unsafe-inline'; img-src 'self' asset: data: blob:;`
  `font-src 'self' data:; media-src 'self' blob: data:;`
  `connect-src 'self' ipc: ws: wss: http://localhost:* https://localhost:* http://127.0.0.1:*;`
  `object-src 'none'; frame-src 'none'; base-uri 'self'; form-action 'none'`.
  Tauri IPC rides the internal bridge, not fetch, so `invoke`/`listen`
  are unaffected; `ws:`/`wss:` cover `?server=` LAN relays opened from
  dev/PWA contexts and vite dev HMR. No iframes/forms exist in the UI,
  hence the `none` directives — re-audit if one is ever added.

## 4. Verification

- `cargo test --workspace` from the repo root (engine/transport/serve;
  engine core untouched by this workstream).
- `cargo test --lib` from `src-tauri/` (`src-tauri` is excluded from the
  workspace; covers the `engine_ipc_tests` round-trip plus the new
  `drain_and_push` regression test).
- `npm run build` in `apps/web` (typecheck + bundle with the new
  transport/mobile wiring).
- `cargo build -p openmix-serve` (relay untouched, still compiles).
- Node: `transport.test.ts` still passes (fallback path); push verified
  via throwaway script during development (listen+subscribe, no timer,
  no double-feed; forced fallback feeds returns + polls).
