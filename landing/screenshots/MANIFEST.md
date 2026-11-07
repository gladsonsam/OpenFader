# OpenFader docs shots - manifest

All shots are real app pixels, cropped to the app client window (no WM
decorations, no Xvfb margins). Tablet shots are 1366x1024, phone shots are
430x932. The app switches to its phone layout below 700 px width
(AppDesign.phoneBreakpoint); the phone layout was verified at 430 px.
Release build (no debug overflow stripes). Crop uses the true client
position from `xwininfo` ("Absolute upper-left X/Y"), not
`xdotool getwindowgeometry` (which reports the client 22 px too low under
openbox and used to cut off the header bar).

Simulated mixer (same across all shots): offline "Demo · simulated" desk -
the Rust `openfader-serve` sim backend, SimDriver 32+8 (generic preset),
driver `sim`, `sim://local`. No hardware, no network mixer.

## Captured

- start-connections.png - 1366x1024. No clicks: app launch, staged boot,
  Connections start screen. Visible: OpenFader brand, Recent mixers box
  ("No mixers found…", Discover mixers / Add manually), Work offline link.
- connect-add.png - 1366x1024. Click "Add manually". Visible: "Add mixer
  manually" dialog (mixer-type picker, IP/hostname, name, Add mixer button).
- connect-add-2.png - 1366x1024. Same dialog, then opened the "Mixer type"
  dropdown. Visible: Automatic / Behringer X-Air / Behringer X32 /
  Mackie DL32S / Behringer WING options. (A connect-add-1.png in this
  directory belongs to a parallel session, not this manifest;
  connect-add.png here is the dialog = variant 1.)
- mixer.png - 1366x1024. Click "Work offline" (needed two clicks; first only
  hovers). Visible: main mixer, All bank CH01–CH10 + Main, MAIN mix target,
  bank rail (MUTE 1–6, MIX, OUTPUTS MAIN/BUS1–8/SUBGROUP…), Demo badge.
- mixer-phone.png - 430x932. Same path at phone size. Visible: compact
  horizontal strips CH01–CH03 + Main, compact toolbar (Demo, FX, Tools).
- mixer-banks.png - 1366x1024. Clicks: Tools → "Mixer layout" tile →
  scrolled bank menu → "FxRet 1–4" → "Mixer" tile; mix target back to MAIN.
  Visible: FXR1–FXR4 bank with MAIN mix and full bank rail.
- mixer-sends.png - 1366x1024. Click "BUS1" in the OUTPUTS rail. Visible:
  sends-on-faders for BUS1 (readout "CH01 Send → BUS1", BUS1 highlighted).
- layers.png - 1366x1024. Clicks: Tools → "Mixer layout" → bank menu →
  keyboard to "Layers" (Up to top, then Down past VCA 1-6/Returns, Return).
  Visible: "Layers" bottom sheet
  (All / Follows bank paging, New from template: Vocals/Drums/Band/Stream,
  Share layers).
- channel-input.png - 1366x1024. Clicks: mixer → tap CH01 strip EQ preview.
  Visible: CH01 Input tab (Source Lcl 01, Gain/Trim/48V/Delay/Insert,
  Main/Sub/VCA/Mute assigns, BUS1–6 sends).
- channel-gate.png - 1366x1024. Left rail "Gate" tab. Visible: gate curve
  (T handle), IN/GR/OUT meters, Threshold/Range/Attack/Hold/Release.
- channel-compressor.png - 1366x1024. Left rail "Comp" tab. Visible:
  compressor curve (G/T/R handles), Soft/Hard Knee, Threshold/Ratio/
  Attack/Gain/Release.
- channel-eq.png - 1366x1024. Left rail "EQ" tab. Visible: 4-band EQ with
  RTA spectrum, HPF + bands 1–4, per-band Shelf/Bell and G/F/Q sliders.
  Note: the 6.0 px bottom-overflow warning stripes seen on Bands 1 and 4
  in the debug build do NOT render in the release build - this shot is
  clean, which is why the shots were retaken in release.
- channel-phone.png - 430x932. Phone mixer → tap CH01 preview. Visible:
  CH01 Input in mobile layout (bottom Input/Gate/Comp/EQ tab bar,
  right-hand fader, Assignments/Automix/Sends sections).
- fx.png - 1366x1024. Toolbar "FX". Visible: FX1–FX4 slots (Plate Reverb,
  Tape Echo Delay, Hall Reverb, Spring Reverb) and the FX1 Plate Reverb
  editor with 14 knobs; "FX preset library is unavailable for this mixer."
- routing.png - 1366x1024. Clicks: Tools → "Routing" tile. Visible: Input
  patch matrix, destinations CH01–CH14 patched 1:1 to Local 1–14.
- scenes.png - 1366x1024. Clicks: Tools → "Scenes" tile. Visible: provider
  show/snapshot library card plus OpenFader local library (Shows and
  Snapshots, both empty, with Capture/Save actions).
- meters.png - 1366x1024. Clicks: Tools → "Meters" tile. Visible: 19 live
  simulated channel meters CH01–CH19 with peak holds.
- recorder.png - 1366x1024. Clicks: Tools → "Recorder & takes" tile.
  Visible: transport (00:00:00.00, Live·Stopped, Loop/Set loop/Clear loop),
  Takes panel (Import take), Arrangement pane ("0 tracks - Configure an
  audio interface to see capture tracks"), 00:00–00:30 timeline.
- settings.png - 1366x1024. Click: mixer toolbar gear. Visible: Settings
  with sidebar (Connections active) - connection behaviour toggles,
  Start-on picker, no saved mixers, Supported mixers XR18/DL32S.
- settings-access.png - 1366x1024. Clicks: settings sidebar "Access limits".
  Visible: "Limits unlocked" + four-character-passcode note, Restrict
  controls list (all off), Restrict all / Clear all. The "Lock" button
  produced no dialog in this run.

## Not captured

- routing-confirm: no confirmation step appeared in offline mode. Tapping
  matrix cells only stages a "+" preview; the single-tap "Confirm route"
  sheet is a touch-gesture path (mouse taps take the direct-patch path in
  `screens/routing.dart`), and the long-press-drag "Review N changes /
  Apply N changes" sheet never opened. A cell long-press menu with
  "Unpatch" was observed but dismissed without a follow-up dialog.
- recorder-arrangement: not a separate view in this build - the
  arrangement/timeline is the recorder page's main pane (see recorder.png,
  empty with 0 tracks and no audio interface configured).
- soundcheck: no virtual-soundcheck panel without a recorded take and an
  audio interface ("Configure an audio interface to see capture tracks";
  state Live·Stopped). Entering soundcheck requires `enter_soundcheck`
  with a take id, which does not exist here.

## How to re-run

Prerequisites (verified on this Linux host): Xvfb, openbox, xdotool,
xwininfo, ffmpeg, Python 3 + PIL. Flutter SDK at ~/sdks/flutter/bin. Build
RELEASE so no debug overflow stripes render (channel-eq is only clean in
release); the bundle needs the serve + capture-helper binaries next to it:

    cd /home/gladson/openfader
    export CARGO_TARGET_DIR=/home/gladson/.cache/cargo-target   # repo has no target/
    cargo build --release -p openfader-flutter-bridge -p openfader-serve -p openfader-audio
    export PATH=~/sdks/flutter/bin:$PATH
    cd app && flutter build linux --release
    cp $CARGO_TARGET_DIR/release/libopenfader_flutter_bridge.so build/linux/x64/release/bundle/lib/
      # bundle: app/build/linux/x64/release/bundle/openfader
      # (+ openfader-serve, openfader-capture-helper staged by the build;
      #  verify the bundled openfader-serve md5-matches the release binary)

Display + window manager (1440x1200 via cap.sh; it grabs a 1440x1200
screen, so start Xvfb at exactly that size):

    Xvfb :93 -screen 0 1440x1200x24 +extension GLX +render -noreset &
    DISPLAY=:93 openbox --sm-disable &

Launch (software GL; without these the frame grabs stay black and the log
says "Timed out waiting for OpenGL frame"):

    cd app/build/linux/x64/release/bundle
    DISPLAY=:93 XDG_DATA_HOME=/tmp/ofXX LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11 \
      dbus-run-session -- ./openfader &

Size/position before EVERY shot (else the client may sit partly off-screen
at phone size), then capture with landing/screenshots/cap.sh (usage:
`cap.sh out.png :DISPLAY` - it reads xwininfo "Absolute upper-left X/Y"
and crops the true client area):

    WID=$(DISPLAY=:93 xdotool search --class openfader | tail -1)
    DISPLAY=:93 xdotool windowsize $WID 1366 1024   # or 430 932 for phone
    DISPLAY=:93 xdotool windowmove $WID 0 0
    landing/screenshots/cap.sh out.png :93

For clicks, compute root coords = client coords + the xwininfo absolute
origin (never getwindowgeometry - 22 px too low under openbox): e.g.
`read OX OY < <(DISPLAY=:93 xwininfo -id $WID | awk ...)` then
`xdotool mousemove $((OX+CX)) $((OY+CY)) click 1`. Park the pointer at a
neutral spot (e.g. header centre) before capturing so no hover tooltip is
in the shot.

Caveats found while capturing, kept here so a re-run does not rediscover them:
- If mouse taps on the Connections start screen are ignored (verified XTEST
  input reaches other windows via xev, and keyboard Tab/Space works), open
  "Add manually" from the keyboard instead: Tab x4 from launch (?, gear,
  Discover, Add manually - watch the focus ring) then Space; the Mixer-type
  dropdown inside the dialog DOES respond to mouse.
- The GtkApplication uses a fixed application-id, so a second copy on
  another display exits(0) silently (D-Bus single-instance). Give it a
  private bus (`dbus-run-session`) and a private descriptor dir
  (`XDG_DATA_HOME=…`) so it spawns its own `openfader-serve`.
- `DesktopHost` attaches to any already-running serve via the host.json
  descriptor; a stale descriptor (dead serve) fails boot - delete it (or
  use a fresh XDG_DATA_HOME per run).
- Taps during/just after a route transition are absorbed; wait ~2 s after
  each navigation before the next tap. On first launch wait ~60 s for
  staged boot + sim connect before driving the start screen. The layout
  bank menu is long and scrollable; mouse clicks near "Layers" kept landing
  on "Group members", so keyboard was used (Up to top, then Down to
  "Layers", Return). "FxRet 1–4" is the 6th bank-menu item below "All".
- The routing "Confirm route" sheet only fires for touch input; xdotool
  mouse taps take the direct-patch path.
- Never `pkill -f <pattern>` with the pattern in your own command line
  (kills your shell); kill app/serve/Xvfb/openbox by explicit PID
  (`pgrep -x openfader`, `pgrep -x Xvfb`, etc.).

## Crops applied after capture

These published shots were cropped from the full 1366x1024 capture to the
part that matters. Re-apply the same boxes (PIL `crop(left, top, right, bottom)`)
after a re-shoot:

- start-connections: (263, 200, 1103, 1024)
- connect-add, connect-add-2: the dialog panel only, (443, 296, 923, 728)
- scenes: (0, 0, 1366, 480)
- meters: (0, 0, 1366, 440)

## Native iPad refresh — 2026-11-07 (current website handoff)

Supersedes the old Linux feature crops above for these five features only:
Mixing, Channel processing, Effects, Routing, Scenes. No recorder, virtual
soundcheck or monitor-mix marketing panel is included. Website source is untouched.

Captured from the actual Flutter UI on iPad Pro 13-inch (M5), iOS 27 simulator,
in landscape. Offline embedded Rust simulation; no hardware verification.
The redundant offline badge is removed in the app, not painted out of images.

- Raw directory: `app/assets/screenshots/ipad-2026-11-07/raw/`
- Raw names: `mixing-raw.png`, `channel-processing-raw.png`, `effects-raw.png`,
  `routing-raw.png`, `scenes-raw.png`.
- All raw captures: **2752 × 2064, 4:3**, native 2× density (1376 × 1032 points).
  Zero capture padding, zero cropping. OS status/home safe areas are retained.
  Scenes is the full screen, not the old 1366 × 480 crop.
- Website frames: `landing/public/screenshots/ipad-2026-11-07/`, with the same
  five names replacing `-raw` with `-framed`. All **2896 × 2208 (181:138)** RGBA.
  Identical illustrative iPad bezel: 48 px, external transparent margin 24 px;
  UI rectangle `(72,72,2752,2064)` in x/y/width/height notation. UI pixels are
  copied 1:1, without stretching, rounding away content, or letterboxing.
- Active GIF: `app/assets/screenshots/ipad-2026-11-07/mixing-active.gif` and
  `landing/public/screenshots/ipad-2026-11-07/mixing-active.gif`, **1376 × 1032,
  4:3**, 96 frames at 12 fps, 8 seconds. Uniform 50% scale from device pixels.
  Both existing `app/assets/screenshots/mixer-ipad.gif` and
  `landing/public/screenshots/mixer-ipad.gif` aliases are refreshed.
- Original screen recording: `app/assets/screenshots/ipad-2026-11-07/mixing-active.mp4`.
  Native recording has portrait storage plus rotation metadata; ffmpeg applies
  that metadata before the uniform GIF reduction.

### Repeat captures

Use the device tools: `device_list`, then `device_open` for the intended iPad.
Never reuse another thread's session. Install a freshly built simulator app:

```sh
export PATH="$HOME/flutter-3.47/bin:$PATH"
cd app
flutter build ios --simulator --debug --dart-define=OPENFADER_MARKETING_FIXTURE=true
```

Install via the bound agent-device command returned by device_open, then from
repo root run (Python environment needs Pillow; ffmpeg must be on PATH):

```sh
python landing/screenshots/capture-ipad.py /absolute/device_open/agent-device
```

The script launches the app, explicitly chooses Work offline, stages the
opt-in named simulation fixture, records/captures real UI, frames all five
screens, and exports the GIF/aliases. The define defaults to false in ordinary
builds. Stop on any command failure; inspect the Device panel before retrying.
Routing's large AX tree triggered the runner watchdog in this capture; its
screenshot and subsequent Tools navigation succeeded after recovery. The script
avoids AX snapshots on the routing screen and uses the documented native iPad
point geometry for its Tools button. Recheck that coordinate if geometry changes.
This run completed Routing/Scenes manually after that watchdog; the revised
whole script has not yet been rerun uninterrupted.

Limitations: the EQ view's lower controls require scrolling at this native size;
the capture preserves the actual viewport. Scene examples are local simulated
captures, not native console archives. The simple common bezel is illustrative,
not an Apple product render. Existing hashed media remain for dev1 to migrate
references without disturbing uncommitted website work.
