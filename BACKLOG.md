# openmix backlog (owner-spammed features land here; max 2 workers at a time)

Rules: one worker per file-area (no overlapping paths). New items go to
Queued with the owner's words. A worker is dispatched only when a slot is
free AND its files don't collide with the running worker.

## In flight (0/2 slots — both free, no active subagents)
- (none)

## Queued (in order)
- (empty — verification matrix 17/19 landed, all gap fixes landed;
  send the next round)

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
- Phone independence (engine embedded, IPC transport, fresh APK).
- Subtractive review loop (6 rounds, net −17 lines, topbar/haptics/dedup).
- Real-only device scan + offline mode (honest empty states).
- Linked-fader pairing (sim + fixture proven, xair honestly scoped).
- Strip mini-icons (phantom/groups/link/polarity, live-verified).
- Mobile master strip parity (Main LR = channel strip on phone).
- Mobile routing rework (picker UI on phone, matrix intact on desktop).
- Graph label collisions fixed (nodes clear of ticks, no mid-unit wraps).
- Android network + sim removal (scan/probe from sim, honest boot,
  fresh APK in /mnt/gladson/temp).
- Worktree phone/daylight UI merged (main reviewed + verified).
- Channel topbar simplification (back+name+prev/next, switcher deleted).
- G5 solo desk→app path wired (25 ms, verified both ways).
- Optimistic-write epsilon + main-fader twin (no more stuck edits).
- EQ ingest arms (desk→app EQ in seconds, not a minute).
- Verification matrix 17/19 PASS, desk left as found.
- Orphan cleanup (7 stale test relays + stray chrome reaped; :8080,
  :8081, :5291 untouched).
