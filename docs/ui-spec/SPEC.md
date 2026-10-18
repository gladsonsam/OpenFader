# openmix UI v3: design spec

Status: draft 1. Owner: Gladson. Implementation: `apps/web/src/v3/` behind `?ui=3`.
Visual references (private, git-ignored): `docs/ui-refs/INDEX.md`.

## 1. Product context

One app, three kinds of people, all on the same mixer:

| Role | Who | Device | What they see |
|---|---|---|---|
| **Engineer** (and Admin) | FOH / monitor engineer running the desk | laptop, tablet, sometimes phone | everything: full mixer, channel detail, FX, routing, shows, clients |
| **Operator** | volunteer / second person with a few controls (church-style) | tablet or phone | a small, safe subset: a few faders, mutes, scene recall; no processing |
| **Musician** | performer mixing their own wedge/IEM | their own phone | their bus only: big faders, one master, CALL FOH |

Backend roles today (`crates/core/src/access.rs`): Admin, Engineer, Musician. Operator is a UI preset
over Engineer policy plus UI-side restriction until the core grows the role.

Mixers: X-Air (XR12/16/18, MR18) and Mackie DL32S. Same UI for both; capabilities come from the driver.

## 2. Principles

1. **Great by default, customisable by data.** Each role x device class ships a tuned default profile.
   Customisation is edits to a profile (JSON), never forked code paths. A user who never opens
   settings must not feel the lack.
2. **Honest UI.** Only show what the backend really provides. The relay never echoes per-strip
   param state (it is a local-edit mirror, see `lib/bands.ts`), so sections without trusted state
   render an explicit *unknown* look (dimmed, no fake numbers). No decorative DSP, no fake meters.
3. **Safe by default.** Mute-enable master, fine mode, exact entry, lock badges, musician fader cap
   stay. Destructive/live-risk actions need deliberate gestures.
4. **Touch first, mouse and keyboard complete.** 44px minimum targets, full-strip fader hit area,
   every control reachable by keyboard with visible focus.
5. **Glanceable.** From arm's length: what is muted, soloed, clipping, which bus, who holds locks.
6. **Fast.** Meters are canvas + rAF off non-reactive buffers (`lib/levels.ts`). 60 fps with
   32 strips on a mid phone. No per-frame Svelte reactivity.


## 2a. Realtime budget (binding; this is live mixing)

Local response is immediate; the network is never on the visual path.
- **Fader/knob/mute/pan:** pixels change in the same frame as the pointer event (<= 16 ms), from
  local component state, not from a store round trip. The relay/mixer confirmation only reconciles.
- **Write path:** leading-edge send (first change goes out immediately), then latest-value-wins
  coalescing at <= 60/s per control. Writes PREEMPT polling and meter work in the engine.
  Budget on LAN: UI send -> engine -> mixer UDP packet out <= 5 ms p99 (loopback <= 2 ms),
  pointer -> UDP out <= 20 ms p99 over WebSocket, <= 10 ms in the native app.
- **Meters:** engine fans out at the SOURCE rate (X-Air ~50 ms today), never downsampled by the
  adapter; compact BINARY frames (typed array, 1-2 bytes per channel), not JSON; each frame carries a
  monotonic sequence and timestamp; slow clients get latest-only (drop stale, never queue).
  The UI paints at display refresh with ballistics/interpolation (attack/release, peak-hold decay
  computed per frame) so a 20 Hz source looks smooth. Meter data age at paint <= source interval
  + 30 ms. Gain-reduction and (later) all channel levels use the same non-reactive pipeline
  (typed arrays + canvas), >= 20 Hz.
- **No Svelte reactivity in hot paths:** meters, GR, RTA, active drags are imperative.
  Reactive state is for rare things (names, on/off, confirmed values).
- **Sockets:** TCP_NODELAY on, permessage-deflate off, Tauri uses an ordered binary Channel for
  meters (not JSON events).
- **Visible diagnostics:** `?perf=1` overlay shows fps, meter age, write round-trip, dropped meter
  frames, long tasks (> 50 ms); usable on site over Wi-Fi.
- **Enforced:** a latency harness reports p50/p95/p99 for pointer->paint, write RTT, meter age and
  long tasks; thresholds above fail the check. Wi-Fi jitter is expected: the UI shows RTT and
  stays local-first.

## 3. Visual language ("modern pro console")

Reference: WING Edit (information density, one-glance signal chain, bottom fader bank, right-hand
bus/monitor rail), Master Fader 5 (tall touch faders, layer rail), Mixing Station (customisation
model). **Not** skeuomorphic: drop v2's noise texture and bevelled knobs.

- Surfaces: flat, near-black, 1px hairlines, radius 8px (controls) / 12px (panels). Elevation by
  lighter surface, not shadow.
- Colour carries meaning only: **channel colour** (identity), **state LEDs** (mute red, solo amber,
  active/selected accent, clip red), **meter gradient** (green to amber to red).
- Channel colour palette: 12 swatches (red, orange, amber, yellow, lime, green, teal, cyan, blue,
  indigo, violet, pink) plus neutral. A strip's colour appears as a 4px top bar, the fader track fill at
  ~35% alpha, and the selected outline.
- Type: UI `Inter`/system; numerals tabular; scribble names in the condensed stack; readouts mono.
  Scale (px): 11 label, 13 body, 15 control, 20 readout, 28 hero value.
- Spacing: 4px base grid. Motion: 120ms ease-out for state, none for meters except peak-hold decay.
- Themes (CSS variables only): `dark` (default), `daylight` (high-contrast light for outdoor/bright
  rooms), `night` (dim, red-shifted, for dark stages). Never rely on colour alone: state also has a
  text/icon/shape cue.

## 4. Fader law and readouts

`format.ts` `faderDb` (20 log10 v) is wrong for X-Air. X-Air fader wire value is linear 0..1 and the
dB mapping is the frontend's job (`docs/xair-protocol.md`). Use the X32/X-Air law:

```
f < 0.0625 : db = f*480 - 90         (floor -90 = -inf)
f < 0.25   : db = f*160 - 70
f < 0.5    : db = f*80  - 50
else       : db = f*40  - 30         (0.75 = 0 dB, 1.0 = +10 dB)
```
Joints (continuous): 0.0625 -> -60, 0.25 -> -30, 0.5 -> -10, 0.75 -> 0.

Implement as `faderLaw.ts` with `toDb(f)`, `fromDb(db)` (exact inverse) and a per-driver registry.
DL32S has its own dB table in the driver (`crates/core` `DB_POINTS`); until the relay exposes the
table, the DL32S UI uses the same law and flags a TODO to verify on hardware (docs/LIVE-RIG.md).
Readout shows one decimal, `-inf` at floor. Scale ticks: +10, 5, 0, -5, -10, -20, -30, -40, -60.
Double-tap fader cap resets to 0 dB (unity). Exact entry opens a numeric field.

## 5. Information architecture

Top-level views (engineer): **Mix** (default), **Channel**, **Overview**, **FX**, **Routing**,
**Shows**, **Setup**. Always visible: console status strip (connection, clock, clip count, solo
state, lock/clients), view nav, and a persistent **Master** (LR + selected bus) panel.

- **Mix**: bank of strips (the core screen), layer rail, bus/send rail, master panel.
- **Channel**: selected strip enlarged: input, gate, EQ graph, comp, sends, routing, with the
  strip bank still visible below (WING Edit pattern). Phone: full screen, back to Mix.
- **Overview**: one health row per channel (meter, clip, mute, solo, jump).
- **Operator**: Mix only, a curated bank, scenes list, no Channel/FX/Routing/Setup.
- **Musician**: single screen: their bus faders, bus master, CALL FOH. Exit is one tap.

## 6. The strip: composable sections

A strip is a vertical stack of **sections**. Each section is a self-contained component with a
registry entry `{ id, minHeight, needs: 'state' | 'mirror' | 'none', Component }`. Order and
visibility come from the active profile.

Section catalogue (top to bottom, default order):

| id | Shows | Data trust |
|---|---|---|
| `scribble` | channel colour bar, number, name, lock badge | state (name) + local (colour) |
| `input` | gain, 48V, phase, HPF as compact controls | mirror |
| `gate` | on/off + GR bar | mirror + GR feed |
| `eq` | mini curve + on/off | mirror |
| `comp` | on/off + GR bar | mirror + GR feed |
| `sends` | aux send mini-knobs / bus dots | mirror (local send cache) |
| `pan` | pan slider with centre detent | mirror |
| `buttons` | mute, solo, (select) | state |
| `fader` | taper fader, dB scale, readout, value box on cap | state |
| `meter` | stereo-aware canvas meter, peak hold, clip latch | live |

Rules: a section that has no trusted data renders its *unknown* style, never invented values.
`fader`, `meter`, `buttons`, `scribble` are always available and cannot all be hidden.
Strip width: `narrow` 64 / `normal` 84 / `wide` 104 px (+ fixed 44px minimum for controls).


### 6a. Height-aware degradation (found in M3 review)

The fader is the product. A strip must never be taller than its container: the bank never scrolls
vertically and the fader region keeps a minimum height (>= 220px when viewport height >= 800,
>= 140px below that, e.g. phone landscape). When the profile's sections do not fit, drop optional
sections in this order until they do: `sends`, `gate`, `comp`, `input`, `pan`, `eq`. Never drop
`scribble`, `buttons`, `fader`, `meter`. Sections the user explicitly pinned (profile `pinned`
list, M7) are exempt and win over the fader minimum only down to 140px. The status strip shows a
quiet "N hidden to fit" chip so nothing disappears silently. Taller screens show more: the same
profile on 1920x1080 shows the full chain.
Chrome budget: layer rail and master panel together take <= 30% of the width on tablet; on phone
the rail is a pager, the master is a slim bar.

## 7. Responsive defaults (device classes)

| Class | Width | Engineer default |
|---|---|---|
| phone | < 640 | fader-first: `scribble, buttons, fader, meter` (+ `pan` in landscape). 5-6 strips/page, swipe between layers; Channel is full screen. |
| tablet | 640 to 1100 | `scribble, eq, comp, sends, pan, buttons, fader, meter`; ~12 strips; right rail for layers + master. |
| desktop | > 1100 | full chain, all sections; 16+ strips; Channel pane docks above the bank (WING Edit). |

Operator default: phone/tablet `scribble, buttons, fader, meter`, curated strips only.
Musician default: always phone-first single column of big faders.

## 7a. Profiles (the customisation model)

All customisation is one JSON document per profile, versioned and exportable:

```json
{
  "v": 1, "id": "engineer-tablet", "name": "Engineer / tablet",
  "base": "engineer", "theme": "dark", "density": "normal",
  "strip": { "width": "normal",
    "sections": [ {"id":"scribble","on":true}, {"id":"eq","on":true}, ... ] },
  "layers": [ { "id":"drums", "name":"Drums", "strips":[1,2,3,4], "color":"amber" } ],
  "channelColors": { "1": "red" },
  "behavior": { "fineByDefault": false, "muteEnable": true, "soloMode": "pfl" }
}
```

- `base` picks the shipped default; a user profile stores only overrides on top, so shipped
  defaults can improve without wiping user changes. "Reset to default" deletes the overrides.
- Role gates what can be edited: Musician cannot change anything beyond theme and size.
- Storage: localStorage (`openmix.profile.v1.*`) now; export/import as a `.json` file; later
  embedded in the show file so a venue's layout travels with it.
- Editor UI: in-place **Edit layout** mode (drag handles, section toggles, colour swatches), no
  separate settings maze. Everything the editor can change is a profile field; anything not in
  the schema is not customisable on purpose.

## 8. Behaviours worth keeping from v2 (see `apps/web/V2.md`)

Sends-on-fader with whole-surface tint, auto-cue on bus entry; custom layers; mute groups; mute-enable
master; fine mode; grow-and-glow fader grab; exact entry; strip locks with holder badge; CALL FOH.
Port behaviour and logic, not the old visuals.

## 9. Constraints for implementation

- Svelte 5 runes, TypeScript, no new runtime dependencies, no CSS framework.
- Reuse `lib/backend.ts` (contract unchanged), `lib/levels.ts`, `lib/meters.ts` painting approach,
  `lib/eqCurve.ts`, `lib/bands.ts`, `lib/sends.ts`, `lib/showFile.ts`. Do not edit `crates/`.
- v3 lives in `apps/web/src/v3/`; the existing app keeps working until cutover.
- Pure logic gets a plain-node test: `node --experimental-strip-types <file>.test.ts` exiting 0
  (same style as `src/lib/backup.test.ts`). No test framework to install.
- Accessibility: roles/labels on custom controls, keyboard operable, visible focus, 4.5:1 text
  contrast, `prefers-reduced-motion` respected.
