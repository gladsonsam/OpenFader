# XR18 hardware baseline — FW 1.20

Live-desk verification against the owner's XR18 at `192.168.40.123`
("Home Studio", FW 1.20). Desk pingable, speakers OFF during write tests.

- Date: 2026-10-17
- Method: python3 stdlib UDP to port 10024 (X-Air OSC: pad-string address +
  `","` empty typetag query, 1.5 s timeout per address). Scripts kept outside
  the repo: `/tmp/xr18_probe.py`, `/tmp/xr18_probe2.py`,
  `/tmp/xr18_probe3.py`, `/tmp/xr18_probe4.py`; raw results in
  `/tmp/xr18_results.json`, `/tmp/xr18_round{2,3,4}.json`.
- Scope rule for this doc: reads always; writes only as same-value roundtrips
  in phase 1 (read baseline), full write tests in phase 2 with restore.

## 1. Desk identity

| Query   | Reply tags | Values (in order)                                     |
|---------|------------|-------------------------------------------------------|
| `/xinfo`  | `,ssss`    | `192.168.40.123`, `Home Studio`, `XR18`, `1.20`       |
| `/info`   | `,ssss`    | `V0.04`, `Home Studio`, `XR18`, `1.20`                |
| `/status` | `,sss`     | `active`, `192.168.40.123`, `Home Studio`             |

## 2. Strip snapshot (baseline values — restore target)

`/ch/NN/mix/fader` → `,f`; `/ch/NN/mix/on` → `,i` (1 = ON/unmuted,
0 = OFF/muted); `/ch/NN/config/name` → `,s`.

| Ch | Fader    | On | Name     | Ch | Fader    | On | Name     |
|----|----------|----|----------|----|----------|----|----------|
| 01 | 0.197458 | 0  | BASS     | 09 | 0.000000 | 1  | DRUM L   |
| 02 | 0.000000 | 0  | (empty)  | 10 | 0.482893 | 1  | DRUM R   |
| 03 | 0.337243 | 0  | (empty)  | 11 | 0.438905 | 0  | KEYS L   |
| 04 | 0.348974 | 0  | VOX      | 12 | 0.438905 | 0  | KEYS R   |
| 05 | 0.375367 | 1  | VOX      | 13 | 0.339198 | 0  | AC GTR   |
| 06 | 0.000000 | 0  | (empty)  | 14 | 0.000000 | 0  | (empty)  |
| 07 | 0.434995 | 1  | GTR L    | 15 | 0.524927 | 0  | Abelton  |
| 08 | 0.434995 | 1  | GTR R    | 16 | 0.524927 | 0  | Abelton  |

| Strip        | Fader    | On | Name    |
|--------------|----------|----|---------|
| `/lr`        | 0.749756 | 1  | (empty) |
| `/rtn/aux`   | 0.457478 | 1  | COMP LR |
| `/bus/1`     | 0.749756 | 1  | COMP L  |
| `/bus/2`     | 0.749756 | 1  | COMP R  |
| `/bus/3`     | 0.749756 | 1  | (empty) |
| `/bus/4`     | 1.000000 | 1  | DRUM    |
| `/bus/5`     | 1.000000 | 1  | KEYS    |
| `/bus/6`     | 1.000000 | 1  | VOX     |
| `/dca/1..4`  | 0.749756 | 1  | n/a     |

Spot reads: `/fx/1/par/01` → `,f 0.1`; `/ch/02/mix/01/level` → `,f 0.2`.

## 3. Verified driver behaviors (hardware agrees)

- V1: `/ch/NN/mix/fader` (2-digit NN, 01–16) get/set float 0.0–1.0.
- V2: `/ch/NN/mix/on` get/set int; 1 = on/unmuted, 0 = off/muted.
  Matches `osc::on_arg` / `mute_is_muted` in `crates/transport/src/osc.rs`.
- V3: `/ch/NN/config/name` get/set string (incl. empty string).
- V4: Mains live at `/lr/mix/fader` (`,f` 0.749756), `/lr/mix/on` (`,i` 1),
  `/lr/config/name` (`,s` empty). X32-style `/main/st/mix/fader` and
  `/main/st/mix/on` get NO reply (re-queried with 1.5 s timeout).
- V5: Same-value roundtrip writes succeed: `/ch/01/mix/fader`,
  `/ch/01/mix/on`, `/ch/01/config/name` (read → write same → read back
  identical). Proves the write path with zero audible change.
- V6: CLI rebuilds clean (`cargo build -p openmix-cli`).

## 4. Discrepancies — TODOs for driver fixes

- [x] T1 (mains address): driver previously used X32-style `/main/st/*`;
  hardware is silent there, `/lr/*` is correct. Fix committed by sibling as
  `8f6a133 xair: mains via /lr (hardware-verified)` — now present in
  `git log`. Closed.
- [ ] T2 (bus index width): driver `bus_fader()` emits zero-padded
  `/bus/01/mix/fader` — hardware is **silent** on all `/bus/01..06/*`.
  Single-digit `/bus/1/mix/fader`, `/bus/1/mix/on`, `/bus/1/config/name`
  reply correctly for buses 1–6. Driver must emit `/bus/1/…` (channels
  stay 2-digit `/ch/01/…`; the widths differ per strip kind).
- [ ] T3 (channel count): `/ch/17/*` and `/ch/18/*` (fader/on/name) get NO
  reply — XR18 is a 16-channel desk (stereo aux returns via `/rtn/aux`).
  Driver `MAX_CHANNEL = 32` is an X32-ism; needs a per-model channel count
  (XR18: 16 + rtn/aux) so `list`/sweeps don't query phantom strips.
- [ ] T4 (meters): desk sent **zero** meter blobs for every subscribe
  variant tried: query `/xremote`, bare `/xremote` (no typetag, XAirEdit
  style) with 2 s × 3 keepalive windows, bare/query `/meters/0`,
  bare/query `/meters/1`, and a 10 s bare-`/xremote` listen. Driver
  `METER_SUBSCRIBE = "/meters/1"` + immediate-blob expectation (as in the
  `proto-xair` fake, which answers `/meters/1` instantly) is unverified
  against hardware. Meter streaming path needs a rethink (capture what
  XAirEdit actually sends, or treat meters as best-effort/empty).
- [ ] T5 (`query_info` field mapping): driver `info_from_strings` maps
  `model = strings[0]`, `firmware = strings[1]`, `name = strings[2]`.
  Hardware `/info` returns `[V0.04, Home Studio, XR18, 1.20]`, i.e. correct
  mapping is `name = [1]`, `model = [2]`, `firmware = [3]` (current code
  yields model=`V0.04`, firmware=`Home Studio`, name=`XR18` — all wrong).
- [ ] T6 (snapshot/CLI): `XAirDriver` has no `snapshot()` impl, so
  `openmix-cli --driver xair --addr 192.168.40.123 list` fails with
  `Error: xair: snapshot not supported` against live hardware. Either
  implement snapshot from per-strip queries (respecting T2/T3 address
  rules) or give the CLI per-strip get commands (current `fader`/`mute`
  commands are set-only; `name` is sim-only).
- [ ] T7 (send on/off): `/ch/02/mix/01/level` replies (`,f` 0.2) but
  `/ch/02/mix/01/on` is silent. Per-send on/off may not exist on X-Air
  (check pre/post tap address, e.g. `/ch/02/mix/01/pre`, before adding
  send-mute to the driver).
- [ ] T8 (fader quantization): hardware quantizes fader writes to ~10-bit
  steps — writing `0.5` lands at `0.4995112419128418` (off by ~5e-4), while
  send levels (`0.8` → `0.800000011920929`) and FX params (`0.5` → `0.5`)
  round-trip bit-exact. Driver `get_fader`/`set_fader` round-trips are NOT
  bit-exact; any verify/readback logic needs epsilon ≈ 1e-3, and tests must
  not assert exact float equality on fader writes.

## 5. Deliberately NOT touched (phase 1)

Network/config surface: IP, routing, FW update, init/reset — never probed.
`/main/st/*` silence confirmed by absence of reply only. No scene recall,
no FX writes, no value-changing writes of any kind in phase 1. Sibling's
uncommitted edits (`crates/proto-xair/src/lib.rs`, `docs/xair-protocol.md`)
were read but not modified, not staged, not committed.

## 6. Phase 2 — full write tests (speakers OFF, owner-approved)

Each op: read baseline → write test value → readback → restore baseline →
verify restore (retry × 5). Full results: `/tmp/xr18_writetests.json`.

| # | Op (addr) | Test value | Readback | Restored to | Result |
|---|-----------|------------|----------|-------------|--------|
| 1 | ch fader `/ch/02/mix/fader` | 0.5 | 0.49951124 | 0.0 exact | PASS* |
| 2 | ch mute `/ch/02/mix/on` | 1 | 1 | 0 exact | PASS |
| 3 | ch rename `/ch/02/config/name` | `PROBE` | `PROBE` | `` exact | PASS |
| 4 | aux send `/ch/02/mix/01/level` | 0.8 | 0.80000001 | 0.2 exact | PASS |
| 5 | FX param `/fx/1/par/01` | 0.5 | 0.5 | 0.1 exact | PASS |
| 6 | bus master `/bus/1/mix/fader` | 0.5 | 0.49951124 | 0.74975562 exact | PASS* |
| 7 | LR master `/lr/mix/fader` | 0.5 | 0.49951124 | 0.74975562 exact | PASS* |

\* PASS with quantization note (T8): fader readback differs from the written
value by ~5e-4 (desk step size); restore-to-baseline verified exact.

Driver-path write: `openmix-cli --driver xair --addr 192.168.40.123 fader 2 50`
→ desk reads `/ch/02/mix/fader = 0.49951124` (raw OSC verify). Driver
`set_fader` + `pct/100` scaling confirmed end-to-end. Restored to 0.0 after.

Desk-as-found: full re-read sweep of all 84 baseline addresses after the
write tests — **84/84 match §2 values**. Desk left exactly as found.

Scene snapshot/recall: NOT tested — no OSC path exists. `proto-xair` has no
scene addresses, `XAirDriver::snapshot()` is unimplemented (T6), and X-Air
scenes transfer as bulk files, not single OSC recalls. The §2 snapshot table
above serves as the scene record.

Deliberately NOT touched in phase 2 either: network/config surface (IP,
routing, FW, init/reset), anything outside 192.168.40.123, and all strips
besides the 7 test addresses (ch02, ch02-s1, fx1-p1, bus1, LR) — each
restored and re-verified.

## Live FX slots (2026-10-18 ~03:10 AWST, read-only /fx/N/type)
- slot1 = 7 VRM (vintage room), slot2 = 0 HALL, slot3 = 26 D/FL (delay+flanger), slot4 = 60 PIT (pitch shifter).
- Index↔name per research table; index 15 reads as gap (consistent with driver treating 15 as unknown).
- Priority spot-check order for labeled params: VRM, HALL, D/FL, PIT.

## Live FX par shapes (2026-10-18 ~03:15 AWST, read-only)
- VRM(7): par01-12 float, par13 INT (=0, switch/enum), par14+ float NaN (unused → NaN, not echo).
- HALL(0): par01-12 float, par13+ NaN.
- D/FL(26): par02/08/09 INT (0/1/1, enums), rest float through par13, par14+ NaN.
- PIT(60): par01-06 float, par07+ NaN.
- Rules for driver: expect INT pars mixed among floats (decode both); NaN float = absent par, must not surface as a value.

## 7. Driver-path probe (2026-10-18, owner rig, FW 1.20)

65/65 through the `MixerDriver` param path (`XAirDriver::get_param` /
`set_param` — the same code the relay poller uses). Read-only except
same-value write roundtrips on quiet ch2 (fader 0.0, muted — no audible
change; speakers left on). Temporary probe, deleted after use.

Live ch1 (BASS) readbacks, engineering units:

| Leaf | Readback |
|------|----------|
| pan | 0 (centre) |
| eq_on | true; eq1 gain 0 dB, freq 124.7 Hz, Q 2, type 2 |
| gate_on | true; thr −41 dB, range 60 dB, att 12 ms, hold 126.2 ms, rel 752.1 ms, mode 0 |
| gate filter | on false, type 6, f 990.9 Hz; gate key_src 0 |
| comp_on | false; thr 0 dB, ratio 3.0 (wire INT index 5 of the 12-step table), knee 1, makeup 0 dB, att 10 ms, hold 10.0 ms, rel 151.2 ms, mode 0, det 0, env 1, mix 100 %, auto false |
| comp filter | on false, type 6, f 990.9 Hz; comp key_src 0 |
| send1..6_tap (bus slots) | 2 (POSTEQ) |
| send7..10_tap (FX slots) | 4 (POST) |
| bus/1 eq5 | gain 0 dB, freq 20 Hz, Q 2, type 2 |
| bus/1 eq6 | gain 0 dB, freq 20 Hz, Q 2, type 4 |
| lr eq5_gain | 0 dB (main carries bands 5..6) |
| /rtn/1 fader / on | 0.7497556 / muted |
| /dca/1 fader / on | 0.7497556 / on |
| /ch/16 color | 0 (answers — contradicts the earlier "silent on /ch/16" note, which is corrected) |

Same-value roundtrips on ch2 (read → write identical → re-read):
fader `0 → 0` exact, mute/colour/send1_tap/gate_filter_type/comp_ratio
(`3.0`) all bit-exact. Bus-only `eq5_gain` on a channel is rejected
driver-side (`out of scope for In strips`, no I/O).

Deltas vs the old (wrong) map:
- tap was `0..=1` pre/post → desk reads `2`/`4`; six-point enum `0..=5` (IN, PREEQ, POSTEQ, PRE, POST, GRP) confirmed.
- comp ratio was a direct float (readback misreported as `5.0`) → wire is the INT step-table index (`5` → `3.0`).
- gate range was `0..=60` → `3..=60`; gate hold was `0..=500` → `0.02..=2000` (log); gate release was `5..=1000` → `5..=4000` (log).
- gate mode/key/filter was `0..=2` / `0..=3` / `0..=2` → mode `0..=4` (GATE, EXP2, EXP3, EXP4, DUCK), key_src `0..=22`, plus the `filter/` node (on `0..=1`, type `0..=8`, f log `20..20000`).
- comp ranges/curves: thr `-60..=0` lin, knee `0..=5` lin, makeup `0..=24` lin, att `0..=120` lin, hold `0.02..=2000` log, rel `5..=4000` log, mix `0..=100` lin (old table had `0..=300` / `0..=500` / `10..=1000` assumed-linear).
- strip roots: FX returns live at `/rtn/N` (there is no `/fxrtn` leaf); DCA fader/on sit directly under `/dca/N` (no `/mix` segment).
- caps: `icon` / `link` / `sendN_on` are OUT (no desk leaf — send block lists `level`, `grpon`, `tap`, `pan` only); gate/comp `filter/` leaves, `comp_key_src` and bus/main `eq5/eq6` are IN (all answer).
- ch16 `config/color` answers (the input-module "silent on /ch/16" note was wrong).
