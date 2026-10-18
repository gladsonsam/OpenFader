# LIVE-RIG: hardware verification checklist (XR18 + DL32S)

Goal: confirm `openmix-cli` / `openmix-serve` actually drive the real
desks before depending on them at a gig. Work through one desk at a time,
at low volume or with the PA muted — every step moves a real fader.

Pre-reqs: this repo built (`cargo build -p openmix-cli`), laptop on the
**same LAN** as the mixer (wired preferred; note which you used).

## 0. Record the rig (do this first, paste back verbatim)

```
desk:            XR18 | DL32S
mixer IP:        ___ (static? DHCP reservation? note which)
laptop IP/link:  ___ (wired wifi?)
mixer FW:        ___ (see below where to read it)
app used:        ___ (X-Air Edit / Mixing Station / Master Fader v___)
date/venue:      ___
```

- **XR18 firmware:** X-Air Edit → Setup → `FW 1.22` is the assumed line
  (see `docs/xair-protocol.md`). Record the exact string.
- **DL32S firmware:** Master Fader → Settings/About. The protocol map was
  verified against firmware `0106000100000002`
  (see `docs/dl32s-protocol.md`). Record the exact string — if yours
  differs, every step below is a re-verification, not a formality.

## 1. Reachability (no audio moves yet)

```sh
# XR18 (UDP 10024, broadcast discovery):
cargo run -p openmix-cli -- discover
cargo run -p openmix-cli -- --driver xair --addr <MIXER-IP> list

# DL32S (TCP 50001, no broadcast — IP required):
cargo run -p openmix-cli -- --driver dl32s --addr <MIXER-IP> list
```

Record: `discover` output (or "no X-Air devices found" + that you used an
explicit `--addr`), and the first 3 lines of `list`.

## 2. Fader roundtrip (pick an unused channel, PA down)

Use a spare channel `N` (1-based as shown in `list`):

```sh
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> fader <N> 50
# confirm on the mixer screen/app, then:
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> fader <N> 0
```

Record: did the on-desk fader follow both moves (50 → 0)? If not, which
direction failed.

## 3. Mute roundtrip (same channel)

```sh
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> mute <N> on
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> mute <N> off
```

Record: mute LED/state on the desk for both commands.

## 4. Name / rename

```sh
cargo run -p openmix-cli -- --driver xair --addr <IP> name <N> TEST
cargo run -p openmix-cli -- --driver xair --addr <IP> list   # check ch N
```

- **XR18:** rename is implemented (`/ch/NN/config/name`). Record whether
  the scribble changed on the desk, then rename it back.
- **DL32S:** rename is **expected to fail** —
  `dl32s: set_name not supported (rename write encoding unconfirmed)`.
  Run it anyway and paste the exact error; a success means the encoding
  got confirmed and `crates/proto-dl32s` needs updating. Check `list`
  afterwards to confirm nothing changed.

## 5. Scene roundtrip via CLI (PA down — no snapshot command exists yet)

There is no `snapshot` subcommand, so a scene is a recorded `list` plus
re-applied moves. Use three spare channels (A, B, C):

```sh
# state A: set it, record it
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> fader <A> 70
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> fader <B> 30
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> mute <C> on
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> list > scene-A.txt
# scramble from the desk's own app (move A/B/C anywhere else)
# re-apply state A with the same three commands, then:
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> list > scene-A2.txt
diff scene-A.txt scene-A2.txt && echo SCENE-OK
```

Record: `SCENE-OK` or the diff. (DL32S on-mixer show files are out of
scope — `save_show` exists in the driver but the file-transfer flows are
not ported; only live fader/mute moves are under test.)

## 6. Meters sanity

```sh
cargo run -p openmix-cli -- --driver <xair|dl32s> --addr <IP> meters
```

With signal present (talk into a mic on a known channel), record whether
the reported levels move. Note: hardware meter subscriptions are driver
best-effort — "levels stuck at 0" is a finding, not a failure.

## What to paste back as results

Reply with one block per desk:

```
desk: XR18, ip: 192.168.1.10 (static), fw: 1.22, laptop: wired 192.168.1.20
1. reach: OK (list shows 18 ch, first line: ...)
2. fader ch7 50->0: OK / FAILED (...)
3. mute ch7 on/off: OK / FAILED (...)
4. rename ch7 TEST: OK, reverted / FAILED (...)
5. scene recall: SCENE-OK / FAILED (paste diff) / skipped (why)
6. meters: moving / stuck at 0
```

Plus anything surprising (wrong channel mapping, inverted fader, extra
latency estimate). Surprises go into `docs/xair-protocol.md` /
`docs/dl32s-protocol.md` as corrections — the driver only encodes those
tables.

## 7. Param-plane verification (mostly verified 2026-10-18, MS milestone)

The relay now advertises a param catalogue (`caps` frame) and polls it
(`params` frames). The X-Air rows below were community-documented only —
the 2026-10-18 driver-path probe (owner XR18, FW 1.20, read-only except
same-value writes on quiet ch2) answered every read below; test-value
writes with restore are still open where noted. Method per row: read the
address, write a test value, read back, restore the baseline, verify the
restore (same discipline as §6).

```
desk: XR18, ip: ___, fw: ___, date: ___
```

| # | Param (relay id) | Address | Check |
|---|------------------|---------|-------|
| P1 | pan | `/ch/01/mix/pan` | read (expect `0.5` wire = centre); write `0.75` → read `0.75` → restore |
| P2 | eq_on | `/ch/01/eq/on` | read; write same-value roundtrip; restore |
| P3 | eq1_gain | `/ch/01/eq/1/g` | write `0 dB` (wire `0.5`) → read `0.5` → restore |
| P4 | eq1_freq | `/ch/01/eq/1/f` | write `1000 Hz` → read back ≈ same (201 log steps) → restore |
| P5 | eq1_q | `/ch/01/eq/1/q` | note the INVERTED wire (wire `0` = Q 10); write Q `1.0` → restore |
| P6 | eq1_type | `/ch/01/eq/1/type` | read int `0..5`; same-value write → restore |
| P7 | gate_on | `/ch/01/gate/on` | same-value roundtrip; restore |
| P8 | comp_on | `/ch/01/dyn/on` | same-value roundtrip; restore |
| P9 | aux1_level | `/ch/02/mix/01/level` | already verified (§6 row 4): `0.8` → `0.80000001` → restored `0.2` |
| P10 | fx1_level | `/ch/02/mix/07/level` | write `0.8` → readback → restore prior value | OPEN (no completed write test on record) |
| P11 | send mutes | `/ch/02/mix/01/on` | EXPECTED SILENCE (baseline T7): record the timeout as confirmation that `auxN_mute`/`fxN_mute` stay unsupported | SILENT confirmed (firmware map lists no `on` leaf; `sendN_on` OUT of caps) |

Rig results 2026-10-18 (owner XR18, FW 1.20, driver-path probe, read-only
except same-value writes on quiet ch2 — readbacks in
`docs/xr18-baseline.md` §7):

- P1: OK-read (`pan` = centre `0`); test-value write (`0.75`) + restore OPEN.
- P2: OK-read (`eq_on` = true on ch1); same-value/write + restore OPEN.
- P3: OK-read (`eq1_gain` = 0 dB); test-value write + restore OPEN.
- P4: OK-read (`eq1_freq` = 124.7 Hz); test-value write + restore OPEN.
- P5: OK-read (`eq1_q` = 2, inverted wire confirmed); test-value write + restore OPEN.
- P6: OK-read (`eq1_type` = int `2`, in `0..=5`); test-value write + restore OPEN.
- P7: OK-read (`gate_on` = true on ch1); write + restore OPEN.
- P8: OK-read (`comp_on` = false on ch1); write + restore OPEN.
- P9: OK (baseline §6 row 4: write `0.8` → `0.80000001` → restored `0.2`).
- P10: OPEN. P11: SILENT confirmed (stands).

## 8. Catalogue v2 verification (mostly verified 2026-10-18, R1 milestone)

Same read → write → read-back → restore discipline (speakers OFF for
writes). Rows without an OK/SILENT tag below are still
community-documented only — the driver treats a silent desk as a finding,
not a bug.

```
desk: XR18, ip: ___, fw: ___, date: ___
```

| # | Param (relay id) | Address | Check |
|---|------------------|---------|-------|
| P12 | gate full + comp full | `/ch/01/gate/thr|range|attack|hold|release|mode|keysrc|filter`, `/ch/01/dyn/thr|ratio|knee|mgain|attack|hold|release|mode|det|env|mix|auto` | same-value roundtrip on each leaf; record the wire reply shape (float vs int) and whether the linear mappings in `docs/xair-protocol.md` hold |
| P13 | colour | `/ch/01/config/color` | read int; same-value write → restore. CONFLICT: `/ch/16/config/color` was live-silent on FW 1.20 (input-module docs) — confirm or refute on this rig |
| P14 | send tap/pan/on | `/ch/01/mix/01/tap`, `/ch/01/mix/01/pan`, `/ch/01/mix/01/on` | roundtrip each. CONFLICT: `/mix/MM/on` was live-silent (baseline T7) — if the desk stays silent here, `sendN_on` drops out of `caps` and P11 stands |
| P15 | bus strip | `/bus/1/mix/fader`, `/bus/1/mix/on`, `/bus/1/config/name`, `/bus/1/eq/1/g` | fader roundtrip (validated T2 for fader); same-value roundtrip for on/name/eq; restore |
| P16 | FX-return strip | `/fxrtn/1/mix/fader`, `/fxrtn/1/config/name` | read; same-value roundtrip; restore (root UNVERIFIED) |
| P17 | FX-send + DCA strips | `/fxsend/1/mix/fader`, `/dca/1/mix/fader` | read; same-value roundtrip; restore (roots UNVERIFIED) |
| P18 | icon + link | `/ch/01/config/icon`, `/ch/01/config/link` | read; same-value roundtrip; restore (UNVERIFIED) | SILENT confirmed (icon: no firmware leaf; link: global `/config/chlink/1-2…` only — both OUT of caps) |

Rig results 2026-10-18 (owner XR18, FW 1.20, driver-path probe — readbacks
in `docs/xr18-baseline.md` §7; same-value writes only, on quiet ch2):

- P12: OK-read — every leaf answered on ch1 (gate: thr −41 dB, range 60 dB,
  att 12 ms, hold 126.2 ms, rel 752.1 ms, mode 0, key_src 0; gate filter on
  false, type 6, f 990.9 Hz; comp: thr 0 dB, ratio 3.0 = wire index 5 of the
  12-step table, knee 1, makeup 0 dB, att 10 ms, hold 10.0 ms, rel 151.2 ms,
  mode 0, det 0, env 1, mix 100 %, auto false; comp filter on false, type 6,
  f 990.9 Hz, key_src 0). Test-value writes + restore OPEN (same-value
  roundtrips on ch2 for `gate_filter_type` and `comp_ratio` were exact).
- P13: OK — `/ch/16/config/color` answers `0`. The CONFLICT in the Check
  column is REFUTED (ch16 is not silent); ch2 same-value write exact.
- P14: tap OK — all 10 slots read back (`1..6` = `2` POSTEQ, `7..10` = `4`
  POST, six-point enum `0..=5` confirmed); ch2 same-value write exact. Send
  pan slot 1 answers (OK-read). Send on: SILENT (T7 stands, OUT of caps).
- P15: PARTIAL — bus fader OK (baseline §6 row 6: write + restore); bus
  `eq5/6` OK-read (all 8 leaves on `/bus/1`); bus on/name/eq1 OPEN (not
  probed this round).
- P16: root CORRECTED — FX returns live at `/rtn/N`, not `/fxrtn/N` (desk
  silent there). `/rtn/1` fader + mute OK-read. Name OPEN.
- P17: DCA root CORRECTED — `/dca/N` carries fader/on directly (no `/mix`
  segment). `/dca/1` fader (`0.7497556`) + on (`1`) OK-read. FX-send
  (`/fxsend/N`) OPEN (not probed this round).
- P18: SILENT confirmed (see row).

Record per row: `OK (readback …)` / `FAILED (…)` / `SILENT (…)` plus any
baseline value you changed and restored. SILENT on P14 keeps the v2
`sendN_on` hidden on hardware (loopback-tested only); SILENT anywhere
else removes that id/family from `caps` until the desk answers.

Record per row: `OK (readback …)` / `FAILED (…)` plus any baseline value
you changed and restored. A row that disagrees with `docs/xair-protocol.md`
(the param-plane table) is a finding: correct the doc, not the desk.

## 9. R2 engine capabilities (UNVERIFIED-ON-RIG, R2 milestone)

Same discipline (speakers OFF for writes; PA down for scene recalls).
Every row is UNVERIFIED-ON-RIG: the engine paths are sim-tested, the desk
side is not. Record `OK` / `FAILED` / `SILENT` per row plus restored values.

```
desk: XR18, ip: ___, fw: ___, date: ___
```

| # | Capability (engine cmd) | Check |
|---|-------------------------|-------|
| P19 | Poller sustainable query rate | Raise `--poll-inflight 2` against the rig and watch for timeouts/dropped pairs over a 5 min sweep; repeat at 4 and 8. Record the highest rate with zero `timed out` drops — that number (not a guess) becomes the xair `max_inflight` default. Until then xair stays at 1. |
| P20 | Discovery (`scan`) | `{"cmd":"scan"}` on the relay with `--driver xair`: expect a `devices` frame listing the rig (ip + model from the `/xinfo` reply). Record the reply shape; SILENT (empty list) means the broadcast never arrives — note the network setup, do not change code. |
| P21 | Link/info (`connect_device`, `info`) | `connect_device` to the discovered rig: `link` reads `connected` with the rig ip and a plausible `latency_ms`; `info` returns the rig model/firmware. Confirm or refute the assumed 48 kHz (`sample_rate`) against the desk Setup screen. |
| P22 | Routing (`patch_set`) | EXPECTED SILENCE (baseline T7): `patch_set` on xair must keep erroring (`insrc` live-silent on FW 1.20). If the desk answers `/ch/NN/config/insrc` on this rig, record the reply shape — that (and only that) flips `caps.routing.supported`. |
| P23 | FX (`fx_param parNN`, `fx_set_type`, `fx_on`, `fx_bypass`) | `fx_param` with `par01..par64` on slot 1: write `0.5` → readback → restore (cf. baseline "Live FX par shapes" for reply shapes). `fx_set_type`/`fx_on`/`fx_bypass` must keep erroring (no verified mapping) — a reply where none is documented is a finding. |
| P24 | Numbered scenes (`scene_save`, `scene_recall`) | PA DOWN. Save slot 901 on ch1 (fader + one EQ gain + one comp threshold), move all three, recall with `fade_ms: 0`: all three read back. Then recall with the default fade and confirm the ramp is audible/smooth. Restore the desk scene afterwards. |
| P25 | Offline push | `offline xair` on the relay, edit ch1 gain, `connect` to the rig: `offline_diff` lists the change and nothing is pushed; `offline_push {"confirm":true}` applies it (readback matches); restore. `confirm:false` and no-diff pushes must error. |
| P26 | Taps (`hello {"taps":true}`) | Subscribe taps: `post` follows the `/meters/1` blob (compare against the meter screen), `gr_gate`/`gr_comp` read plausible reduction when gated/compressed (else 0.0). `in` must stay absent from `caps.meter_taps` (no `/meters/0` subscribe). SILENT GR (always 0.0 under real reduction) means `/meters/6` does not flow on this rig — record it, do not fake it. |
