# openmix backlog (owner-spammed features land here; max 2 workers at a time)

Rules: one worker per file-area (no overlapping paths). New items go to
Queued with the owner's words. A worker is dispatched only when a slot is
free AND its files don't collide with the running worker.

## In flight (2/2 slots full)
1. APK build (`ses_eb1c6c33fffeg3UdMqs67cwO6p`) — src-tauri + builds.
   Desktop bundle done; debug APK built, final verification pending.
2. UI gaps + snapshot package (`ses_eb0dfe76dffeDSz8OhB880Fqr9`) —
   crates/engine + proto-xair + apps/web/src/ui. Deterministic-check
   removal, LR mute/solo, attach snapshot, mute-group UI, channel
   filtering, LR meter, phantom meters, FX racks, bus popup, snapshots,
   live screen bindings.

## Queued
- RTA overlay for EQ (live spectrum on the EQ graph).
- Presets: inbuilt presets + user presets (create/save/recall).
- Global meter bridge (X-Air Edit Meters pattern: Analog Inputs 1-16+17/18,
  Effects Sends 1-4, FX Returns, Bus Outputs 1-6, Mon/Solo, Main L/R, USB
  Returns, P16 sends; grouped ladders 0..-50) with an icon button in the
  topbar to open it.

## Queued — server-side (later; venue PC on LAN + USB/ASIO)
- Automixing (gain-sharing across channels on the server).
- Feedback removal (auto-EQ driven by RTA).
- MCP server (agents operate the mixer).
- CLI scripting (openmix-cli exists — extend to full coverage for scripts).

## Done (this wave)
- ui4 is the only UI; desktop diffs held; phone responsive layout.
- Behringer spec match to XR18 FW 1.20 (65 live checks, docs rewritten).
- Rust consolidation (single engine/protocol, 283 tests green).
- Desktop bundles (.deb/.rpm/AppImage+binary) built + verified.
- Docker image (22/22 smoke).
- XR18 hot-reload dev setup (`scripts/dev-xr18.sh`, relay :8081).
- Bidirectional live sync (desk→app 2–197 ms proven).
- Connect-sim flip fix (relay stays on desk).
- Load fix (~20 s → ~2.7 s to live faders).
- mixosc audit (ranked takeaways) + clean-room implementation
  (catch-all inbound, mute groups /config/mute/1..4, clearsolo,
  fast-connect sweep; keepalive unchanged on evidence).
- Engine write-through (fader/mute/pan/sends move the desk, restored).
- Full desk-state surfacing (names/colors/groups badges, groups card).
- Repo rules: AGENTS.md (no frontend tests till MVP).
