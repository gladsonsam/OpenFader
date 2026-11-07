---
title: Virtual soundcheck
group: Recording
order: 9
description: Mixing from a recording instead of the band.
---

Virtual soundcheck plays a recorded take back through the mixer's channels, so you can work on the mix without the band. It runs from the [recorder](/docs/recording) on a computer.

You need:

- a complete take, recorded in OpenFader or imported
- an audio interface whose sample rate matches the take
- the mixer's channels fed from the interface's playback outputs, either set up by OpenFader (automatic) or by you (manual)

## Automatic or manual

When you start a soundcheck, OpenFader offers **Automatic mixer routing** if it can safely re-patch your mixer for you. Otherwise it goes straight to manual.

| | Automatic | Manual |
| --- | --- | --- |
| Who re-patches the mixer | OpenFader, and it puts everything back afterwards | You |
| Mixers | Behringer XR18 on firmware 1.20 only | Any mixer, or none |
| Needs a network connection to the mixer | Yes | No |

Automatic routing is limited to setups that have been checked on real hardware. The X32 and Mackie DL32S do not have it yet. When automatic is not available, the manual screen shows the reason under **Automatic routing unavailable**.

## Run an automatic soundcheck

1. In the recorder, select a take and tap **Soundcheck**.
2. Pick a playback interface if asked.
3. Tap **Automatic mixer routing**, then **Play**.

OpenFader saves your live input patching, switches the channels to the USB returns, and starts playback. Your faders, mutes and processing work as normal while it plays.

To finish, tap the input button in the transport bar, then **Return to live**. OpenFader writes your original patching back. On other screens, a recorder banner at the top also has a **Return to live** button.

Pressing **Stop** only stops playback. The mixer stays patched to the USB returns until you return to live.

## Run a manual soundcheck

1. On the mixer, patch the channels you want to the interface's playback returns, and bring output levels down to a safe level.
2. In the recorder, select a take and tap **Soundcheck**. Choose **Manual routing** if asked.
3. In **Prepare manual playback**, choose an output for each track of the take. Each output can only be used once.
4. Tap **Routing is ready · play**.

OpenFader never changes the mixer in manual mode.

To finish, tap the input button in the transport bar. OpenFader stops playback and asks you to put the mixer back to its live inputs. Do that on the mixer, then tap **Live routing restored**. Until you do, OpenFader will not start a new recording or soundcheck, and will not close.

## If something goes wrong

If the mixer disconnects, or someone changes the patching at the desk during an automatic soundcheck, OpenFader does not guess. It shows **Routing recovery required**:

- **Rebind recovery:** confirm it is the same mixer, then tap **Return to live** to put the original patching back.
- **Accept current routing and archive:** use this if you already put the mixer back by hand.

Recording stays blocked until one of these is done.

## Quitting during a soundcheck

OpenFader asks before quitting. For an automatic soundcheck, **Stop and quit** stops playback and returns the mixer to live before closing. For a manual soundcheck, it stops playback and asks you to confirm you restored the live inputs.
