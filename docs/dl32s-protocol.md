# DL32S protocol (ported from verified Python implementation)

Source of truth for the wire: the proven Home Assistant integration
(`mackie_dl/client.py`, `addressmap.py`, `SPEC.md` — read-only reference,
own-LAN black-box capture, no decompile). This crate is a behavior-faithful
Rust port: same frame layout, same init sequence, same push semantics.

## Transport

One client-initiated TCP connection, default port **50001**. All values
big-endian.

```
0xAB | seq | chunk count (u16) | type | command | header checksum (u16)
     | body (4-byte chunks) | body checksum (u32)
```

- Header checksum is `0xFFFF` minus the sum of the header bytes; body
  checksum is `0xFFFFFFFF` minus the sum of the body bytes. With zero chunks
  both body and body checksum are absent.
- Message types: Request (0), Response (1), Error (5), Broadcast (8).
- The body checksum is computed but not enforced on receipt, matching the
  reference implementation.

Commands used: `KEEP_ALIVE (0x01)`, `CLIENT_HANDSHAKE (0x03)`,
`FIRMWARE_INFO (0x04)`, `CHANNEL_INFO_CONTROL (0x06)`,
`SHOW_SNAPSHOT (0x07)`, `GENERAL_INFO (0x0E)`, `CHANNEL_VALUES (0x13)`,
`BROADCAST_CONTROL (0x15)`, `METER_LAYOUT (0x16)`, `CHANNEL_NAMES (0x18)`.
File-transfer commands (`0x09`–`0x0D`) are defined in the codec but the
upload/download flows are not ported (see below).

## Init sequence (best-effort per step)

Keepalive → `CHANNEL_INFO_CONTROL` zeros (large-message permit) →
`CLIENT_HANDSHAKE` → `GENERAL_INFO` type 2 → `METER_LAYOUT` (full 32-input
DL-family list) + `BROADCAST_CONTROL` → `CHANNEL_INFO_CONTROL` type 6
(channel-data request). After that the mixer pushes its whole value space
unprompted (3072 values from address 1, then 2185 from 3073 on a DL32S).

## The two behaviours that catch you out

1. **The mixer pushes, it does not serve.** An explicit `CHANNEL_VALUES`
   read returns an empty body. The driver keeps push-driven caches
   (`cached`, `cached_type2`, `name`); reads before the first push fail with
   "no value pushed yet". Our own writes are cached locally because the
   mixer never echoes them back.
2. **Bulk pushes carry `count = 0`.** Those set the meta word to
   `0x00000500`: type 5, count zero, real length implied by the frame size.
   The parser takes `min(count, available)` when count is set and `available`
   when it is zero. Small messages do set count, so the field is honoured —
   it just cannot be trusted blindly.

## Value messages

```
Chunk 0: start address
Chunk 1: count (u16) | type (u8) | unknown (u8)   type 5 = params, 1 = meters, 2 = mute groups
Chunk 2+: one value per chunk
```

Levels are BE IEEE-754 floats in dB; switches/enums are plain u32. `type`
selects the address space (address 11 is input 1 `main_assign` under type 5
and mute-group 1 master under type 2), so the two caches stay separate.
Type-1 meter blocks are parsed and dropped: the meter value encoding is
unconfirmed, so `subscribe_meters` stays unsupported until a capture pins
it down. Type-2 holds ~107 addresses; mute group N master = address 10+N.

## Text is word-swapped

Every string on the wire is 4-byte words with bytes reversed inside each
word (`MGZU` travels as `55 5A 47 4D`). `word_swap` undoes it for channel
names, snapshot names and file payloads.

Names arrive as a flat 75-slot table (1-32 inputs, 50-57 aux, 58-63
subgroups, 64-69 mute groups, 70-75 VCAs). An unnamed slot is a bare NUL
and still consumes an index. The meta count bounds single-slot rename
pushes; a zero count takes the whole bulk dump. One deliberate deviation
from the Python reference: it ignores the count and inserts padding NULs
as extra slots, so a rename also blanks the neighbour slot in its cache.
This port honours a nonzero count (first-N-chunks), which is strictly
better under every observed shape; the assumption is flagged here because
no rename capture exists to confirm the count semantics.

## Address space (DL32S verified, firmware `0106000100000002`)

Inputs are `base 1, stride 106, count 32`. Offsets 0-34 are the channel
head (mute 7, fader 8, gain 12, phantom 13, HPF, gate, comp, EQ mode/on);
35-49 EQ bands 4/3/2 (gain/freq/Q/type) then band 1 (gain/freq/Q, no type
word); 50-73 eight aux sends (level/mute/unknown triples); 74-81 four FX
sends (level/mute pairs); 82-105 mute-group/view/subgroup/VCA membership.
44 of 106 fields are `verified: false` — position certain, meaning inferred.

Channel 2 mute must be address **114**; channel 1 fader is 9,
`aux1_level` is 51. The table test asserts all three plus full 0-106
coverage with no duplicates, so a shifted layout fails loudly.

## Verified vs unconfirmed

| Area | Status |
|---|---|
| Frame framing, checksums, word swap | Verified (byte vectors in tests) |
| Init sequence, push-not-poll, count==0 rule | Verified (loopback fake-mixer test) |
| DL32S input strip (mute/fader/gain/phantom/HPF/gate/comp/EQ/sends) | Verified against hardware dump |
| Mute-group masters (type-2, 11-16) | Verified |
| Channel names, snapshot op-1/5 relays | Ported; covered by loopback test |
| Snapshot recall/save bodies (Master-Fader app sequence) | Observed on wire, untested on HW from here |
| DL16S EQ packing (four uniform 4-word bands?) | **Unconfirmed — driver rejects `dl16s`** |
| DL32R (base 41, stride 132, beyond mute/fader) | **Unconfirmed — driver rejects `dl32r`** |
| Meter value encoding (type-1) | **Unconfirmed — ignored, meters unsupported** |
| `set_name` rename write encoding | **Unconfirmed — left unsupported** |
| Show.zip download / file upload (needs `zip` dep) | **Not ported** |
| GENERAL_INFO model/firmware string decode | **Not ported** (`query_info` reports static model) |

## Port notes (where things live)

- `openmix-core`: `FaderScale` — generic piecewise-linear level↔units
  interpolation. The DL32S dB table (`DB_POINTS`, DigiMixer DbFaderScale
  shape) is driver data; the math is shared for future dB-fader providers.
- `openmix-transport::mackie`: frame codec, value-block codec, word swap,
  meter-layout/broadcast constants, name/snapshot parsers, and
  `MackieSession` (TCP connect, init, rx loop with seq-correlated
  request/response, keepalive every 2.5 s, push caches). Reuses the shared
  `session::resolve_peer` for `host[:port]` parsing. Does NOT own the
  strip table — session methods take explicit addresses.
- `openmix-proto-dl32s`: `addrs` (input-strip table + scale table + model
  gate) and `Dl32sDriver` (thin `MixerDriver` mapping: 0-based `StripId` →
  1-based channel, level↔dB, param encode/decode with limit checks,
  mute groups, show recall/save, cache-built snapshots).
- Out of scope on purpose: output-strip fields (structurally mapped but
  deliberately unlabelled per SPEC — bus/main faders stay unsupported),
  file transfer, meter streaming.

Run without hardware: `cargo test -p openmix-proto-dl32s` (table + mapping)
and `cargo test -p openmix-transport mackie` (codec vectors + loopback
fake-mixer session test). After a firmware update, re-verify against a
fresh dump (`listen_mackie.py` + `verify_map.py` upstream) before trusting
any field.
