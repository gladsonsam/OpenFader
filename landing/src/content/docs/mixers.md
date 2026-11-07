---
title: Supported mixers
group: Start
order: 2
description: Which mixers work and what each one supports.
---

| Mixer | Status |
| --- | --- |
| Behringer X Air (XR18, XR16) | Supported. Tested on an XR18 with firmware 1.20. |
| Behringer XR12, Midas MR18 | Accepted, not tested. |
| Mackie DL32S | Supported. |
| Behringer X32, Midas M32 | In testing. Built from the published protocol and an emulator, not yet tried on a real desk. |
| Behringer WING | In testing. Built from Behringer's protocol document, not yet tried on a real desk. |
| Mackie DL16S, DL32R | Not supported. The app refuses to connect. |

If you try OpenFader with a mixer marked "in testing", try it with [Work offline](/docs/getting-started#try-it-without-a-mixer) first, then on the real desk before a service or show, not during one. Reports from real desks help: use **Report a bug** in [Settings](/docs/settings#about--support).

## What works on every supported mixer

Faders, mute, solo, pan, meters, channel names, gain, gate, compressor, EQ and sends. Each mixer uses its own fader scale, so the dB values in OpenFader match the desk.

Controls the mixer does not have are hidden or greyed out, with the reason shown. OpenFader never shows a control it cannot actually send to the mixer.

## What differs

| | X Air | DL32S | X32 / M32 | WING |
| --- | --- | --- | --- | --- |
| Effects slots | 4 | 4 | 8 | 16 |
| Effects on / bypass | No | No | No | No |
| Input routing | Yes | Yes | Yes | Yes |
| Output routing | Yes | Yes | Yes | No |
| Mixer's own scenes | 64 snapshot slots | Stored show, Show.zip files | Scenes, snippets, cues, presets | No |
| Automatic virtual soundcheck | XR18 firmware 1.20 only | No | No | No |

"Effects on / bypass: No" means OpenFader cannot switch a whole effects slot on or off yet. You can still change the effect and its settings.

## Connection details

Your device and the mixer need to be on the same network and subnet. If your network blocks these ports, OpenFader cannot reach the mixer.

| Mixer | Port |
| --- | --- |
| X Air | UDP 10024 (falls back to 10023) |
| X32 / M32 | UDP 10023 |
| WING | UDP 2223 for control, TCP 2222 for meters |
| DL32S | TCP 50001 |

## Notes per mixer

**X Air.** Some setups answer on port 10023 instead of 10024. OpenFader tries both.

**DL32S.** Loading the mixer's state after connecting can take up to 90 seconds. Discover mixers can miss it the first time, so add it by IP address. A few control ranges come from Mackie's manual rather than testing.

**X32 / M32.** The meter analyser (RTA) is not available.

**WING.** Preamp controls only appear when the channel's source is a local or AES50 input.
