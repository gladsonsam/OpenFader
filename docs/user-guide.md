# openmix user guide (operator's handbook)

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
so no relay is needed.

### B. Relay + any browser (Docker / Pi)

`openmix-serve` is one process: static UI + WebSocket mixer API on one
port (default `8080`). The browser cannot open UDP sockets, so the
server is the bridge: browser ↔ WebSocket ↔ server ↔ UDP/TCP ↔ desk
(see `docs/docker.md`).

```sh
cargo run -p openmix-serve                       # sim on :8080
docker compose up --build                        # sim on :8080
DRIVER=xair ADDR=192.168.1.10 docker compose up --build
```

Then open `http://<host-lan-ip>:8080` from any laptop/phone browser on
the same network. The `./scenes` volume keeps saved scenes next to the
deployment. Detached on a venue Pi: `docker compose up --build -d`.

### C. Phone PWA (no install, no store)

1. Start path A or B on the mixer LAN.
2. On the phone browser (same Wi-Fi as the desk/relay) open
   `http://<host-lan-ip>:8080` (relay) or the dev URL (desktop).
3. Browser menu → **Add to Home Screen / Install app**. Manifest is
   `standalone`, landscape, dark theme (`apps/web/manifest.webmanifest`).

Touch targets are 44 px; faders use pointer capture so a drag keeps
working if your thumb slides off the strip.

## 2. First connect (sim vs XR18 vs DL32S)

Top bar: **driver** dropdown (`sim | xair | dl32s`), **addr** field,
**connect** button, `online`/`offline` badge.

| What | Driver | Addr | Notes |
| --- | --- | --- | --- |
| Rehearse with no desk | `sim` | blank | Auto-connects on load; 8 `CHxx` strips at 0.75. Safe to learn on. |
| Behringer XR18 (X-Air) | `xair` | mixer LAN IP, e.g. `192.168.1.10` | UDP 10024. See §9 for FW/discovery notes. |
| Mackie DL32S | `dl32s` | mixer LAN IP (**required**) | TCP 50001. No broadcast — there is no discover; type the IP. |

Laptop/phone **and** the relay (if used) must be on the **same LAN /
subnet** as the desk. Wired preferred for the relay. UDP broadcast
discovery does not cross Docker's default bridge or subnets — pass
`--addr <mixer-ip>` explicitly (see `docs/docker.md`, `docs/LIVE-RIG.md`).

After connect the badge flips to `online`; strips populate from the
backend. If it stays `offline`, see §9.

> DL32S rename is expected to fail (`set_name` encoding unconfirmed):
> the UI keeps the old name. XR18 rename works.

## 3. Mixer views tour

Four views, always in the nav bar: **Overview / Mixer / Channel / Shows**.

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
and does not touch the desk. **Mute** hits the desk (when armed, §6).

In **Mains** you get the `MAIN` pseudo-strip: a local monitor-dim in
the web UI (the relay exposes no main fader op). The Tauri shell does
expose a driver-backed main fader — **TODO-owner**: confirm which main
value the desk follows when both are visible.

### Channel — one strip, big

Big fader for the selected strip plus the **Detail** panel (§4) and
mute-group assignment (`G1`–`G4`; tap to assign this channel). Select
by tapping a strip in Mixer, or jump from Overview.

### Sends-on-fader (SoF) + aux rail

The **aux rail** (under the view nav) lists `LR` plus one button per
bus. Tap a bus (e.g. `AUX1`) to flip: input-strip faders now drive
**send levels to that bus**, and the bus master pins itself at the end
labelled `MASTER`.

**Tint warning:** the whole app tints to the bus colour and a banner
reads `SENDS → <name> — faders are sends, not LR`. If you see colour,
you are NOT on LR. `EXIT ✕` (or tap `LR`, or tap the same bus again)
returns to LR. `copy LR` seeds the bus mix from the current LR balance.

Sends are cached locally and pushed via the existing `param` command
(`auxN_level`, dB). If a push fails the value is kept locally and the
status line says so — re-tap after reconnecting.

## 4. Channel detail (EQ graph, dynamics, sends)

Channel view → Detail panel for the selected channel. Sliders map to
engineering units; ranges mirror the DL32S address limits.

- **Input:** gain (−20…+60 dB), +48V phantom toggle, pan (L/R).
- **Filter / dynamics:** HPF on + freq (20…400 Hz), gate on, comp on,
  EQ on.
- **EQ bands 1–4:** per band gain (±20 dB), freq (20 Hz…20 kHz, log),
  Q (0.1…20); bands 2–4 add a type select. The **EQ graph** at the top
  draws the combined curve — drag bands directly; the sliders follow.
- **Aux sends 1–8 / FX sends 1–4:** level (−120…+10 dB) + per-send mute.

Rules of the road:

- Controls the backend **rejects disappear** (unsupported on this
  desk). A one-line hint confirms what hid.
- The backend never echoes param state, so EQ sliders reset to defaults
  when you change channel — what you pushed already landed.
- **Gain-reduction bars** (gate/comp) render a static scale:
  **TODO-owner** — no GR frames come from the relay yet.
- **Rename:** text field, ≤16 chars, pushed as `name`. (DL32S: expect
  failure, see §2.)

## 5. Shows / scenes workflow

Two levels — use the right one:

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

Gig pattern: build one Show per band/set (`Doors`, `Support`,
`Headline`, `Encore`), one scene per song or segment. Store before
soundcheck, recall with fade during changeover. The web library lives in
the browser that stored it — use the same device all night, or re-store
on the spare. (Relay and Tauri shells each hold their own server-side
copy; the `./scenes` Docker volume is the deployment-side copy, not an
auto-sync.)

## 6. Meter bridge + spectrum

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

## 7. Safety: mute-enable, fine mode, tap-a-value

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
(kept locally)`.

## 8. Offline mode

- The app **auto-connects `sim`** on load, so it is usable with no
  hardware and no relay.
- The badge reads `offline` until the first good state/meters arrive;
  if no meter frame arrives for >1.5 s it drops back to `offline`
  (LAN loss vs. mixer silence — check the relay, not the desk, first).
- Scenes, shows, layers, orders, mute groups, sends, and safety prefs
  are all `localStorage` — they survive reloads and work offline.
  What does NOT survive offline: the live desk state itself. Reconnect
  and recall your scene to re-assert faders/mutes.

## 9. Troubleshooting

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
| App tinted a bus colour, faders "wrong" | You are in sends-on-fader. `EXIT ✕` / `LR` returns to LR (§3). |
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

## 10. FAQ

**Do I need the internet at the venue?**
No. Everything is LAN-local (desk ↔ relay ↔ browser). The PWA installs
from the relay itself.

**Can two operators mix at once?**
Two browsers can open the same relay and each gets state, meter,
spectrum, and `shows` broadcasts, but last-write-wins with no
locking — **TODO-owner**: confirm multi-client arbitration before
running FOH + monitors from two devices on one relay.

**Where do my scenes live?**
In the browser that stored them (`localStorage`). Same device all
night, or re-store on the spare. The `./scenes` Docker volume is the
deployment-side copy, not an auto-sync. (Relay and Tauri shells each
hold a server-side library too — but the web UI only reads its own
`localStorage` copy, so treat the browser as the source of truth until
the sync TODO in §5 is closed.)

**Sim or hardware — how do I tell?**
The `via` label next to the badge (`tauri ipc` vs `ws <url>`) plus the
driver dropdown. Sim always works with a blank addr — if the desk is
dark and everything responds, check you didn't rehearse the sim.

**Why do rejected controls vanish instead of greying out?**
So an unsupported desk reads clean on a small phone screen. The hint
line (`Controls the backend rejects disappear…`) confirms nothing is
broken — switch desks and they come back.
