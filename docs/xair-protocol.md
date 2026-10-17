# X-Air OSC protocol (XR12/16/18, MR18)

Assumed firmware: **XR18 FW 1.22** (latest published Behringer release line).
No hardware on hand to confirm — addresses below are the community-documented
X-Air map (xair-api-python, X-Air OSC wiki, X32 OSC docs). If a field disagrees
with your mixer, this file is the place to correct it; the driver only encodes
this table.

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

Zero-padded two-digit channels. Driver validates ch `1..=32`, bus `1..=6`.

| Function      | Address                  | Args (set)              | Reply (get)        |
|---------------|--------------------------|-------------------------|--------------------|
| Ch fader      | `/ch/XX/mix/fader`       | float `0.0..1.0`        | float              |
| Ch mute       | `/ch/XX/mix/on`          | int `1`=on / `0`=muted  | int                |
| Ch name       | `/ch/XX/config/name`     | string                  | string             |
| Bus fader     | `/bus/XX/mix/fader`      | float `0.0..1.0`        | float              |
| Main LR fader | `/lr/mix/fader`          | float `0.0..1.0`        | float              |
| Main LR mute  | `/lr/mix/on`             | int `1`=on / `0`=muted  | int                |
| Main LR name  | `/lr/config/name`        | string                  | string             |
| Meter sub     | `/meters/1` (send empty) | —                       | `/meters/N` blobs  |

Notes:

- Fader wire range is linear `0.0..1.0`; UI dB mapping is a frontend concern.
- `/mix/on` is inverted vs mute: `1` means channel ON (unmuted).
- X32 uses `/main/st/mix/fader` for mains; X-Air uses `/lr/*`.
  Hardware-confirmed on XR18 FW 1.20 (2026-10-17): `/main/st/mix/fader`
  gets NO reply. Driver uses `/lr/*` only. `/lr/mix/on` and
  `/lr/config/name` follow by analogy with `/ch` and are not yet
  live-queried.

## Meters

- Subscribe: send empty `/meters/1`; mixer streams `/meters/0..3` blobs ~50ms.
- Blob payload: run of **little-endian i16** samples, level = `raw / 32767`
  clamped to `0.0..1.0`. Index 0 = ch 1.
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
