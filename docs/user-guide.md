# openmix user guide (operator's handbook) — v0.2.0

Gig-oriented: install it, connect it, mix the show, go home.
Covers the desktop app, the Docker relay, and the phone PWA — all
driving the same mixer views.

> Scope: X-Air (XR12/16/18, MR18) and DL32S first. Anything marked
> **TODO-owner** is a behaviour the UI hints at but the backend does not
> confirm yet — do not depend on it at a gig.

## 1. Install paths

Pick one head per gig. They all talk to the same desk.

### A. Desktop (Tauri)

Unsigned local bundle, no signing configured. Needs the Svelte build
present (see `src-tauri/README.md` for the full notes).

```sh
cd src-tauri
cargo tauri dev      # run it
cargo tauri build    # local bundle
```

Linux needs the usual Tauri system libs (`libdbus-1-dev`,
`libwebkit2gtk-4.1-dev`, …) before the first build. Frontend served is
`apps/web/dist` — run `npm run build` in `apps/web` first if it is
missing. Desktop talks to the desk directly (Rust UDP/TCP sockets),
so no relay is needed. Note: the Tauri shell has **no strip-lock
support** (lock buttons never appear) — see §3.

### B. Relay + any browser (Docker / Pi)

`openmix-serve` is one process: static UI + WebSocket mixer API on one
port (default `8080`). The browser cannot open UDP sockets, so the
server is the bridge: browser ↔ WebSocket ↔ server ↔ UDP/TCP ↔ desk
(see `docs/docker.md`).

```sh
cargo run -p openmix-serve                       # sim on :8080
cargo run -p openmix-serve -- --preset studio     # sim rig-as-found on :8080
docker compose up --build                        # sim on :8080
DRIVER=xair ADDR=192.168.1.10 docker compose up --build
```

`--preset` shapes the **sim** desk only (hardware drivers ignore it):
`generic` (default), `xr18`, `dl32s`, `studio`. `studio` mirrors a
desk-as-found — faders/mutes already placed — so rehearsal starts from
something that looks like a gig, not a flat line. See §2.

Then open `http://<host-lan-ip>:8080` from any laptop/phone browser on
the same network. The `./scenes` volume keeps saved scenes next to the
deployment. Detached on a venue Pi: `docker compose up --build -d`.

### C. Phone PWA (no install, no store)

1. Start path A or B on the mixer LAN.
2. On the phone browser (same Wi-Fi as the desk/relay) open
   `http://<host-lan-ip>:8080` (relay) or the dev URL (desktop).
3. Browser menu → **Add to Home Screen / Install app**. Manifest is
   `standalone`, landscape, dark theme (`apps/web/manifest.webmanifest`).

A service worker (`apps/web/sw.js`) caches the app shell so the UI
itself loads offline; live socket traffic (`/ws`) is never cached —
you still need the LAN for the desk. Touch targets are 44 px; faders
use pointer capture so a drag keeps working if your thumb slides off
the strip.

## 2. First connect (sim vs XR18 vs DL32S)

Top bar: **driver** dropdown (`sim | xair | dl32s`), **addr** field,
**connect** button, `online`/`offline` badge.

| What | Driver | Addr | Notes |
| --- | --- | --- | --- |
| Rehearse with no desk | `sim` | blank | Auto-connects on load; 8 `CHxx` strips at 0.75. Safe to learn on. |
| Rehearse a lived-in desk | `sim` + relay `--preset studio` | blank | Sim with as-found faders/mutes. Relay only. |
| Behringer XR18 (X-Air) | `xair` | mixer LAN IP, e.g. `192.168.1.10` | UDP 10024. See §11 for FW/discovery notes. |
| Mackie DL32S | `dl32s` | mixer LAN IP (**required**) | TCP 50001. No broadcast — there is no discover; type the IP. |

Laptop/phone **and** the relay (if used) must be on the **same LAN /
subnet** as the desk. Wired preferred for the relay. UDP broadcast
discovery does not cross Docker's default bridge or subnets — pass
`--addr <mixer-ip>` explicitly (see `docs/docker.md`, `docs/LIVE-RIG.md`).

After connect the badge flips to `online`; strips populate from the
backend. If it stays `offline`, see §11.

> DL32S rename is expected to fail (`set_name` encoding unconfirmed):
> the UI keeps the old name. XR18 rename works.

**First run?** The onboarding wizard opens automatically: pick the
desk, type its LAN address, test the link, then walk a 5-step guided
tour of the app. Skippable throughout, re-open any time from the
top-bar **?** button. (If the browser blocks storage, the wizard
reappears next load — that is the fallback, not a bug.)

## 3. Mixer views tour

Six views, all in the nav bar: **Overview / Mixer / Channel / Rack /
Shows / Wizard**.

### Overview — whole-show health

One row per channel: name + `chN`, live meter bar (green/amber/red),
clip dot (latched — tap the meter to clear), fader-position marker,
`M` mute, `S` solo, and `mix›` on bus rows. Tap the name to jump to
that channel in Mixer. Think Mackie-style glance: red clip dots and
muted rows should read from three metres away.

### Mixer — faders

Horizontal strip bank: name, staging meter, touch fader, dB readout,
mute/solo, clip latch. Only one bank (layer) is visible at a time.

**Layers** (fader banks): `Inputs`, `Buses`, `Mains` built in, plus your
own custom layers. Toolbar → `LAYOUT` enables edit mode: reorder strips
(◀ ▶), insert blank spacers, `+layer` (built from current strips),
`rename`, `reset` (restore backend order), `del` (custom layers only).
Layout persists in `localStorage` per browser.

**Solo** is local monitor only — it dims non-soloed strips in the UI
and does not touch the desk. **Mute** hits the desk (when armed, §9).

In **Mains** you get the `MAIN` pseudo-strip: a local monitor-dim in
the web UI (the relay exposes no main fader op). The Tauri shell does
expose a driver-backed main fader — **TODO-owner**: confirm which main
value the desk follows when both are visible.

### Dual-pane (FOH + monitors, one screen)

Toolbar toggle **dual-pane** (title: `dual-pane: two independent mixer
views side by side`). Two full mixer panes, A and B, each with its own
layer bank and its own sends-on-fader destination — run LR on A and a
monitor bus mix on B, side by side. Drag the divider (or arrow keys,
`Home` resets); each pane tints independently when in sends-on-fader.

- On/off persists per browser (`openmix.split.v1`); pane B's bank
  resets to `buses` on reload (session-only).
- Below ~1100 px wide the panes stack and pane B hides — one pane on
  a phone, two on a laptop. That is the design, not a fault.

### Sends-on-fader (SoF) + aux rail

The **aux rail** (under the view nav) lists `LR` plus one button per
bus. Tap a bus (e.g. `AUX1`) to flip: input-strip faders now drive
**send levels to that bus**, and the bus master pins itself at the end
labelled `MASTER`. In dual-pane each pane has its own SoF destination.

**Tint warning:** the whole app (or the active pane, in dual-pane)
tints to the bus colour and a banner reads `SENDS → <name> — faders
are sends, not LR`. If you see colour, you are NOT on LR. `EXIT ✕` (or
tap `LR`, or tap the same bus again) returns to LR. `copy LR` seeds
the bus mix from the current LR balance.

Sends are cached locally and pushed via the existing `param` command
(`auxN_level`, dB). If a push fails the value is kept locally and the
status line says so — re-tap after reconnecting.

### Musician mode (performer's wedge box)

Top-bar **musician** button. The performer picks their own wedge/IEM
bus, then gets ONLY that bus: giant touch faders driving that bus's
per-channel sends, big channel names, one big master (bus) volume, a
**CALL FOH** flash button, and single-tap exit. No LR, no mutes/solos,
no layers, no scenes, no detail panel, no renames — hidden, not just
disabled. Send writes reuse the normal sends path; entry and exit are
one tap each (PIN-less by design — a locked-down device still wants
OS-level kiosk work: guided access / screen pinning, no browser
chrome; see the note at the top of `MusicianView.svelte`).

**CALL FOH:** fires a best-effort `attention` event on the bus strip,
which the relay fans out to every client — FOH builds flash an
attention toast (max 3 stacked, 10 s each, tap to jump to the
channel). The musician box always shows its own local strobing banner
too, so the performer gets feedback even if FOH runs a build without
the toast listener. Builds without the listener ignore the unknown
field gracefully — nothing crashes.

### Strip locks (two operators, one relay)

Over the relay (WS only — Tauri direct has no locks), any operator can
claim a strip: tap the lock affordance on the strip. The holder's id
shows on the strip badge and in the `locks` strip at the top; writes
to a strip locked by someone else are rejected and the denial names
the holder. Holds expire after 60 s without holder activity and
release on disconnect. Scene recall **bypasses** locks but the
recaller gets a toast naming every overridden channel (`scene recall
overrode locks on ch …`).

There is no client roster in the web UI — the locks strip *is* the
presence display. Everything else is last-write-wins: claim the
strips you are riding, and confirm the FOH/monitors split before
running two devices on one relay.

### Channel — one strip, big

Big fader for the selected strip plus the **Detail** panel (§4) and
mute-group assignment (`G1`–`G4`; tap to assign this channel). Select
by tapping a strip in Mixer, or jump from Overview.

### Rack / Shows / Wizard

- **Rack:** the 4-slot X-Air FX rack with labeled params and tap
  tempo (§5).
- **Shows:** the ordered show library with export/import and
  settings backup (§6).
- **Wizard:** the gain-staging walkthrough (§7).

## 4. Channel detail (EQ graph, dynamics, sends, presets)

Channel view → Detail panel for the selected channel, plus the
**preset picker** at the top: 10 factory strip starting points
(Kick, Snare, Hat, Tom, OH, Bass DI, E-Gtr, Vox, Keys, Playback —
web mirror of `crates/core/src/presets.rs`). A preset pushes its
fields through the normal param path and reports what it set; values
are conservative starting points, not targets — the engineer still
dials in. Sliders map to engineering units; ranges mirror the DL32S
address limits.

- **Input:** gain (−20…+60 dB), +48V phantom toggle, **USB return**
  source (`U01`…`U18` — X-Air live-verified; the `/insrc`
  input-source select is live-silent on FW 1.20 so it stays
  unsupported and has no field), **trim** (−18…+18 dB), **phase**
  (polarity), pan (L/R).
- **Filter / dynamics:** HPF on + freq (20…400 Hz), gate on, comp on,
  EQ on. **Gain-reduction bars** (gate/comp) go live when the relay
  sends `gr` frames — X-Air reads them from the desk's DYN blob
  (~5 Hz, first 16 channels); sim/DL32S report 0. Until the first
  frame arrives the bars render the static scale with a `no GR data
  yet` note.
- **EQ bands 1–4:** per band gain (±20 dB), freq (20 Hz…20 kHz, log),
  Q (0.1…20); bands 2–4 add a type select. The **EQ graph** at the top
  draws the combined curve — drag bands directly; the sliders follow.
- **Aux sends 1–8 / FX sends 1–4:** level (−120…+10 dB) + per-send mute.

Rules of the road:

- Controls the backend **rejects disappear** (unsupported on this
  desk). A one-line hint confirms what hid.
- The backend never echoes param state, so EQ sliders reset to defaults
  when you change channel — what you pushed already landed.
- **Rename:** text field, ≤16 chars, pushed as `name`. (DL32S: expect
  failure, see §2.)

## 5. FX rack (Rack view)

Four X-Air FX slot cards. Each card: an effect picker, the slot's
parameters, and (on delay-type effects) a tap-tempo row.

- **Labeled params:** the picker selects which effect's parameter set
  to render, from the research table mirrored in
  `crates/proto-xair/src/fx.rs`. Continuous params render sliders,
  stepped/mode params render selects, padding params stay hidden,
  low-confidence rows carry a `?` suffix. Writes go through the
  existing `param` path (`fxN_parMM`, 0.0…1.0 bit-exact) — no protocol
  change, and a relay/shell without an FX route disables the card
  instead of failing silently.
- **The picker is labels-only:** it never writes `/fx/N/type`, so
  picking an entry never retunes the desk — it just relabels the same
  `fxN_parMM` addresses. Unknown effect index falls back to generic
  `P01`–`P08` sliders.
- **Tap tempo:** on delay-type effects a **TAP** button plus
  quarter / dotted-8th division select. Tap 4+ times → BPM → ms,
  pushed to the effect's Time param (assumed-linear ms→0..1 over the
  documented range; the card shows the computed `BPM · ms`, or `tap
  4×` until it has enough taps). Effects without a documented
  time-param range get no TAP button — no guessing.

## 6. Shows / scenes workflow (+ export, import, backup)

Three levels — use the right one:

- **Scene slots S1–S4** (strip above the views): quick local snapshots.
  `store` arms, then tap `S1`…`S4` to store; tap a filled slot (`●`) to
  recall faders + mutes. Empty slots show `○`. Stored in
  `localStorage` (`openmix.scenes.v1`) — per browser, per device.
- **Shows view:** ordered library of named shows, each with named
  scenes (name + strips + main fader). Capture from live strips,
  recall with a fade (default 400 ms), reorder, mark a dirty scene
  with `*`. Also `localStorage`-first (`openmix.shows.v1`); the Tauri
  shell additionally owns server-side scene ops
  (`capture_scene`/`recall_scene`/…), and the relay now does too
  (`list_shows`/`capture_scene`/`recall_scene`/`delete_scene` with a
  `shows` broadcast, `crates/serve/src/lib.rs`) — **TODO-owner**: the
  web `ShowEditor`/`Backend` is not wired to the relay's scene ops yet
  (still `localStorage`-only over WS), so confirm the sync story before
  relying on both at once.
- **Share files (export/import):** Shows view exports the active show
  or the whole library as a versioned JSON share file
  (`openmix-show`, v1 — offline-first, no server needed). Import
  validates first and **merges, never overwrites**: bad files are
  rejected with a reason, good ones preview (shows/scenes/strips,
  names) and add as new entries. Hand a support band your file on a
  USB stick; nothing gig-tested is lost. The S1–S4 slots import once
  into an `Imported` show so legacy snapshots survive.
- **Settings backup/restore:** one file holding every `openmix.*`
  localStorage entry (layers, orders, prefs, shows cache, sends,
  mute groups, onboarding flag). Export → `openmix-backup-v1.json`;
  restore validates **all** entries before writing anything, previews
  key count/bytes, and keeps unknown keys aside with a warning instead
  of applying them. Clone your tuned browser to the spare laptop
  before doors.

Gig pattern: build one Show per band/set (`Doors`, `Support`,
`Headline`, `Encore`), one scene per song or segment. Store before
soundcheck, recall with fade during changeover. The web library lives in
the browser that stored it — use the same device all night, or re-store
on the spare (or carry the export file). (Relay and Tauri shells each
hold their own server-side copy; the `./scenes` Docker volume is the
deployment-side copy, not an auto-sync.)

## 7. Gain wizard (Wizard view)

Step-through gain staging, one input at a time: live meter against a
**−18 dBFS target line** (peaks should kiss it, never park above it),
±1 dB steps pushed as `gain`, clip latch at ≥0.985 (tap to clear),
manual "staged" check per channel with a progress count. +48V is
behind an **armed confirm** — tap to arm, tap again to apply — so a
mis-tap never phantoms a ribbon. Input strips only: no buses, no MAIN.

## 8. Meter bridge + spectrum

- **Bridge button** (top bar) toggles a full-width live meter canvas —
  every strip side by side, muted strips read zero. Handy on a laptop
  at FOH; leave it off on a phone to save battery/CPU.
- **Per-strip meters** are painted imperatively (no re-render at 60 fps)
  with clip latch at ≥0.985. Tap a meter to clear its clip dot.
- **Spectrum** (Channel view, above EQ): 64 bars, bass→treble
  green/amber/red. Over the relay the server sends one `spectrum`
  message per channel per meter tick (first 16 channels,
  `SPECTRUM_CHANNELS` in `crates/serve/src/lib.rs`) and the panel renders
  it live; where no spectrum frame has arrived yet it shows a `fallback`
  tag — bars interpolated from the live meter level. The Tauri shell has
  no spectrum source yet, so desktop always shows the fallback.
- **`?perf=1` (dev builds only):** append to a dev-server URL for a
  tiny fps + frame-ms overlay (bottom-left) while tuning meter render
  cost. Inert in release builds — not a gig feature, a workbench gauge.

## 9. Safety: mute-enable, fine mode, tap-a-value

Three guards against fat-finger moments:

- **MUTE EN** (Mixer toolbar): master mute-enable. When **off**, every
  mute button locks out (Overview `M` buttons disable too). Run with
  mutes locked during the show; arm only for changeovers. Persists.
- **FINE**: fader drags move at 20%, keyboard steps shrink (0.01 →
  0.002 per step; Shift = coarse 0.05). Persists. Use for vocal rides.
- **Tap-a-value:** tap the dB readout to type an exact fader (or send,
  in SoF) value instead of dragging. Keyboard: arrows ride the
  selected fader, `m` toggles mute (when armed).

Status line (next to the scene slots) narrates every guarded action:
`mutes disabled — tap MUTE EN…`, `scene 2 recalled`, `send ch3→AUX1…
(kept locally)`, `ch4 locked by foh-2`, `scene recall overrode lock
on ch 5`.

## 10. Offline mode

- The app **auto-connects `sim`** on load, so it is usable with no
  hardware and no relay.
- The badge reads `offline` until the first good state/meters arrive;
  if no meter frame arrives for >1.5 s it drops back to `offline`
  (LAN loss vs. mixer silence — check the relay, not the desk, first).
- Scenes, shows, layers, orders, mute groups, sends, and safety prefs
  are all `localStorage` — they survive reloads and work offline.
  What does NOT survive offline: the live desk state itself. Reconnect
  and recall your scene to re-assert faders/mutes.

## 11. Troubleshooting

| Symptom | Likely cause → fix |
| --- | --- |
| `No mixer found` / `discover` empty | UDP broadcast never crosses subnets or Docker's bridge. Use directed IP: `--addr <mixer-ip>` / `discover` with explicit target. Put laptop, relay, and desk on the same subnet (see `docs/docker.md` §113, `docs/xair-protocol.md`). |
| DL32S never connects | TCP 50001, **no broadcast** — the IP field is mandatory. Check the desk's network screen, ping it, keepalive is 2.5 s so a slow join still settles. |
| `/bus` moves do nothing | Address width: buses are **single-digit** (`/bus/1/…`), channels are two-digit (`/ch/01/…`). Hardware is silent on `/bus/01/…` (see `docs/xr18-baseline.md` T2); the driver emits the single-digit form (strict-fake tested in `crates/proto-xair`). If a bus strip seems dead on an old build, update before debugging further. |
| Fader readback off by ~0.0005 | Normal: the XR18 quantises faders to ~10-bit steps (`0.5` → `0.4995`). Not a bug; never assert exact float equality on fader writes (T8). |
| Meters stuck at 0 | Hardware meter subscriptions are best-effort. The subscribe form is hardware-verified (send `/meters` with one string arg `/meters/1`; bare queries stay silent — see `docs/xair-protocol.md`), but the desk only dribbles blobs without keepalive renew, so expect gaps. Check signal on a known channel, then `meters` via CLI; "levels stuck at 0" is a finding to report with FW + network path, not proof of no audio (`docs/LIVE-RIG.md` §6). |
| Wrong channel count (ch 17–32 silent) | XR18 is 16 channels + stereo aux return (`/rtn/aux`); there are no `/ch/17..18` replies. Ignore phantom strips past 16 (T3). |
| Rename fails on DL32S | Expected: rename write encoding unconfirmed, driver leaves it unsupported. Re-label from Master Fader instead. |
| Scene recall missed a channel | Recall pushes fader-then-mute per strip and stops at the first error (status line names the channel). Fix the named channel (often an unsupported strip), re-recall. |
| `…locked by <id>` on your move | Another operator holds that strip (§3). Ask them to release, or recall the scene (recall bypasses locks and names what it overrode). |
| App tinted a bus colour, faders "wrong" | You are in sends-on-fader. `EXIT ✕` / `LR` returns to LR (§3). |
| Musician sees pick-your-bus, nothing else | That is musician mode (§3), not a fault. Single-tap exit returns to FOH. |
| TAP button missing on an FX card | That effect has no documented time-param range — tap tempo is withheld rather than guessed (§5). Pick the labeled effect that matches the desk program. |
| Import rejected / backup rejected | The file failed validation before anything was written — read the reason, re-export from the source device. Nothing was applied, nothing was lost (§6). |
| Blank page on Android `dev` | Cleartext HTTP blocked: release loads bundled assets, but `dev` needs `usesCleartextTraffic` on Android 9+ (see `docs/mobile.md` §5). |
| iOS asks for Local Network permission | Expected once: allow it, or all sockets fail silently. The prompt text comes from `NSLocalNetworkUsageDescription` (§5 of `docs/mobile.md`). |

**Firmware notes.** X-Air map assumed FW **1.22**, hardware-verified on
an XR18 at **1.20** (`docs/xr18-baseline.md`): mains live at `/lr/*`
(X32-style `/main/st/*` is silent — driver uses `/lr/*`). DL32S map
verified against firmware `0106000100000002` (`docs/dl32s-protocol.md`);
after any desk FW update, re-verify against a fresh dump before
trusting every field — 44 of 106 DL32S offsets are position-certain
but meaning-inferred.

**Before filing a rig issue**, capture the `docs/LIVE-RIG.md` §0 block:
desk model + IP (static/DHCP), laptop IP (wired/wifi), mixer FW, app
used + version, date/venue — plus which checklist step failed.

## 12. FAQ

**Do I need the internet at the venue?**
No. Everything is LAN-local (desk ↔ relay ↔ browser). The PWA installs
from the relay itself, and the service worker keeps the app shell
cached.

**Can two operators mix at once?**
Yes, with discipline: open the same relay from both devices, claim
strip locks for the channels you are riding (§3), and agree the
FOH/monitors split up front. Locks arbitrate fader/mute/param writes
(60 s expiry, denials name the holder); scene recall bypasses locks
and reports what it overrode. Everything unclaimed is last-write-wins.

**Where do my scenes live?**
In the browser that stored them (`localStorage`) — plus, now,
wherever you carried them: export a show or the whole library as a
share file, or back up all settings to one file (§6). Same device all
night still works; the export file is the spare.

**Sim or hardware — how do I tell?**
The `via` label next to the badge (`tauri ipc` vs `ws <url>`) plus the
driver dropdown. Sim always works with a blank addr — if the desk is
dark and everything responds, check you didn't rehearse the sim. On a
relay, `--preset studio` sim is deliberately lived-in; `generic` is
the flat one.

**Why do rejected controls vanish instead of greying out?**
So an unsupported desk reads clean on a small phone screen. The hint
line (`Controls the backend rejects disappear…`) confirms nothing is
broken — switch desks and they come back.

**Why does the effect picker not change the desk sound?**
The Rack picker is labels-only (§5): it relabels the same `fxN_parMM`
addresses so you can see what a param does. The desk's loaded effect
program is the truth — match the picker to it.
