# XR18 hardware evidence

## 2026-10-31 read-only investigation (in progress)

The supplied rig context reports 18-in/18-out 48 kHz USB capture with 240128 nonzero frames. This demonstrates capture, not channel identities, input selectors, USB tap positions, or an available network control path. No audio output or mixer setting changes are authorized by this investigation.

Prior source observations in `crates/proto-xair/src/input.rs` conflict: on 2026-10-18 `/ch/{01,05,09,15,16}/config/insrc` and `/preamp/rtnsw` timed out (5 seconds, twice), whereas on 2026-10-27 those same leaves reportedly answered. The earlier `/rtn/aux/preamp/rtnsw` returned integer 1. Those historical observations do not establish the current state or all-channel address coverage. `/ch/01` and `/ch/16/config/rtnsrc` previously returned 0 and 17 respectively; those are historical values, not a current routing snapshot.

Existing driver assumptions to verify: channels 01–16 each expose `config/insrc`, `config/rtnsrc`, `preamp/rtnsw`; USB sends 01–18 use `/routing/usb/NN/src` and `/pos`. The driver models a stereo aux input separately; no current evidence establishes its full selector addresses or values. Driver tap labels identify 0 as Analog input and 4 as Pre EQ, but physical signal position and USB capture correspondence have not been independently proven here. Do not use these assumptions as a verified soundcheck route.

Historical rig notes identify XR18 firmware 1.20 and `/info` response `V0.04, Studio, XR18, 1.20`; firmware must be read again to establish current identity. Network search and readback pending.

## Current unicast readback, 2026-10-31

Unicast UDP port 10024 answered. `/info` returned `,ssss` `[V0.04, Example Mixer, XR18, 1.20]`; `/xinfo` returned `[<mixer IP>, Example Mixer, XR18, 1.20]`. Two sequential passes of no-argument OSC queries matched. Per-address timeout was 250 ms. This is a sequential sample, not atomic; settings may change afterward. All values below are raw `,i` integers.

| Channel | `/ch/NN/config/insrc` | `/ch/NN/config/rtnsrc` | `/ch/NN/preamp/rtnsw` |
|---:|---:|---:|---:|
| 01 | 0 | 0 | 0 |
| 02 | 1 | 1 | 0 |
| 03 | 2 | 2 | 0 |
| 04 | 3 | 3 | 0 |
| 05 | 4 | 4 | 0 |
| 06 | 5 | 5 | 0 |
| 07 | 6 | 6 | 0 |
| 08 | 7 | 7 | 0 |
| 09 | 8 | 8 | 0 |
| 10 | 9 | 9 | 0 |
| 11 | 10 | 10 | 0 |
| 12 | 11 | 11 | 0 |
| 13 | 12 | 12 | 0 |
| 14 | 13 | 13 | 0 |
| 15 | 14 | 16 | 1 |
| 16 | 15 | 17 | 1 |

Stereo aux root: `/rtn/aux/config/insrc` timed out in both passes; `/rtn/aux/config/rtnsrc` = 8; `/rtn/aux/preamp/rtnsw` = 1. This is one stereo aux strip, not proof of separate channel 17/18 selectors. The driver interprets rtnsrc as a zero-based USB source and rtnsw 1 as USB; the stereo pair semantics need independent confirmation.

| USB send | `/routing/usb/NN/src` | `/routing/usb/NN/pos` |
|---:|---:|---:|
| 01 | 0 | 0 |
| 02 | 1 | 0 |
| 03 | 2 | 0 |
| 04 | 3 | 0 |
| 05 | 4 | 0 |
| 06 | 5 | 0 |
| 07 | 6 | 0 |
| 08 | 7 | 0 |
| 09 | 8 | 0 |
| 10 | 9 | 0 |
| 11 | 10 | 0 |
| 12 | 11 | 0 |
| 13 | 12 | 0 |
| 14 | 13 | 0 |
| 15 | 14 | 0 |
| 16 | 15 | 0 |
| 17 | 16 | 0 |
| 18 | 17 | 0 |

The current driver decodes source values 0–15 as Ch 1–16 and 16–17 as Aux 1–2; it labels tap 0 “Analog input”. The responses establish raw settings and address coverage, not an independent signal-path proof of these labels. In particular, input channels 15–16 have return bank 1 and return selectors 16–17 while all USB taps read 0; do not assume capture is post-processing or that a playback route is safe from feedback based on capture alone. No OSC writes or audio output were sent.

A further concurrent 5-second read-only query returned `/rtn/aux/config ,sii ["COMP LR", 1, 8]` and `/rtn/aux/preamp ,fi [0.0, 1]`. No replies arrived for `/rtn/aux/config/insrc` or for `/ch/17` and `/ch/18` each with `config/insrc`, `config/rtnsrc`, and `preamp/rtnsw`. Timeout is not proof those paths cannot ever respond. Stereo aux analog selector and exact USB pair mapping remain unverified. The historical silence of channel 01–16 selector leaves is contradicted by both current passes, but its cause is unknown.

**Outstanding before soundcheck:** independently establish the stereo aux source/USB pair semantics and verify that the selected USB send tap really precedes processing in the relevant signal path; determine a feedback-safe playback routing with fresh readback. Neither the two-pass snapshot nor USB capture alone proves these. The raw input and USB output snapshot above is complete for the queried addresses, except for the explicitly timed-out aux analog selector; it does not include unrelated physical output, bus, or FX routing.

## Manufacturer documentation

The manufacturer [XR18 quick start guide](https://mediadl.musictribe.com/media/PLM/data/docs/X-AIR/QSG_BE_0605-AAA_X18_WW.pdf) describes 18 channels of USB recording and playback, physical line inputs 17/18, and a stereo USB/Aux input in its system diagram. It does not establish the OSC selector encoding or the actual capture tap for this configuration. Therefore it does not resolve the outstanding soundcheck proof above.

The manufacturer also publishes its [X AIR remote control protocol](https://mediadl.musictribe.com/download/software/behringer/XAIR/X%20AIR%20Remote%20Control%20Protocol.pdf), documented for firmware 1.11. This is useful protocol evidence but does not independently establish the current rig's signal path or prove a feedback-safe playback route. Hardware routing remains gated.

## Playback route research, 2026-10-31

A fresh no-argument query confirmed `/xinfo` = `[<mixer IP>, Example Mixer, XR18, 1.20]`, `/rtn/aux/config/rtnsrc` = 8, `/rtn/aux/preamp/rtnsw` = 1, and USB sends 01, 17, 18 have `(src,pos)` = `(0,0)`, `(16,0)`, `(17,0)`. This is readback, not a transaction or proof of restoration after a change; no settings were written.

A read-only inspection found the ordered pair names `U0102` through `U1718` and a bundled routing preset explicitly stating “connects USB Return 1/2 with Aux Channel 17/18, channel switch is USB On (A/D off)” followed by `/rtn/aux/preamp/rtnsw ON` and `/rtn/aux/config/rtnsrc U0102`. Thus the aux return is selectable rather than fixed to 17/18; the observed value 8 corresponds to U1718 in the published community OSC table (https://schwiftything.com/blogs/code-tips-tricks/x-air-m-air-osc-commands). The bundled preset also describes USB sends 1–18 as Ch 1–16 and Aux In 17/18 with tap A IN. The executable contains `Analog` and `Analog + Mute` labels, but these strings alone do not prove the raw enum's physical signal path.

**Candidate minimal playback changes, conditional on fresh snapshot and physical verification:** for channels 01–14 change `/ch/NN/preamp/rtnsw` from 0 to 1; change `/ch/15/config/rtnsrc` from 16 to 14 and `/ch/16/config/rtnsrc` from 17 to 15. Channels 01–14 already select USB 01–14; channels 15–16 already select USB bank; aux already selects USB bank and pair 17/18 (`rtnsrc=8`, `rtnsw=1`). All other fields remain untouched. Verify each changed field by readback; on restore return those fourteen banks to 0 and the two USB source indices to 16 and 17, respectively, then read each back against a fresh pre-change journal, bound to the same desk identity. Do not require `/rtn/aux/config/insrc` to snapshot or restore since this plan never writes it. Concurrent external edits and non-atomic OSC prevent treating the historical table as an unconditional restore journal.

This is a candidate, not a verified physical profile: raw `/routing/usb/NN/pos=0` must still be proved to tap the physical analog signal before the USB/analog source switch and channel DSP, including aux L/R, and thus avoid recapturing playback. The official remote protocol link above was inaccessible during this investigation; the published manufacturer quick start does not settle the raw enum. Before enabling playback, perform an explicitly authorized isolated signal test: individually inject identifiable signals into physical inputs 1–16 and line 17/18, verify their corresponding USB capture channels with the proposed USB banks selected, and compare capture with playback stopped versus a distinct USB playback signal. Confirm each physical signal remains on its intended capture channel and no playback signal appears in capture; use the actual intended gain and DSP configuration and do not rely on silence alone. Independently verify the selected aux pair reaches the stereo aux strip and channel 15/16 return assignments reach their intended strips. This investigation sent no audio or configuration writes.

## Duplex probe preparation, 2026-10-31

A standalone CoreAudio diagnostic is available at `crates/audio/examples/xr18_duplex_probe.rs`. It captures 18 channels and emits a 997 Hz sine at -96 dBFS peak on exactly one chosen USB output, with all other output samples zero. It reports per-channel RMS and quadrature tone amplitude for baseline and marker windows. Stream start times are asynchronous, so window alignment must be checked before interpreting measurements; a missing marker is not proof of isolation. The tool changes no mixer selectors and does not itself create or restore a selector journal. It must only be used after confirming the host recorder is stopped, persisting a fresh identity-bound selector/mute snapshot, and arranging verified restoration. Audio streams are dropped before results are printed. Startup is not watchdog-bounded.

`cargo check -p openfader-audio --example xr18_duplex_probe --locked` passed, and `--self-test` recovered the synthetic -96 dBFS peak as 0.000015849. This is a calculation check, not a hardware test. The desktop host process was running, but the expected `~/Library/Application Support/OpenFader/Recorder/host.json` descriptor was absent, so its recorder state could not be queried. No duplex stream, marker, or selector write was attempted; no new hardware isolation or channel mapping is proved. Restore/readback remains required if a later test changes a selector. The production profile remains gated. A distinct, sustained acoustic stimulus on channel 5 may be needed to distinguish the physical input from its observed very quiet noise floor.

## Quiet channel 5 probe, 2026-10-31

The supplied `/tmp/xr18-quiet-proof.json` records a 199168-frame probe on the
XR18 Example Mixer firmware 1.20. With channel 5 temporarily muted and its USB
bank selected, the input-strip meter measured -60.0 dB while the raw capture
channel 5 marker amplitude was approximately 7e-8 against a 0.001 marker
(about -83 dB relative). This supports pre-bank separation for that tested
path; it does not prove all 18 physical sockets or identify microphones. The
probe was stopped and channel 5 bank 0 and mute 1 were restored, with two final
snapshots reported matching the original. The earlier candidate plan above
would replace the live USB sources on channels 15/16 with unrelated analog
captures; those sources must instead be preserved or explicitly excluded.
The aux strip's current USB 17/18 source is likewise not the physical aux
capture. Production remains gated until fresh session-pinned identity,
firmware and send readbacks, take matching, and safe routing are integrated.
