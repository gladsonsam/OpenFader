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
1. Frontend review loop (tonight's job) — subtractive only: no new
   features/screens. Screenshot-driven rounds on phone + desktop:
   slim the topbar, densify channel detail, remove/reduce/resize,
   tone down haptics/vibration. Uses the frontend-design skill.
   Iterate until genuinely usable. Starts when the merge lands.
2. Mobile UI audit loop — FOLDS INTO #1 (same loop covers vibration,
   topbar, channel density; no separate agent).
3. Devices: scan shows ONLY real desks (owner's XR18, no sim/fake
   entries) + explicit work-offline mode.
4. Linked-fader pairing in the frontend: where the app knows two strips
   are linked (stereo pair), moving one fader moves the other locally —
   no WS round-trip, no re-read, sync computed front-end-side.
5. Strip status mini-icons below the mute button: tiny indicators for
   phantom power, mute-group membership, and similar strip state —
   glanceable, no extra taps.
6. Push merged main (post-merge verification first).

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
- Fresh APK from current tree (/mnt/gladson/temp, badging verified).
- Worktree phone/daylight UI (merged or in merge — see In flight).
