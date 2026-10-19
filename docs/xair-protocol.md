# X-Air OSC protocol (XR12/16/18, MR18)

Owner-rig firmware: **XR18 FW 1.20** ("Home Studio" @ 192.168.40.123).
Anything marked VERIFIED below was read back from that desk on
2026-10-18 through the driver param path (see `docs/xr18-baseline.md`
§7 for the readback table); community-documented rows say so. If a field
disagrees with your mixer, this file is the place to correct it; the
driver only encodes this table.

## Transport

- UDP port **10024**, ephemeral local port, `connect()`d socket.
- Keepalive: empty OSC message `/xremote` every **~8s**, else the mixer stops
  streaming meters/updates. (Shared `KeepaliveConfig::xremote()` in
  `openmix-transport::session`.)
- Unicast discovery: no-arg `/info` → reply `/info <model> <firmware> <name…>`
  as OSC strings. Implemented in `XAirDriver::query_info`.
- Broadcast discovery: no-arg `/xinfo` to `255.255.255.255:10024` → mixers
  reply `/xinfo <ip> <model> <fw> …`. Legacy FW answers `/info` instead.
  Implemented once in `openmix-transport::discovery` (`PROBE_TABLE`,
  `discover_xair`); drivers add a table row, never a socket loop.
- Requests: send address with no args to query, mixer replies to the same
  address. Serialized with a 2s timeout (`OscSession::query`).

## Address table (implemented)

Zero-padded two-digit channels (`/ch/01`–`/ch/16`, XR18 is a 16-channel
desk — `/ch/17+` is silent). Buses stay single-digit (`/bus/1` — the desk
is silent on zero-padded `/bus/01`).

| Function      | Address                  | Args (set)              | Reply (get)        |
|---------------|--------------------------|-------------------------|--------------------|
| Ch fader      | `/ch/XX/mix/fader`       | float `0.0..1.0`        | float              |
| Ch mute       | `/ch/XX/mix/on`          | int `1`=on / `0`=muted  | int                |
| Ch name       | `/ch/XX/config/name`     | string                  | string             |
| Bus fader     | `/bus/N/mix/fader`       | float `0.0..1.0`        | float              |
| Main LR fader | `/lr/mix/fader`          | float `0.0..1.0`        | float              |
| Main LR mute  | `/lr/mix/on`             | int `1`=on / `0`=muted  | int                |
| Main LR name  | `/lr/config/name`        | string                  | string             |
| Meter sub     | `/meters/1` (send empty) | —                       | `/meters/N` blobs  |

Notes:

- Fader wire range is linear `0.0..1.0`; UI dB mapping is a frontend concern.
  Desk quantizes fader writes (~5e-4 steps — readback differs from the
  written value; send levels and FX params are bit-exact).
- `/mix/on` is inverted vs mute: `1` means channel ON (unmuted).
- X32 uses `/main/st/mix/fader` for mains; X-Air uses `/lr/*`.
  Hardware-confirmed on XR18 FW 1.20 (2026-10-17): `/main/st/mix/fader`
  gets NO reply. Driver uses `/lr/*` only. `/lr/config/name` follows by
  analogy with `/ch`. VERIFIED 2026-10-18: `/lr/mix/on` answers queries
  (reads `1` unmuted) and the full `main_mute` write->readback path
  round-trips on the owner rig (same-value; the audible `true` direction
  uses the identical leaf/codec as channel mutes).
- VERIFIED 2026-10-18: per-channel solo writes to `/-stat/solosw/NN`
  (1-based; stereo-linked partners follow, e.g. soloing ch16 sets bits 14
  + 15) and `/-action/clearsolo` clears the 54-int bank, read back via
  `/-stat/solosw`. `/-stat/solo`, `/lr/mix/solo`, `/ch/NN/mix/solo`
  and `/bus/N/mix/solo` are all silent (no desk leaf).
- VERIFIED 2026-10-18: the `/meters/1` blob is 84 B = 42 LE-i16 samples
  with NO header (whole-packet parse); index 0 = ch 1 (ch1 mic noise floor
  reads ~40 counts while the silent rig holds all other indices at 0).
  Bus/main/FX blob positions are NOT mapped (silent rig) — the driver and
  UI treat unverified families as unknown/silent, never as another
  strip's level.

## Address table: param plane (rig-verified 2026-10-18 unless noted)

Implemented in `proto-xair` (`eq.rs`, `gate.rs`, `comp.rs`, `mix.rs`), covered by
strict fake-desk loopback tests. Rows marked VERIFIED were read back from
the owner XR18 (FW 1.20) through the driver param path (readbacks in
`docs/xr18-baseline.md` §7); the rest are community-documented and say so
(checklist in `docs/LIVE-RIG.md`). Value units below are engineering units
(dB, Hz); the wire floats are an implementation detail.

| Function        | Address                      | Args (set)              | Reply (get) | Engineering mapping |
|-----------------|------------------------------|-------------------------|-------------|---------------------|
| Ch pan          | `/ch/NN/mix/pan`             | float `0..1`            | float       | `-100..+100` linear. VERIFIED (ch1 reads centre `0`) |
| Ch EQ on        | `/ch/NN/eq/on`               | int `1`/`0`            | int         | bool. VERIFIED (ch1 `true`) |
| Ch EQ1..4 gain  | `/ch/NN/eq/N/g`              | float `0..1`            | float       | `-15..+15` dB linear. VERIFIED |
| Ch EQ1..4 freq  | `/ch/NN/eq/N/f`              | float `0..1`            | float       | `20..20000` Hz log. VERIFIED (eq1 `124.7` Hz) |
| Ch EQ1..4 Q     | `/ch/NN/eq/N/q`              | float `0..1`            | float       | `0.3..10` INVERTED log (wire 0 = Q 10). VERIFIED (eq1 Q `2`) |
| Ch EQ1..4 type  | `/ch/NN/eq/N/type`           | int `0..=5`             | int         | LCut,LShv,PEQ,VEQ,HShv,HCut. VERIFIED (eq1 type `2`) |
| Ch gate on      | `/ch/NN/gate/on`             | int `1`/`0`            | int         | bool. VERIFIED (ch1 `true`) |
| Ch dyn on       | `/ch/NN/dyn/on`              | int `1`/`0`            | int         | bool (comp on). VERIFIED (ch1 `false`) |
| Bus send level  | `/ch/NN/mix/0N/level` N=1..6 | float `0..1`            | float       | X-Air fader law dB (`-90` = `-inf`). VERIFIED (write+restore, baseline §6 row 4) |
| FX send level   | `/ch/NN/mix/0N/level` N=7..10| float `0..1`            | float       | X-Air fader law dB (`-90` = `-inf`). Community-documented (no completed write test on record) |

Sources: XR18 firmware OSC map FW 1.14 (xr-meters `parameters.txt`:
`mix/pan lin(101)`, `eq/ on + 1..4 {type,f,g,q}`, `gate/on`, `dyn/on`,
`mix/01..n/level` with `10` on ch, send block listing `level`, `grpon`,
`tap` (+ `pan` on odd slots) and no send-mute leaf); xair-api-python
`shared.py` + `kinds.py` (XR18 `num_bus = 6`, `num_fx = 4` → slots
`01..=10`; `Send.level` + `EQBand` codecs incl. the inverted Q wire);
QLC+ forum X-Air thread (slots 1–6 = bus sends, 7–10 = FX sends, seconding
the 6+4 split); Maillot's unofficial X32 OSC doc (`/tmp/x32.pdf`) for the
`/mix/pan` node shape, the send `level` fader law, the six EQ type
names/order and the gate/dyn leaf vocabulary (`thr`, `range`, `attack`,
`hold`, `release`) only. Per-send mute (`/ch/NN/mix/MM/on`) is silent on
hardware (baseline T7, re-confirmed by the firmware map's send-block leaf
list — no `on` leaf exists), so `auxN_mute`/`fxN_mute` and the v2
`sendN_on` stay out of `caps`.

## Address table: catalogue v2 additions (rig-verified 2026-10-18 unless noted)

Same root rule everywhere: channel leaves use `/ch/NN`, bus `/bus/N`
(single digit), FX return `/rtn/N` (there is NO `/fxrtn` leaf — the desk
is silent there), FX-send master `/fxsend/N`, main `/lr`, DCA
`/dca/N` with fader/on directly underneath (no `/mix` segment — the desk
is silent on `/dca/N/mix/*`). See `XAIR_TOPOLOGY` in `proto-xair`.
Channel-only leaves (headamp, input routing) keep `/ch/NN` + `/headamp/NN`.
VERIFIED rows were read back from the owner XR18 (FW 1.20) through the
driver param path (readbacks in `docs/xr18-baseline.md` §7); the rest are
community-documented and say so (checklist P12..P14 in `docs/LIVE-RIG.md`);
loopback-tested against the strict fake desk either way.

| Function | Address pattern | Args (set) | Engineering mapping |
|----------|-----------------|------------|---------------------|
| Strip colour | `{root}/config/color` | int `0..=15` | enum (name-bar). VERIFIED: `/ch/01`, `/ch/16`, `/lr` answer; ch2 same-value write exact |
| Strip icon | — (no firmware leaf; live-silent) | — | OUT of caps |
| Stereo link | global `/config/chlink/1-2…`, no per-channel leaf (live-silent) | — | OUT of caps |
| Gate thr/range | `{root}/gate/thr`, `{root}/gate/range` | float `0..1` | `-80..=0` lin(161) / `3..=60` lin(58) dB. VERIFIED (ch1 `-41` / `60`) |
| Gate att/hold/rel | `{root}/gate/attack|hold|release` | float `0..1` | `0..=120` lin(121) / `0.02..=2000` log(101) / `5..=4000` log(101) ms. VERIFIED (ch1 `12` / `126.2` / `752.1`) |
| Gate mode/key | `{root}/gate/mode|keysrc` | int | `0..=4` (GATE, EXP2, EXP3, EXP4, DUCK) / `0..=22`. VERIFIED (ch1 `0` / `0`) |
| Gate filter | `{root}/gate/filter/on|type|f` | int / int / float | on `0..=1`, type `0..=8`, f log `20..20000` Hz. VERIFIED (ch1 `0` / `6` / `990.9` Hz; ch2 same-value write exact on type) |
| Comp thr/makeup/mix | `{root}/dyn/thr|mgain|mix` | float `0..1` | `-60..=0` lin(121) / `0..=24` lin(49) dB, `0..=100` lin(51) %. VERIFIED (ch1 `0` / `0` / `100`) |
| Comp ratio/knee | `{root}/dyn/ratio|knee` | INT index / float | ratio = INT step index `0..=11` into `1.1, 1.3, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10, 20, 100` (NOT a direct float — a direct-float codec misreports). VERIFIED: ch1 wire `5` → `3.0`; ch2 same-value write exact. Knee `0..=5` lin(6). VERIFIED (ch1 `1`) |
| Comp att/hold/rel | `{root}/dyn/attack|hold|release` | float `0..1` | `0..=120` lin(121) / `0.02..=2000` log(101) / `5..=4000` log(101) ms. VERIFIED (ch1 `10` / `10.0` / `151.2`) |
| Comp mode/det/env/auto | `{root}/dyn/mode|det|env|auto` | int | `0..=1` each (comp/exp, peak/rms, lin/log, on/off). VERIFIED (ch1 `0` / `0` / `1` / `false`) |
| Comp key/filter | `{root}/dyn/keysrc`, `{root}/dyn/filter/on|type|f` | int / int/int/float | key_src `0..=22`; on `0..=1`, type `0..=8`, f log `20..20000` Hz. VERIFIED (ch1 `0` / `0` / `6` / `990.9` Hz) |
| Send tap | `/ch/NN/mix/MM/tap` | int `0..=5` | IN, PREEQ, POSTEQ, PRE, POST, GRP. VERIFIED: bus slots read `2` (POSTEQ), FX slots `4` (POST); ch2 same-value write exact |
| Send pan | `/ch/NN/mix/MM/pan` | float `0..1` | `-100..+100` linear (desk lists odd slots only; driver addresses every slot — slot 1 verified answering) |
| Send on | `/ch/NN/mix/MM/on` — NO leaf (live-silent, T7) | — | OUT of caps |
| Ch/bus/ret/main fader | `/ch/NN\|/bus/N\|/rtn/N\|/lr/mix/fader` | float `0..1` | linear 0..1. VERIFIED (incl. `/rtn/1` `0.7497556`; ch2 same-value write exact) |
| Strip mute+name | `…/mix/on`, `…/config/name` | int / string | by analogy with `/ch`. Mute VERIFIED on `/rtn/1` (muted) and `/dca/1` (on); ch2 same-value write exact |
| DCA fader/on | `/dca/N/fader`, `/dca/N/on` (NO `/mix` segment) | float / int | VERIFIED: `/dca/1` fader `0.7497556`, on `1` |
| Bus/main EQ 5..6 | `/bus/N\|/lr/eq/5..6/{g,f,q,type}` | float/float/float/int | `-15..+15` dB lin, `20..20000` Hz log, `0.3..10` inv-log Q, type `0..=5`. VERIFIED: all 8 leaves on `/bus/1` + `/lr` eq5_gain (channels/returns carry 4 bands — ch `eq5_gain` is scope-rejected driver-side) |

Honest exclusions (NOT in `caps`, UI collapses): `delay_on`/`delay_ms`
+ `hpf_slope` (X32-only), `source` (`insrc` live-silent on FW 1.20),
`icon` (no firmware leaf), `link` (global `/config/chlink/1-2…`, no
per-channel leaf), `send1..10_on` + all legacy `auxN_mute`/`fxN_mute`
(the send block has no on/off leaf and `/ch/NN/mix/MM/on` is live-silent
— baseline T7), `send11..16_*` (only 6+4 send slots), per-channel `eq5_*`
(channels and aux returns carry 4 bands; `eq5/6_*` ride buses + main only).

## R2 engine capabilities on X-Air (mostly UNVERIFIED-ON-RIG)

Engine-side behaviour that needs no new wire reads (works the day the
desk answers), vs behaviour that needs a verified address first. Every
row without a VERIFIED tag is UNVERIFIED-ON-RIG until checklist P19+ in
`docs/LIVE-RIG.md` says otherwise. (The 2026-10-18 driver-path probe
covered the param plane only — meter subscribe/renewal rows below are
covered by the earlier meter findings, not that probe.)

| Function | Engine path on xair | Address / note |
|----------|---------------------|----------------|
| Device discovery (`scan`) | OSC broadcast probes via `transport::discovery` (`/xinfo`, `/info`) behind the engine `DeviceScanner` trait | `discovery.rs` `PROBE_TABLE`; reply parse `parse_info_reply` — probe loop never ran against a real desk from the engine |
| Link latency (`link.latency_ms`) | timed `get_main_fader` (`/lr/mix/fader`) query per `link` emit | fader address rig-verified; the timing loop itself is not |
| Info (`info`) | `/info` query → `info_from_strings` (name/model/firmware) + assumed 48 kHz | `/info` order `[proto-rev, name, model, firmware]` rig-verified (T5); 48 kHz assumed, never read from the desk |
| Numbered scenes | engine-side full-plane capture/recall over the param plane (faders ride the legacy fade) | no desk-native snapshot used; capture walks the poll list (~2k pairs — slow on hardware, prefer short `fade_ms` tests) |
| FX `fx_param parNN` | `/fx/N/par/MM` float writes (1-based both axes) | param write path loopback-tested; live FX par shapes verified 2026-10-18 (baseline "Live FX par shapes") but the engine `fx_param` command never ran them |
| FX type/on/bypass | type READS live (engine polls `/fx/N/type` into `Event Fx` + `caps.fx` tables); type-select/on/bypass writes NOT implemented (error) — no verified mapping | `/fx/N/type` int `0..=60` reads VERIFIED 2026-10-19 (slots VRM/HALL/D/FL/PIT = 7/0/26/60 on the owner rig, query-only); writes/type-select never attempted live |
| Routing patch | `supported:false` — `insrc` live-silent on FW 1.20 (baseline T7) | `/ch/NN/config/insrc` stays out of `caps` until a rig hears a reply (P22) |
| Offline push target | normal `set_param` path per id | per-id verification = its own P-row; push itself is engine-side |
| Taps `post` | `/meters/1` blob cache (same source as `poll_meters`) | blob endianness/rate rig-verified; per-strip index mapping assumed from the blob order |
| Taps `gr_gate`/`gr_comp` | `/meters/6` DYN blob cache (`gate_gr`/`comp_gr` param path) | community-documented only (fw 1.11 doc), never seen on the owner rig |
| Taps `in` | UNAVAILABLE on xair (not in `caps.meter_taps`) — no `/meters/0` subscribe | would need a per-channel `/meters/0` subscribe the driver does not do |
| Poller pipelining | engine-side concurrency (`max_inflight`); xair default 1 | raise only after P19 measures a sustainable query rate — never assumed |

## Meters

- Subscribe: send `/meters` with ONE string arg `/meters/1` (bare/empty
  queries to `/meters/1` get silence — baseline T4). Mixer streams blobs
  addressed `/meters/1` (also `/meters/2`, `/meters/3` when subscribed)
  at ~20/s with /xremote + resubscribe every 8 s.
- Blob payload (`/meters/1`, rig-verified 2026-10-18): exactly 84 B = 42
  **little-endian i16** samples, NO header; level = `raw / 32767` clamped
  to `0.0..1.0`. Index 0 = ch 1; indices past the 16 channels are NOT
  mapped (silent rig — never assume the blob order for bus/main).
- Parsing lives in `openmix-transport::meter` (`parse_meter_message`,
  `MeterRing`) and also accepts float/int arg-list forms so future providers
  reuse it. The recv loop + broadcast hub is `OscSession`; the driver only
  calls `subscribe_meters()`.

## What lives where (modularity rule)

- `openmix-transport`: OSC encode/decode (`osc.rs`), UDP session +
  request/response + keepalive (`session.rs`/`udp.rs`), meter parse + ring
  (`meter.rs`/`meters.rs`), TCP framing (`tcp.rs`), reconnect backoff
  (`reconnect.rs`/`backoff.rs`), discovery probes (`discovery.rs`).
- `proto-xair`: **only** the table above + arg mapping + `MixerInfo` mapping
  (`addrs` module, `info_from_strings`). No sockets, no loops.
- A new provider = new crate implementing `MixerDriver` + address table.
  Zero changes to core/transport.

## Capture checklist (when hardware is available)

1. Own LAN only. XR18 + FW version from Setup screen.
2. `openmix-cli discover --timeout 3000` → confirm `/xinfo` reply shape.
3. `openmix-cli --driver xair --addr <ip> list` → confirm `/info` strings.
4. Move one fader on the mixer while subscribed; confirm `/meters/1` blob
   endianness (LE i16 assumed) and `/ch/XX/mix/fader` echo format.
5. Record any deviation here before changing code.

## Live hardware observation (2026-10-17, owner rig)
- Unit: XR18 "Home Studio" @ 192.168.40.123, FW 1.20 (docs assumed 1.22 — table holds, note version).
- /xinfo reply: (ip, name, model, fw) as 4 strings. Discovery broadcast won't cross subnets; use directed /xinfo.
- ch1: name "BASS", fader float ~0.196, /mix/on = 0 (muted). ch2 fader 0.0.
- BUG FOUND: /main/st/mix/fader gets NO reply on X-Air. Main LR on X-Air is /lr/mix/fader (X32 uses /main/st). Driver must use /lr/* for X-Air mains.
- FIXED 2026-10-17: `proto-xair` mains routed via `/lr/*` (`MAIN_FADER = /lr/mix/fader`, plus `MAIN_ON = /lr/mix/on`, `MAIN_NAME = /lr/config/name` by analogy, not yet live-queried). Confirmed on FW 1.20 hardware. Loopback test `main_fader_uses_lr_not_main_st` uses a strict fake answering only `/lr/mix/fader` (silent on `/main/st`, like the real desk).

## Meter subscribe — HARDWARE VERIFIED 2026-10-18
- Subscribe = send `/meters` with ONE string arg `/meters/1` (typetag `,s`). Bare/empty queries to /meters/0 or /meters/1 get silence.
- Desk streams LE-i16 blobs (~104B for /meters/1). Observed only ~5 blobs/12s without keepalive renew — driver must repeat /xremote + resubscribe (10s expiry suspected, unconfirmed).

## Meter renewal — HARDWARE VERIFIED 2026-10-18 ~01:30 AWST
- Without renewals: ~5 blobs/12s (dribble). With /xremote + resubscribe every 8s: 601 blobs/30s = ~20/s (~50ms rate confirmed).
- 8s METER_RENEW_INTERVAL is now the driver default. 10s expiry still suspected-not-proven; 8s gives margin either way.

## Meter expiry — HARDWARE PROVEN 2026-10-18 ~00:50 AWST
- One subscribe, no renewals: [200, 0, 0, 0, 0, 0, 0] blobs per 10s window over 70s. Stream dies at ~10s sharp.
- 8s METER_RENEW_INTERVAL confirmed correct with margin. No longer INFERRED.

## Headamp — HARDWARE VERIFIED 2026-10-18 ~01:00 AWST (XR18 FW 1.20)
- `/headamp/NN/gain` float linear `0..1` ↔ `-12..+60 dB` (read 0.17 ≈ +0 dB on ch1/16; same-value write ACKed, readback 0.1667 — wire quantizes).
- `/headamp/NN/phantom` int `1`/`0` (read 0 on ch1; write path loopback-only, untested live).
- NN 2-digit 1-based, one headamp per input strip (matches xair-api `headamp.py` mapping).
- Driver: `proto-xair/src/headamp.rs` (`set_param gain/phantom`); UI: GainWizard.
