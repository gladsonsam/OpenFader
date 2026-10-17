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
