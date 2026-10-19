# openmix-engine: the single mixer session

Every frontend (browser via the relay, desktop/mobile via Tauri, CLI,
future apps) gets the FULL mixer capability through this crate. The engine
is transport-agnostic: it speaks exactly one protocol — [`Cmd`] in,
[`Event`] out — and adapters (`crates/serve` for WebSocket + static files,
`src-tauri` for IPC) only move `Cmd`/`Event` JSON. They contain no mixer
logic. A feature that is not in the engine does not exist.

Byte-compatibility rule: the JSON is the pre-existing WebSocket protocol.
Existing clients must not notice the move (same `cmd`/`type` tags, same
field names, same shapes).

## Embedding

```rust
use std::sync::Arc;
use openmix_engine::{Cmd, Engine, EngineConfig};

let engine: Arc<Engine> = Engine::new(EngineConfig::default());
engine.connect("sim", None).await?;        // initial driver (adapters stay up on failure)
let mut client = engine.attach(Some("ui".to_string())).await;
client.send(Cmd::Fader { ch: 0, key: None, value: 0.5 }).await?;
// or raw off the socket: client.send_json(r#"{"cmd":"fader","ch":0,"value":0.5}"#).await?;
while let Some(event) = client.events.recv().await {
    socket.send(event.to_json()).await?;
}
client.detach().await; // also runs on drop
```

- `Engine::new(config) -> Arc<Engine>`: pure construction (no runtime
  needed). Background tasks (param poller, lock sweeper, meter ticker)
  start on the first `attach` and exit when the last `Arc` is dropped.
- `attach(label) -> ClientHandle { id, send(Cmd), send_json(&str), events }`:
  `id` is `client-N`; `events` (`mpsc::UnboundedReceiver<Event>`) first
  yields the attach snapshot (`you`, `state`, `shows`, `clients`, `locks`,
  `caps`, `devices`, `link`, `info`?, `scenes`, `fx`, `routing`, then one
  `params` frame per cached channel) in that order, then
  every live broadcast. Unicast frames (replies, denials, command errors)
  arrive only in the sender's mailbox.
- `send(Cmd)` returns `Err` only when detached. Domain errors (bad args,
  denied writes, driver failures) arrive as unicast `error` events — a bad
  command never closes the session.
- `set_client_policy(id, Policy)` hosts restricted roles on the shared
  session (default: admin). Denied writes surface as unicast `error` events.
- The serve adapter maps `ServeOptions` to `EngineConfig` (`--poll-ms` to
  the poll tick, lock TTL to the sweeper) and pumps text lines both ways.
  The Tauri adapter (ME2) will call `send` from `invoke('engine_cmd')` and
  forward `events` over a `Channel`/event.

## Commands (`Cmd`, `{"cmd":...}`)

| cmd | fields | reply |
|-----|--------|-------|
| `connect` | `driver` (`sim` default), `addr?` | unicast `you` + broadcast `state`, `caps` (cache cleared) |
| `fader` | `ch`, `value` 0.0..=1.0, `key?` (stable strip key, wins over `ch`) | broadcast `state` (`by` = writer) |
| `main_fader` / `set_main_fader` (alias) | `value` 0.0..=1.0 (validated) | broadcast `state` (`by`); not strip-locked |
| `mute` | `ch`, `muted`, `key?` | broadcast `state` (`by`) |
| `param` | `ch`, `field`, `value` (number\|bool\|string), `key?` | broadcast confirmed `param` (`by`) + `state` |
| `focus` | `ch` **or** `chs:[...]` (indices or stable keys) | none on success; `error` when all out of range (partial: valid focus + `error` naming the ignored) |
| `lock` / `unlock` | `ch`, `key?` | broadcast `locks`; contending/foreign ops denied with `locked_by` |
| `hello` | `binary_meters?` (bool), `taps?` (bool) | unicast `hello` ack with the current preferences (never broadcast) |
| `list_shows` | — | unicast `shows` |
| `capture_scene` | `show`, `name` (trimmed, non-empty) | broadcast `shows` |
| `recall_scene` | `show`, `name`, `fade_ms?` (default 400, 0 jumps) | broadcast `param` (every applied value) + `state`; unicast `recalled` when foreign pins were overridden |
| `delete_scene` | `show`, `name` | broadcast `shows` |
| `scan` (R2) | — | broadcast `devices` (fresh discovery) |
| `connect_device` (R2) | `id` | unicast `you` + broadcast `state`, `caps`, `link`, `info`, `fx`, `routing`; `error` on unknown ids and I/O boxes |
| `disconnect` (R2) | — | broadcast `link` (`disconnected`); the session survives |
| `info` (R2) | — | unicast `info` |
| `scene_save` (R2) | `n` (1..=999), `name` | broadcast `scenes` |
| `scene_recall` (R2) | `n`, `fade_ms?` (default 400, 0 jumps), `scope?` (`strips?`, `strip_keys?`, `groups?`, `globals?`) | broadcast `param` (every applied value) + `state` + `scenes`; unicast `recalled` when foreign pins were overridden |
| `scene_rename` (R2) | `n`, `name` | broadcast `scenes` |
| `scene_delete` (R2) | `n` | broadcast `scenes` |
| `scene_clear` (R2) | — | broadcast `scenes` (library emptied) |
| `fx_set_type` (R2) | `slot`, `type` | broadcast `fx` |
| `fx_param` (R2) | `slot`, `id`, `value` 0.0..=1.0 | broadcast `fx` |
| `fx_bypass` (R2) | `slot`, `bypass` | broadcast `fx` |
| `fx_on` (R2) | `slot`, `on` | broadcast `fx` |
| `patch_set` (R2) | `strip`, `group` (`Lcl`\|`Net`\|`USB`), `index` (1..=32), `key?` | broadcast `routing` |
| `patch_clear` (R2) | `strip`, `key?` | broadcast `routing` |
| `offline` (R2) | `model` (`sim`\|`xair`\|`dl32s`) | broadcast `state`, `caps`, `link` (`offline`), `fx`, `routing` |
| `offline_push` (R2) | `confirm` (must be `true`) | broadcast `param` (every pushed value) + `state`; errors without a pending diff or without confirm |
| `offline_discard` (R2) | — | broadcast `offline_diff` (`count: 0`) |
| `bus_fader` (R2) | `bus` (0-based), `value` 0.0..=1.0 | broadcast full-state `bus` (both fields re-read) |
| `bus_mute` (R2) | `bus` (0-based), `muted` | broadcast full-state `bus` |
| `subgroup_fader` (R2) | `group` (0-based), `value` 0.0..=1.0 | broadcast full-state `subgroup` |
| `subgroup_mute` (R2) | `group` (0-based), `muted` | broadcast full-state `subgroup` |
| `subgroup_assign` (R2) | `ch`, `group` (0-based), `member`, `key?` | broadcast `subgroup_assign` echo; channel-lock checked |
| `mute_group` (R2) | `group` (0-based), `muted` | broadcast `mute_group` (re-read) |
| `mute_group_assign` (R2) | `ch`, `group` (0-based), `member`, `key?` | broadcast `mute_group_assign` echo; channel-lock checked |
| `file_download` (R2) | `filename` | unicast `file` with base64 `data` |
| `file_upload` (R2) | `filename`, `data` (base64, ≤16 MiB), `file_type` | broadcast `file` ack (`data` absent, `size` = bytes written) |

Driver coverage: mix-bus masters ride every driver (sim mix strips,
XR18 `/bus/N`, DL32S aux masters). Subgroup / mute-group / file transfer
are DL32S-native and sim-modelled in-memory; the XR18 has no subgroup,
mute-group or file leaf on its OSC map, so those commands answer
`error ... not supported` there — honest `unsupported`, never faked
(`caps.groups` counts are 0 and `caps.files.supported` is false on xair).
"Create" needs no command: group counts are fixed per desk family.

Unknown `cmd` → unicast `error` (`unknown cmd "bogus"`); malformed JSON →
`error` (`invalid JSON: ...`); wrong shapes → `error` (`invalid command:
...`). Channels are 0-based (`StripId`).

## Events (`Event`, `{"type":...}`)

- `you`: `{"type":"you","id":"client-1"}` — unicast on attach and as the
  `connect` reply, before `state`, so the client learns the id its later
  `by` tags carry. Distinct per client.
- `state`: `{"type":"state","driver":"sim","strips":[{"ch","key"?,"name",
  "fader","muted"}],"main_fader":0.8|null,"by":"client-1"?}` — full snapshot on
  attach/connect and after every mutation. `main_fader` is `null` when the
  driver has none; `by` is absent on snapshots. LR main mirrors the Tauri
  `get_state` shape.
- `param`: `{"type":"param","ch":1,"key"?,"field":"gain","value":12.5,
  "by":"client-1"?}` — the confirmed (re-read) value after a `param`
  write, and once per applied value on `recall_scene`.
- `shows`: `{"type":"shows","shows":[{"name":"gig","scenes":[{"name":
  "verse","main_fader":0.8,"strips":[{...}]}]}]}` — server-side library
  (survives reloads); sent on attach, as the `list_shows` reply, and after
  `capture_scene`/`delete_scene`.
- `meters`: `{"type":"meters","levels":[...]}` — one entry per strip, at
  the meter SOURCE rate (50 ms / 20 Hz, the desk rate; `METER_INTERVAL` in
  `openmix-transport`). Never downsampled by the engine or the adapter.
- `spectrum`: `{"type":"spectrum","ch":0,"bins":[...64 floats in 0..1...]}` —
  per tick, first 16 channels only.
- `gr`: `{"type":"gr","ch":0,"gate":0.0,"comp":0.0}` — per tick, first 16
  channels; dB reduction `>= 0` (0.0 = no reduction / no data yet). Reads
  are cache-only (no extra wire traffic), so the per-tick cadence is free.
- `hello`: `{"type":"hello","binary_meters":true}` — unicast ack to the
  `hello` command, carrying the client's current binary-meter preference.
  R2 adds `"taps":true` when the client negotiated meter taps (skipped on
  the wire while false, so the MR1 ack stays byte-identical).
- `devices` (R2): `{"type":"devices","list":[{id,name,model,ip,
  kind:"console"|"io",state:"available"|"connected"|"in_use",fw?}...],
  "scanned_at":<unix secs>}` — discovery result (sim: the design's three
  devices; xair: LAN discovery, UNVERIFIED-ON-RIG). Broadcast after every
  `scan`; unicast in the attach snapshot.
- `link` (R2): `{"type":"link","state":"connected|disconnected|offline",
  "name","ip","fs":48000,"latency_ms":1.2}` — re-emitted on every link
  change and every ~2 s (`LINK_INTERVAL`) with a fresh round-trip sample
  (OSC query timing on hardware, deterministic jitter on sim).
- `info` (R2): `{"type":"info","model","firmware","sample_rate",
  "console_name","driver"}` — desk identity, on connect and as the `info`
  reply. Errors (unicast) when the driver cannot answer (hardware while
  unconnected).
- `scenes` (R2): `{"type":"scenes","scenes":[{number,name,current,
  saved_at}...]}` — numbered library slots `001..999` in number order.
  Broadcast after every scene mutation; unicast in the attach snapshot.
- `fx` (R2): `{"type":"fx","slots":[{slot,type,on,bypass,ret_label,level,
  params:{id:value}}...]}` — whole-rack state (the rack is small; diffs
  ride the same frame). `params` maps design param ids to normalized
  0.0..=1.0 values; `ret_label` is the design return label (`Ret 1/2` …);
  `level` is the return meter 0..1 (sim-measured, 0.0 elsewhere).
- `routing` (R2): `{"type":"routing","patches":[{strip,key?,group,index,
  label}...]}` — one row per patched input strip (`Lcl 01` style labels;
  empty where patching is unsupported).
- `offline_diff` (R2): `{"type":"offline_diff","changes":[{strip,key?,id,
  from,to}...],"count":N}` — the recorded offline change list, emitted on
  connect while changes are pending (nothing pushes without
  `offline_push {"confirm":true}`) and after `offline_discard`
  (`count: 0`). Namespaced ids: `fx/<param>` (FX writes, strip = slot),
  `patch` (routing writes), `main_fader` (strip 65535).
- `taps` (R2): `{"type":"taps","tap":"in|post|gr_gate|gr_comp",
  "levels":[...]}` — one frame per tap per tick (`in` pre-fader,
  `post` post-fader 0..1; `gr_gate`/`gr_comp` dB reduction `>= 0`).
  Broadcast only while at least one client negotiated taps (`hello
  {"taps":true}`); old clients ignore the frame; the adapter encodes it
  as WS BINARY type 3 for taps-negotiated clients.
- `caps`: `{"type":"caps","driver":"sim","params":[{"id","kind"
  (`float`|`bool`|`enum`|`text`),`scope`,`scopes`,`min`,`max`,`unit`,`default`,
  `label"}...],"unsupported":[...],"topology":[{`key`,`kind`
  (`in`|`ret`|`bus`|`fxsnd`|`mtx`|`main`|`dca`),`label`,`first`,`count`,
  `name_prefix`,`color_default`,`params_scope`,`strip_keys`}],"meter_taps":[...],
  "fx":{`slots`,`types`:[{`id`,`label`,`params`:[{`id`,`label`,`unit`,
  `min`,`max`,`default`,`scale`}]}]},"routing":{`groups`:[{`key`,
  `label`,`count`}],`per_strip`,`supported`},"groups":{`buses`,
  `subgroups`,`mute_groups`},"files":{`supported`}}`
  — supported catalogue entries (v2: 176 ids) plus the rest in catalogue
  order; `scope` is the legacy single-scope tag (kept for back-compat),
  `scopes` the v2 strip-kind set the id applies to (the poller filters on
  it: DCA strips get fader/mute/name/color only). `topology` is the
  ordered strip-family map (global strip indices; the first 32 sim inputs
  and 8 buses keep their historic indices) plus `strip_keys`: the stable
  per-strip keys in index order (`ch/01`, …, `bus/01`, …, `main`), which
  survive desk switching where positional indices do not (sim 77 strips,
  xair 35, dl32s 32; sim `mix` normalises to `bus`). `meter_taps` declares the
  meter taps (`in`, `post`, `gr_gate`, `gr_comp`). `fx` is the FX slot
  family (sim: 8 slots + the six design type tables from `core::fx`;
  xair: 4 slots, desk-native types not projected yet; dl32s: 0 slots).
  `routing` declares patch support (sim: Lcl/Net/USB × 32, per-strip;
  xair/dl32s: `supported:false` — `insrc` is live-silent on FW 1.20).
  `groups` counts the master-section commands the driver backs (sim:
  8/6/6; xair: 6/0/0; dl32s: 8/6/6). `files` flags desk file transfer
  (sim/dl32s: true; xair: false).
  `min`/`max` are `null` for bool/text. Broadcast on `connect` and when
  a param drops out (below); unicast to every newly attached client.
  Additive: old fields are byte-identical.
- `params`: `{"type":"params","ch":3,"key"?,"values":{"gain":12.5,...}}` —
  engineering units (dB, Hz, whole-number enums), diff-only after the
  attach snapshot (one frame per cached channel).
- `locks`: `{"type":"locks","holds":{"0":"client-1"}}` — channel keys are
  strings; broadcast on every change including expiry.
- `clients`: `{"type":"clients","count":2,"writers":["client-1"]}` —
  broadcast on join/leave; `writers` holds ids with a mutation in the last
  10 s (sorted). Labels stay server-side; the authoritative self id comes
  from `you`.
- `error`: `{"type":"error","message":"..."}` plus `locked_by`/`ch` on
  lock denials. Unicast to the offending client only.
- `recalled`: `{"type":"recalled","overrode_locks":[1]}` — unicast to the
  recaller when `recall_scene` changed strips pinned by someone else
  (recall bypasses pins).
- `bus`: `{"type":"bus","bus":0,"fader":0.75,"muted":false}` — full
  master state after a `bus_fader`/`bus_mute` write (both fields re-read).
- `subgroup`: `{"type":"subgroup","group":1,"fader":0.7,"muted":true}` —
  subgroup master echo.
- `subgroup_assign`: `{"type":"subgroup_assign","ch":0,"group":1,
  "member":true}` — membership echo.
- `mute_group`: `{"type":"mute_group","group":0,"muted":true}` — mute-group
  master echo (re-read).
- `mute_group_assign`: `{"type":"mute_group_assign","ch":5,"group":2,
  "member":false}` — membership echo.
- `file`: `{"type":"file","filename":"Show.zip","size":4,
  "file_type":0,"data":"AAECAw=="?}` — `file_download` replies carry
  base64 `data` (`size` = decoded bytes); `file_upload` replies are acks
  (`data` absent, `size` = bytes written). `data` is skipped on the wire
  while absent.

Unicast vs broadcast: `you`, `list_shows` replies, denials, `recalled`,
command errors → sender only. Everything else → every client.

## Ordering guarantees
- Attach snapshot order: `you`, `state`, `shows`, `clients`, `locks`,
  `caps`, `devices`, `link`, `info` (when the driver answers), `scenes`,
  `fx`, `routing`, `params…`. The R1 six-frame prefix is unchanged.
- A command's unicast reply (or error) is delivered to the sender *before*
  that command's broadcasts (mirrors the relay socket, which sent the
  reply inline before flushing broadcasts). Broadcasts from one command
  keep apply order (`param` echoes, then `state`).
- Background broadcasts (poller diffs, `caps` dropout updates, sweeper
  expiry, meter ticks) interleave arbitrarily — clients must match on
  content, never position (the WS tests do; the web `Client` inbox does).
- Meter frames never delay fader traffic: writes run inline on the
  caller's task while the poller and the meter ticker run on their own
  tasks (see "Write priority" below).

## Write priority (MR1: writes preempt poller and meter work)

Three lanes, no shared queue:

- **Write lane**: `ClientHandle::send` / `send_json` (or the adapter's
  `Engine::handle_json`) runs `apply` inline on the caller's task — a
  fader/mute/param/main write never waits on poller or meter state.
- **Poller lane**: one query per `poll_interval` tick on its own task.
  Before issuing the query it checks `writes_in_flight` (an atomic
  incremented for the whole duration of `apply`, RAII-guarded so every
  return path releases it) and **skips the tick** while any command is in
  flight, counting `poll_skips`. So a write is never queued behind a poll
  read; the poller resumes on the next tick. Focus entries just wait one
  tick longer.
- **Meter lane**: the ticker task only makes read-only driver calls
  (`poll_meters`, cache-only GR reads) and short lock-free broadcasts, so
  it never blocks a write either.

On hardware the remaining bound is the driver's own serialization (e.g.
the X-Air `query_lock` shared by reads and writes): the poller skip
minimizes that contention, and the MR1 latency test measures dispatch at
the engine→driver call. LAN budget: UI send → engine → mixer UDP packet
out <= 5 ms p99 (loopback <= 2 ms).

## Binary meter frames (MR1)

Default off, so the old UI keeps getting byte-identical JSON `meters` /
`gr` / `spectrum`. A client opts in with `{"cmd":"hello",
"binary_meters":true}` and gets a unicast `{"type":"hello",
"binary_meters":true}` ack; from then on the adapter sends `Meters`/`Gr`
events as WS BINARY messages instead of JSON text. `{"cmd":"hello"}`
without the flag just re-acks; `{"cmd":"hello","binary_meters":false}`
opts back out.

`Event::to_binary(seq, t_ms)` is the single encoder (transport-agnostic:
the WS adapter and the future Tauri adapter share it; JSON frames stay
byte-identical for clients that did not opt in). Layout, little-endian:

```
[type (1=meters, 2=gr, 3=taps), version=1, n u16, seq u32, t_ms u32]
```

then for meters `n × u16` (linear 0..1 mapped to 0..65535, clamped,
rounded; decode divides by 65535), for gr `n × (u8 ch, u8 gate dB×4,
u8 comp dB×4)` (currently one entry per frame: each per-channel `Gr`
event encodes as `n=1`; gate/comp clamp to 0..=63.75 dB at 1/4 dB
resolution, decode divides by 4), for taps `[u8 tap id]` then `n × u16`
(tap ids: `in`=0, `post`=1, `gr_gate`=2, `gr_comp`=3; `in`/`post` ride
the meter codec, GR dB values scale by `TAPS_GR_DB_FULLSCALE` = 24 dB
and clamp — sim GR taps peak near 12 dB, so 24 dB keeps headroom;
decode divides appropriately via `u16_to_taps_level`).

`seq` is a per-client monotonic frame counter (adapter-owned,
wrapping `u32`); `t_ms` is the engine monotonic clock in ms
(`Engine::now_ms`, wraps at ~49.7 days — never wall time). `spectrum`
stays JSON in both modes (it is already small and typed).

Golden binary fixtures live in `tests/golden_bin/*.bin.hex` (NOT
`tests/golden/*.json`: that directory is consumed 1:1 by the TypeScript
parser test, which rejects unknown frame types). `tests/realtime.rs`
validates them against `to_binary` and decodes them with a reference
decoder that `apps/web/src/lib/binaryFrames.test.ts` mirrors in
TypeScript. R2 taps binaries live alongside the R2 JSON fixtures in
`tests/golden_r2/` (`taps_in.bin.hex`, `taps_gr.bin.hex`, validated by
`tests/golden_r2.rs`, which also carries every R2 command/frame JSON
fixture — same split-directory reason).

## Latest-only delivery and `stats` (MR1)

Per client, per stream (`meters`, `gr`, `spectrum`) the adapter holds
bounded latest-only slots: one frame for `meters` (a single frame already
covers every strip), one frame per channel for `gr`/`spectrum` (a single
slot per stream would discard 15 of 16 channels every tick; the bound that
matters is "latest frame per channel series, never a queue"). When the
socket is slow, the old frame is replaced by the new one — stale frames
are dropped, never queued. Drops are counted per client and exposed via a
`stats` event on request (adapter-level diagnostics, hence answered by
the adapter, not the engine):

```
-> {"cmd":"stats"}
<- {"type":"stats","meters_sent":N,"meters_dropped":N,
    "gr_sent":N,"gr_dropped":N,
    "spectrum_sent":N,"spectrum_dropped":N,
    "queue_depth":N,"binary":true}
```

`queue_depth` is the currently held frames (occupied stream slots +
pending control frames). Control frames (`state`, `params`, `locks`,
…) keep reliable ordered delivery and are never dropped. `binary`
echoes the client's `hello` preference. The `stats` shape example is
pinned in the serve golden file
(`crates/serve/tests/golden/stats.json`); `stats` is intentionally NOT
an engine command (an in-process client gets `unknown cmd "stats"`) —
per-connection counters only exist where the socket is.

## Catalogue (v2)

176 ids (`openmix_core::params::CATALOGUE`): the v1 set (fader/mute/name,
gain/phantom/usbreturn/trim/polarity/pan/hpf, gate/comp/eq on, 4-band EQ,
legacy `aux1..8`/`fx1..4` sends — kept verbatim for the v3 UI) plus strip
look (`color` 0..15, `icon`, `link`, `source`), `delay_on`/`delay_ms`
(X32-only), `hpf_slope` (X32-only), the full gate set (`gate_thr/range/att/
hold/rel/mode/key_src/key_filter`), the full comp set (`comp_thr/ratio/
knee/makeup/att/hold/rel/mode/det/env/mix/auto`, ratio as number
`1.1..=100`), EQ band 5, and generic `send1..16_level/tap/pan/on`
(`send1..6` alias the bus sends, `send7..10` the FX sends on X-Air;
`send11..16` are sim-only). Drivers advertise honest subsets
(sim 176, xair 117, dl32s 3); unsupported ids land in
`caps.unsupported` and the UI collapses them. Ranges come from the
catalogue via `caps` — never invented.

## Poller budget and focus

One driver query per `poll_interval` (production: 50 ms = 20 queries/s
total) at default pipelining. The poll list is built once per driver
(topology × capabilities × catalogue scopes) and drains strip-major:
every param of strip N before N+1, so one strip's whole set lands as a
block. Only changed values broadcast (diff); after a `param` write the
value is re-read and the confirmed value broadcast (`by` = writer). A
read failing as unsupported/unknown/timed-out drops that (driver, param)
pair from the poll set (skipped in place — one tick — never rebuilt) and
rebroadcasts `caps` once with it in `unsupported` (no spam). Timeouts are
tolerant: a pair drops only after 3 consecutive timeouts with no success
in between (`POLL_TIMEOUT_STRIKES`), so one lost datagram on venue wifi
never evicts a live param; out-of-scope reads (`... out of scope for ... strips`) and no-data errors (empty sim
slots, unpushed caches, not-connected) never drop — the poller
pre-filters by scopes, so an out-of-scope error only fires on direct
misuse. Transient I/O keeps polling silently.

`focus` moves strips to the FRONT of the shared queue (most-recent-first:
a new focus supersedes older ones, pending (strip, param) pairs are
de-duplicated, re-read at the same budget, then round-robin resumes). The
fresh set drains core-first across the whole focused set: `fader`+`mute`
of every strip first, then `name`+`pan`, then the remaining scoped ids in
catalogue order — a 16-strip mixer bank is fader-live after ~32 ticks
(≈1.6 s at the default budget) instead of waiting out a full strip-major
catalogue sweep. A single-strip focus still drains as one unbroken block
ahead of the background. The queue is bounded (~4 sweeps, oldest dropped
from the back)
and a detached client's pending focus is dropped with it, so a gone
client's flood never starves the next focus. Flood-then-focus ordering is
covered by `focus_flood_superseded` (engine) and `ws_focus_flood_superseded`
(serve WS).

Cold full-sweep times (static list sizes; why focus matters — background
keeps 20 q/s at xair/sim defaults, `max_inflight` 4):

| driver | strips | pairs | full sweep |
|--------|--------|-------|------------|
| sim    | 77     | 8212  | ~7 min |
| xair   | 35     | 2218  | ~2 min |
| dl32s  | 32     | 96    | ~10 s (lanes alternate whole ticks at `max_inflight` 1) |

The essentials lane (~150 pairs on xair: every strip's
fader/mute/name/pan/color + xair `solo`) rides ~60 q/s alongside the
background at xair/sim defaults, so faders/mutes/names converge in ~3 s
even though the deep sweep above still takes minutes. Visible/focused
strips drain first; the sweep above is the slow
background. Per-family pair counts are reported by the v2 verification
script (see the R1 report).

## Poller pipelining (R2: first-view latency)

The poller issues up to `max_inflight` driver queries per tick
(`EngineConfig::poll_inflight`, `--poll-inflight`; per-driver default:
sim/xair 4, dl32s 1; clamped 1..=16). Strict lanes per tick: writes (the
tick is skipped while any command is in flight, counting `poll_skips`) >
`focus` backlog > visible-group lane (the most-recently focused strips,
capped at 16 — one full sweep of their EQ/dyn/sends ids per focus, then
the lane disarms so the background can never starve) > essentials lane
(every strip's fader/mute/name/pan/color + the xair `solo` backstop,
~150 pairs on xair — perpetual, so a reconnected app is desk-fresh in
seconds) > background sweep, with a fairness reservation (the background
keeps one slot every `max(2, n)` ticks) so the perpetual essentials lane
can never starve it. The
effective inflight adapts down (halves) while the batch-RTT EMA exceeds
the poll tick, so a saturated desk is not piled on further.

Measured sim first-view (12 visible strips × ≥40 ids each, cold cache,
WS end-to-end on a quiet box, production 50 ms tick):

| inflight | first-view |
|----------|------------|
| 1 | ~84 s |
| 4 (sim/xair default) | ~21 s |
| 8 (`--poll-inflight 8`) | ~11 s |
| 4 at `--poll-ms 5` (test tick) | ~2.1 s |

Pipelining scales ~linearly while reads are in-process (4.0×/7.9× for
4/8); on hardware the ceiling is the desk RTT, not the tick — hence the
adaptive backoff. The xair default of 4 was rig-measured 2026-10-19
(XR18 @ 192.168.40.123, routed LAN: 48 sequential queries in 29 ms,
~1600 q/s burst — 80 q/s average runs with ~20x headroom).
`poll_tick` is public so tests and diagnostics can drive (and time) the
poller deterministically; `poll_once` stays as the exact legacy
one-query-per-call path.

## Devices, link, info (R2)

`scan` re-runs discovery (sim: the design's three devices — FOH Console
`192.168.1.20` linked, Monitor Console `.21` available, Stage Box A
`.40` I/O in use; xair: `crates/transport` OSC broadcast discovery
behind the `DeviceScanner` trait — `RealScanner` in production, a fake
in tests, so no test touches the network; UNVERIFIED-ON-RIG) and
broadcasts `devices` with link-aware states. `connect_device {id}`
links a console (I/O boxes are denied: they cannot mix) and runs the
full post-switch broadcast set; `disconnect` flips `link` to
`disconnected` without tearing the session down. `link` re-emits on
every change and every `LINK_INTERVAL` (2 s) with a fresh round-trip
sample; `info` answers the desk identity (sim always; hardware errors
unicast while unconnected). All desks report 48 kHz (`DESK_SAMPLE_RATE`;
a desk reporting otherwise is a rig finding).

## Numbered scenes (R2)

Slots `001..=999` with names (`scenes` event: number, name, current,
saved_at). `scene_save {n,name}` captures the FULL param-plane cache —
every catalogue value of every strip (fader/mute/name + all scoped
params), main, the FX rack and the routing table. Capture walks the
poll list sequentially (sim: instant; xair: ~2k pairs at the poll
budget — allow minutes on hardware, or save from a warmed cache; the
poller does not speed single-command capture). `scene_recall
{n,fade_ms?}` ramps faders with the legacy fade, applies every other
param through `set_param` in safe order (input → HPF → gate → comp →
EQ → sends → pan → look; names ride along), then FX + routing + main.
Roles gate recall (`allow_recall`, like legacy); locks are bypassed and
reported (`recalled.overrode_locks`, like legacy) — direct writes keep
the existing deny semantics. The legacy show commands
(`capture_scene`/`recall_scene`/`delete_scene`, fader/mute only) keep
working unchanged on the same engine; scenes share nothing with shows
except the fade math. Persistence: the library lives in `scenes.json`
under `EngineConfig::data_dir` (`--data-dir` on the relay; created on
first save, reloaded in `Engine::new`, corrupt files read back empty);
without a data dir scenes are in-memory only. The design's
Confirm/Instant recall mode is a UI concern (no engine state).

## FX racks (R2)

`caps.fx` advertises `{slots, types[]}` from `core::fx` design metadata
(the six design types Hall/Plate/Room/Delay/Chorus/Flanger with their
param tables — labels, units, display ranges, normalized defaults,
`lin`/`log` scale hints; ranges are design display strings, never desk
documents). sim serves 8 slots seeded like the design's `buildFx`
(Hall/Plate/Delay/Chorus/Room/Delay/Flanger/Hall, all on but the last);
xair serves 4 slots with only numbered `fx_param par01..par64` writes
implemented (type/on/bypass have no verified mapping and error out;
desk-native 61-type table stays in `proto-xair`, UNVERIFIED-ON-RIG);
dl32s serves none. The `fx` event carries the whole rack per mutation;
an FX poll interleave (`FX_POLL_EVERY` = 32 ticks) refreshes one sim
slot per cycle, cache-only (silent unless changed).

## Routing patch (R2)

`caps.routing` advertises `{groups, per_strip, supported}`. sim fully
supports patching (Lcl/Net/USB × 32; defaults mirror the design: Ch n
← Lcl n, Ch 33+ ← Net; reads/writes ride the `source` param plane).
xair reports `supported:false` (`insrc` is live-silent on FW 1.20 —
see `docs/xair-protocol.md`); `usbreturn` stays a plain param there.
DL32S: unsupported. `patch_set`/`patch_clear` honour roles
(`allow_strip`) and strip locks (deny with `locked_by`, like params).

## Offline editing (R2)

`offline {model}` parks the desk selection and runs the sim model keyed
to that desk's topology while recording every param/fader/mute/main/FX
(type/param/on/bypass)/patch write as `{strip,id,from,to}` (`from` =
pre-write value). `link`
reads `offline` until the next connect. A connect with pending changes
emits `offline_diff` and opens the push gate — nothing is pushed without
`offline_push {"confirm":true}` (which applies the list in record order
with per-change `param` echoes); `offline_discard` drops the list
(broadcasting an empty diff). FX/patch changes ride namespaced ids
(`fx/<param>` with strip = slot, `patch`, `main_fader` with strip
65535).

## Meter taps per driver (R2)

`caps.meter_taps` names the taps; transport carries them (JSON `taps`
frames + binary type 3, negotiated by `hello {"taps":true}`), never the
param poller. Sources: sim synthesizes all four per strip (`in`
pre-fader ignoring mute/fader, `post` post-fader silent when muted,
`gr_gate`/`gr_comp` dB); xair serves `post` from the `/meters/1` blob
cache and GR from the `/meters/6` DYN cache (`in` unavailable — no
`/meters/0` subscribe; both UNVERIFIED-ON-RIG); dl32s serves `post`
only (fake-meter cache). Taps broadcast only while at least one client
negotiated them, so old clients see no new traffic; the serve adapter
holds one latest-only slot per tap (drops/sends fold into the meters
`stats` counters — the shape is pinned) and encodes binary type 3 only
for taps-negotiated clients.

## Roles / access policy

Each client carries a `core::access::Policy` (default: admin). Musician
(allow-list of strips, no main LR, no scene recall, no renames, no file
uploads, fader cap `MUSICIAN_MAX_FADER` ≈ −10 dB; master-section writes —
bus/subgroup/mute-group masters and membership — are blocked like main LR)
denials arrive as unicast `error` events;
reads are never blocked. `CheckedDriver` enforces the same rules
driver-side; the engine checks up front so denials never touch hardware.

## Tests and fixtures

- `src/engine.rs` `mod tests`: catalogue/caps/dropout/poll-list/focus/
  full-cycle unit tests (moved from the relay, assertions identical).
- `tests/engine_api.rs`: in-process scenarios mirroring the relay WS
  tests (connect/fader/mute/param, caps/snapshot, diff, focus, pins,
  shows, `you`/presence, policy, meter ticks, raw JSON errors).
- `tests/r2_api.rs`: R2 in-process scenarios (devices/link/info,
  scene save/recall roundtrip + persistence across restart, FX,
  routing, offline diff/push/discard, taps negotiation, poller
  inflight defaults + focus/visible ordering + pipelined write
  preemption).
- `tests/golden/*.json`: one file per frame type (`state`, `meters`,
  `spectrum`, `gr`, `param`, `params`, `caps`, `shows`, `locks`,
  `clients`, `you`, `error`, `recalled`) and per command type
  (`cmd_connect`, `cmd_fader`, …). `tests/golden.rs` validates them
  against the constructors (regenerate with `GOLDEN_UPDATE=1`, then
  commit); `apps/web/src/lib/golden.test.ts` feeds every frame through
  the existing TypeScript parser.
- `tests/golden_r2/*.json` + `*.bin.hex`: one file per R2 frame
  (`devices`, `link`, `info`, `scenes`, `fx`, `routing`,
  `offline_diff`, `taps`, `hello_taps`) and per R2 command
  (`cmd_scan`, `cmd_connect_device`, …, `cmd_hello_taps`), plus the
  binary taps fixtures (`taps_in.bin.hex`, `taps_gr.bin.hex`).
  `tests/golden_r2.rs` validates them (regenerate with
  `R2GOLDEN_UPDATE=1`). They live outside `tests/golden/` so the
  TypeScript parser test (which enumerates known shapes) stays green —
  the TS parser itself ignores unknown frame types.
