# Recorder API v1

The authenticated desktop recorder host runs on macOS, Windows and Linux. Audio recording and manual playback use an OS-supported interface independent of the network mixer. A compatible driver and supported interface layout are required; Windows currently uses ASIO, macOS CoreAudio and Linux ALSA. Layouts support at most 64 channels and playback requires the take and interface sample rates to match. Physical devices and mixers have not all been tested.

All responses are JSON. `GET /api/recorder/v1/state` reports `input_mode`, `transport`, `selected_device_id`, `selected_take_id`, telemetry and `capabilities`. `manual_routing` is true while the persistent manual-routing marker exists. `input_mode` is `soundcheck` while a manual playback helper is open and `manual_recovery_required` after it closes or the host restarts. `GET /devices` asynchronously scans interfaces; `POST /devices/refresh` starts a refresh. `GET /takes` lists takes.

`POST /command` accepts `{ "request_id": string, "action": string, "args": object }`. Request IDs replay their result within the same host process. `select_device` takes `device_id`, `start_recording` takes `take_id`, `seek` takes `position_frames`, and `set_loop` takes `start_frame`, `end_frame`, and `enabled`. `play`, `pause`, and `stop` operate on the active transport. Import/export uses local staging with `take_id`, `transfer_name` and `format` (`wav`, `native`, or `tracks`); tracks folders currently require the OpenFader export manifest.

`enter_manual_soundcheck` takes `{ "take_id": string, "expected_device_id": string, "routing_confirmed": true, "output_mapping": [integer|null, ...] }`. The zero-based array contains one entry per take track; null means unassigned/silent. Assigned outputs must be unique, within the selected interface's current output layout, and at least one track must be assigned. The host verifies the expected device, rate, layout, complete take and confirmation again before playback. This mode never accesses mixer routing or recalls a snapshot. The user must route mixer inputs to interface returns before confirming. A persistent marker is written before output starts and survives failure or restart.

`stop_manual_output` closes and reaps the playback helper while retaining the marker. After restart it checks a process-held playback lock; if a prior helper remains alive, recovery stays blocked until it exits. The helper validates its session nonce under the same lock before initializing audio, so a delayed helper cannot start after recovery. Only after physically restoring live inputs may the user call `return_manual_live` with `{ "routing_restored": true }`; it requires output closed and clears the marker. Pending manual recovery blocks recording, fresh soundcheck and host shutdown. It does not require a mixer network connection. `enter_soundcheck` and `return_live` are separate automatic operations that require a verified provider routing profile and preserve their journal/recovery protections. Unverified physical routing is not enabled. Native snapshot links are optional UI preferences and recall is separately confirmed against the current provider.

## Desktop lifecycle contract

`openfader-serve --desktop-host` creates a per-user locked descriptor with a random token and loopback endpoint. On macOS it lives under `~/Library/Application Support/OpenFader/Recorder`; on Linux under `$XDG_DATA_HOME/OpenFader/Recorder` or `~/.local/share/OpenFader/Recorder`; on Windows the bootstrap uses `%LOCALAPPDATA%`. The client verifies owner-only permissions and instance identity before attaching. Recorder requests require `Authorization: Bearer <token>`. Authenticated `POST /shutdown` succeeds only when recording and routing recovery are inactive. A stale descriptor does not authorize terminating another process.

## Waveform peaks contract

Authenticated `POST /api/recorder/v1/waveform` accepts JSON
`{"take_id":"...","start_frame":0,"end_frame":48000,"buckets":512}`.
The interval is half-open `[start_frame,end_frame)`. `take_id` is a single
component. Requests are bounded to at most 1,000,000 frames and 2,048 buckets;
`1 <= buckets <= end_frame-start_frame`. Responses are
`{"take_id":"...","start_frame":0,"end_frame":48000,"buckets":512,
"channels":[[[min,max],...],...]}` with one channel array per take track (1–64). Each bucket
covers consecutive frames partitioned by integer boundaries across the requested
range. Min/max are normalized signed 24-bit sample values in `[-1,1]`, computed
from actual stored samples. A missing, incomplete, corrupt, or out-of-range take
returns an error, never synthetic peaks. The endpoint is read-only and does not
use the audio callback or send raw audio samples. Clients request only their
visible range and should discard stale responses when zooming or changing takes.
Bounded concurrency may return HTTP 503; clients can retry.

During capture, `state.live_peaks` is an array of normalized absolute peak
values from the latest batch processed by the capture writer; it is `null`
when not recording or before the first captured batch. These are lightweight transient meters, not waveform
history, and are computed off the audio callback.

### Windows build prerequisites

Use an x64 Visual Studio developer shell with the MSVC C++ toolchain, Rust
stable (`x86_64-pc-windows-msvc`), Flutter 3.47.7, and LLVM's `libclang.dll`.
Obtain the ASIO SDK from [Steinberg](https://www.steinberg.net/asiosdk), review
its license, and set `CPAL_ASIO_DIR` to the extracted SDK root and
`LIBCLANG_PATH` to the LLVM bin directory. Steinberg offers GPLv3 licensing
for ASIO ([announcement](https://ocl-steinberg-live.steinberg.net/_storage/asset/808575/storage/master/Press%20Release%20-%202025-10-15%20-%20OBS%20Partnership-%20EN.pdf));
verify the license included with the SDK you use. CPAL is pinned to 0.17.3.
Run `scripts/build-windows.ps1` from PowerShell. It requires a provisioned SDK
and does not download one. The Windows desktop host uses an owner-restricted descriptor and ACL.

### Local signed macOS debug build

From the repository root, set `SIGNING_IDENTITY` to a local development signing identity, then run:

```sh
(cd app && ~/flutter-3.47/bin/flutter build macos --debug)
APP=app/build/macos/Build/Products/Debug/OpenFader.app
for f in "$APP"/Contents/Frameworks/*.framework "$APP"/Contents/Frameworks/*.dylib "$APP"/Contents/MacOS/*.dylib; do
  codesign --force --sign "$SIGNING_IDENTITY" --options runtime "$f"
done
for f in "$APP"/Contents/MacOS/openfader-serve "$APP"/Contents/MacOS/openfader-capture-helper; do
  codesign --force --sign "$SIGNING_IDENTITY" --options runtime --entitlements app/macos/Runner/Host.entitlements "$f"
done
codesign --force --sign "$SIGNING_IDENTITY" --options runtime --entitlements app/macos/Runner/DebugProfile.entitlements "$APP"
codesign --verify --deep --strict "$APP"
```

Sign the outer app without `--deep` so the helper's host entitlements are preserved.

Windows CI runs on GitHub's `windows-2022` image through its configured manual and release workflows. It downloads Steinberg's official ASIO SDK 2.3.4 archive,
checks its SHA-256, and installs LLVM 18.1.8 through Chocolatey before running
locked workspace checks, Clippy and tests (including the Windows descriptor ACL
test), then builds the Flutter Windows release bundle and checks its host,
helper and bridge files. The SDK is GPLv3; review Steinberg's license and
trademark terms when distributing. The build job does not exercise an ASIO
driver, device capture, or Windows desktop interaction; those require native
Windows runtime testing.
