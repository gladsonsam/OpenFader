---
title: Recording
group: Recording
order: 8
description: Multitrack recording on a computer.
---

OpenFader can record every channel from your audio interface to a computer, and play takes back through the mixer for a [virtual soundcheck](/docs/virtual-soundcheck).

Recording only runs on a computer: Windows, Mac or Linux. A phone or tablet can control a computer's recorder remotely, but cannot record by itself. See [Control the recorder from a phone](#control-the-recorder-from-a-phone).

| Computer | Audio driver |
| --- | --- |
| Windows | ASIO. Install your interface's 64-bit ASIO driver. There is no fallback to other Windows audio drivers. |
| Mac | Core Audio. Allow microphone access when asked. |
| Linux | ALSA |

Up to 64 channels. Takes are saved as 24-bit WAV.

## Set up your audio interface

1. Tap **Tools**, then **Recorder & takes**.
2. Tap **Audio setup** in the recorder's top bar.
3. Under **AUDIO INTERFACE**, tap **Refresh devices**, then pick your interface. Each one is listed with its input and output count.

The recorder uses the interface's current sample rate. There is no sample rate setting in OpenFader. Set the rate, buffer size and clock in your interface's own control panel. If you change any of them, tap **Refresh devices** and pick the interface again.

## Record

1. Choose your interface in **Audio setup** once. OpenFader remembers it on the recorder computer and restores it when its ID, channel counts and sample rate still match.
2. Tap **Record**. OpenFader names the next take and starts capturing all available interface inputs.
3. Tap **Stop**. The take stays open. Tap **Play** to listen back through the mixer, or **Record** to capture another take.

**New take** in the recorder actions menu is optional: use it to choose a name before recording. Tap the take name in the header to browse previous recordings. Hold or hover over an icon to see its action.

Track names default to the connected provider's reported USB patch when its audio interface matches and complete routing readback is available. The recorder reads the patch again when recording starts, so a USB send patched to another input follows that input's name. Matching channel counts alone do not establish a mapping.

Under **Audio setup → Input names and mixer mapping**, use **Auto from USB patch** to refresh automatic names or return from custom labels to automatic naming. The manual picker lists the provider's input channels, including those outside the visible mixer bank; returns and VCAs are not numbered as extra input channels. You can also type custom labels, which are remembered on the recorder computer. Names in existing recordings stay as they were when recorded. Naming does not change the mixer's physical or USB routing or enable automatic soundcheck routing. Missing or unreadable routing is reported in Audio setup.

You cannot quit OpenFader while it is recording. It asks you to stop first.

## Play back and move around

Tap the take name in the header to search or sort past recordings, then tap a completed take to open it. Tap **Play**. If a verified automatic routing profile is available, OpenFader prepares the mixer returns and starts playback. Otherwise, confirm manual output mapping and mixer routing. Once playback is prepared:

- Tap the clock to jump to a time. Type seconds or `mm:ss`.
- Tap the waveform to jump to that point.
- Hold and drag across the waveform, then choose **Loop range** from the recorder actions menu to review and enable that section. **Clear loop** in that menu turns it off.
- Pinch to zoom and swipe to pan the timeline. Tap to seek during playback; double-tap to show the whole take. The **− / +** controls also zoom around the visible section.

Use the mixer icon at the top left to work with the normal mixer EQ, dynamics and levels during playback. The return-to-live icon beside the timecode restores live input routing while keeping those mix adjustments. Playback never automatically recalls a linked mixer snapshot; snapshot recall is a separate action in the recorder menu. See [Virtual soundcheck](/docs/virtual-soundcheck).

## Export a take

1. Stop playback and tap **Return to live** if playback routing is active.
2. Open **Browse takes** and tap **Export** beside the recording. You can also tap **Export** in the open take's top bar.
3. Choose **Multitrack** for a folder containing one WAV file per recorded channel, or **Stereo mix** for one stereo WAV file.
4. Pick a destination folder and wait for the export confirmation. Long takes can take several minutes to process and save.

Stereo mix combines every recorded track at equal level, centered. If the combined signal would clip, OpenFader reduces the level of the entire mix to prevent clipping. It does not include the mixer's live fader settings, EQ, dynamics or effects. It does not assume any recorded channel is a master output.

Exports use 24-bit WAV, with RF64 for files over 4 GiB. Your audio software must support RF64 for those files, and the destination drive must support large files (FAT32 does not). Multitrack exports also include the channel names and an OpenFader import manifest.

## Import a take

Tap the take name to browse, then tap the import icon and pick a multichannel WAV, a native take folder, or a mono tracks folder exported from OpenFader. A mono tracks folder must have come from OpenFader. A folder of WAV files from another program will not import.

Export and import only work on the computer itself, not from a phone.

## Control the recorder from a phone

On the computer:

1. Open **Audio setup** in the recorder and expand **Remote server**.
2. Set a server password if you want one, then enable access for app clients.
3. Note the computer's network address and app port.

On the phone or tablet:

1. On the Connections screen, add a server using the computer's IP address or hostname and port.
2. Enter the server password if asked.

The phone can now record, stop, play and run soundchecks on the computer. To disconnect a client, use **Revoke** in the server settings.

You can also enable browser hosting in the same section. Open the displayed browser address on another device and enter the same optional password. Recording still runs on the hosting computer; the browser controls its recorder and mixer session.

## Limits

- Per-track arm, mute and solo buttons in the arrangement do not work yet.
- There are no markers.
- A take marked **Recovery needed** was interrupted, for example by the interface disconnecting. On the recorder computer, use **Recover** in **Browse takes** to keep its last saved checkpoint; audio after that checkpoint is discarded.
- The app does not show free disk space. Check it before a long recording.
- Recording and playback have not been tested with every interface and mixer.

## Messages you might see

| Message | What to do |
| --- | --- |
| No usable ASIO interfaces found... | Install the interface's 64-bit ASIO driver, connect it, close other audio programs, then tap **Refresh devices**. |
| The take and interface sample rates must match. | Set the interface to the take's sample rate in its control panel, then refresh. |
| ...layout changed; refresh and select the interface again | You changed the interface's settings. Refresh and pick it again. |
| Recorder host is unavailable. | The background recorder is not running. Restart OpenFader. |
