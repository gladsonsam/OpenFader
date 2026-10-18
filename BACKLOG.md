# openmix backlog (owner-spammed features land here; max 2 workers at a time)

Rules: one worker per file-area (no overlapping paths). New items go to
Queued with the owner's words. A worker is dispatched only when a slot is
free AND its files don't collide with the running worker.

## In flight (2/2 slots full)
1. Worktree merge (`ses_eb07b0b58ffevIdir5Y5YMhulo`) — phone/daylight UI
   branch `fix/boot-crashes` into main (reconcile + rebuild + screenshots).
2. Bidirectional verification loop (`ses_eb0877d76ffeFIOsVr9lbo0kFj`) —
   scripts/verify-bidirectional.mjs matrix + root-caused gap list.

## Queued (in order)
1. Bidirectional verification matrix (running — `ses_eb0877d76dffeFIOsVr9lbo0kFj`):
   per-control both-directions proof + root-caused gap list.
2. Scan/remote-access fix (running — `ses_eaebbc0f3ffeOulz3Vh5MTj0q4`).
3. Phone independence: embed the engine in-process in the Tauri app
   (Tauri IPC transport, no relay/server/LAN needed). Auto-select
   transport: IPC inside the app, WebSocket in the browser. The W2a
   TauriBackend is currently a stub — this is the ME2 work to finish it.
4. Mobile master strip parity: Main LR renders like every other channel
   strip (volume + mute only); tap to expand for more. No special-case
   master layout on phone.
5. Channel view topbar simplification: back-to-mixer button + channel
   name + prev/next channel buttons only; then remove the separate
   channel switcher UI it replaces.

## Queued — server-side (NOT today; parked, do not dispatch)
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
- RTA overlay for EQ (live engine spectrum; honest non-analyzer caveat).
- Presets (8 built-in + user save/recall/rename/delete, live-proven).
- Global meter bridge (grouped ladders + topbar icon, live XR18 verified).
- Subtractive review loop (6 rounds, net −17 lines, topbar/haptics/dedup).
- Real-only device scan + offline mode (honest empty states).
- Linked-fader pairing (sim + fixture proven, xair honestly scoped).
- Strip mini-icons (phantom/groups/link/polarity, live-verified).
- Fresh APK from current tree (/mnt/gladson/temp, badging verified).
- Worktree phone/daylight UI (merged or in merge — see In flight).
