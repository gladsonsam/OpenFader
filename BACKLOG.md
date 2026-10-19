# openmix backlog (owner-spammed features land here; max 2 workers at a time)

Rules: one worker per file-area (no overlapping paths). New items go to
Queued with the owner's words. A worker is dispatched only when a slot is
free AND its files don't collide with the running worker.

## In flight (2/2 slots full)
1. Channel topbar simplification (running) — back-to-mixer + name +
   prev/next only, switcher removed.
2. Android network + sim removal (running) — real LAN access on-device
   (multicast lock etc.), sims out of device lists (offline == sim).

## Queued (in order)
1. G5 fix (REAL product gap from verification): solo has no desk→app
   path — solosw bank not in poll set, UI solos view echo-only. Wire
   parse_inbound + poll set. (Needs re-run approval: harness fixed but
   re-run touches the desk ~40 min.)
2. Mobile routing rework (usable on mobile).
3. Graph label collisions (comp/EQ axis + summary wraps).

## Queued (in order)
1. Channel view topbar simplification: back-to-mixer button + channel
   name + prev/next channel buttons only; then remove the separate
   channel switcher UI it replaces.
6. Routing usable on mobile (currently unusable — rework for small
   screens: readable, tappable, no precision-dependent patching).
7. Graph label collisions (comp/EQ graphs, phone): x-axis "-60" hides
   behind the G node, "Input →" overlaps the curve, summary line
   ("Makeup 0.0 dB") wraps mid-unit. Keep nodes clear of ticks,
   no mid-unit wraps.

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
- Fresh APK from current tree (/mnt/gladson/temp, badging verified).
- Worktree phone/daylight UI (merged or in merge — see In flight).
