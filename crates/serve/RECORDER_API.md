# Recorder API v1

This document describes the recorder interface. The loopback bootstrap enables
the recorder service on macOS and Windows. macOS capture starts are temporarily
disabled because CoreAudio stream initialization can block and later resume after
a client timeout; `capabilities.recording` is false. macOS desktop discovery and authenticated shutdown
have passed an unsandboxed smoke test; Windows desktop hosting, remote pairing,
and physical soundcheck remain unavailable.

All responses are JSON. `GET /api/recorder/v1/state` returns `available`,
`input_mode` (`unknown`, `live`, `soundcheck`, or `recovery_required`),
`transport` (`stopped`, `recording`, `playing`, or `paused`),
`selected_device_id`, `selected_take_id`, `position_frames`, `sample_rate`,
`recorded_frames`, `disk_free_bytes`, `xruns`, `error`, and `capabilities`
with boolean `recording` and `soundcheck`. Transport and input mode are
independent: stopping playback must not implicitly return the desk to live.
`GET /api/recorder/v1/devices` returns `{ "devices": [...], "scan_pending": boolean, "scan_error": string|null }`. The first request starts an asynchronous scan; `POST /api/recorder/v1/devices/refresh` requests another scan when no scan is running. A blocked native scan remains the sole in-flight scan and never blocks other recorder operations. The last successful list remains available during refresh.

`GET /api/recorder/v1/takes` returns `{ "takes": [...] }`.

`POST /api/recorder/v1/command` accepts `{ "request_id": string, "action":
string, "args": object }` and returns `{ "ok": boolean, "request_id": string,
"error": string|null }`. Actions are `select_device` (`device_id`),
`start_recording`, `stop`, `pause`, `seek` (`position_frames`), `set_loop`
(`start_frame`, `end_frame`, `enabled`), `enter_soundcheck` (`take_id`), and
`return_live`. Requests are serialized and duplicate
request IDs replay the original result; reusing an ID with different contents
must fail. `return_live` is explicit and must restore only journaled routing,
never a scene. A restart with a pending journal requires recovery before any
new routing transition. Recovery must not overwrite a conflicting external
routing change.

Operational endpoints require authentication: a local desktop bootstrap must
prove possession of a per-instance secret; remote access requires explicit
pairing and permissions. Neither loopback origin nor a browser Origin header is
an authentication credential. Remote pairing, permissions, complete error codes, and durable request-ID
retention are not yet implemented. Clients must treat unavailable
capabilities and failed commands as authoritative.

XR18 soundcheck additionally requires a verified complete input and USB
routing map (including stereo aux 17/18), confirmed writes, and safe recovery.
No physical switching is currently enabled. Recording must be independent of
mixer network connectivity once started.

## Local bootstrap contract

Launch `openfader-serve --bind 127.0.0.1 --port PORT` with a fresh,
cryptographically random secret of at least 32 characters in
`OPENFADER_RECORDER_TOKEN`; never put the secret on the command line. Supply
`Authorization: Bearer <secret>` on every recorder request. The desktop parent
owns and transmits this secret out of band. A host bound to a non-loopback
address will not enable the recorder with this bootstrap. Until remote pairing
and a secure transport exist, remote recorder control is unavailable. The
existing `/ws` mixer API requires the same Authorization header when this
bootstrap is enabled and shares the host engine. Bind it to loopback. The default server CLI bind is not
safe for this desktop usage; pass the explicit bind shown above.

Command `args`: `select_device`: `{ "device_id": string }`;
`start_recording`: `{ "take_id": string }`; `stop`: `{}`; `pause`: `{}`;
`seek`: `{ "position_frames": integer }`; `set_loop`: `{ "start_frame":
integer, "end_frame": integer, "enabled": boolean }`;
`enter_soundcheck`: `{ "take_id": string }`; `return_live`: `{}`. Take IDs
are single path components. Commands and audio calls serialize on a worker.
The `load_take` action accepts `{ "take_id": string }`; pause, seek, and loop
are passed to the audio service and require an appropriate transport state.
Physical playback and soundcheck remain unavailable while routing is unverified. Request-ID replay is limited to this host process;
clients must generate unique IDs and query state after a restart.

## Desktop lifecycle contract

On macOS, the desktop client starts `openfader-serve --desktop-host` without a token,
port, or mixer credentials on the command line. The host chooses a loopback
port and an independently generated secret. At most one host owns the current
user session; a second invocation exits without replacing the owner. The client
first tries the descriptor and starts the host only when it cannot validate a
running owner. It must retry discovery during startup and after a competing
launch; it must not delete an apparently stale descriptor itself.

The descriptor is `$HOME/Library/Application Support/OpenFader/Recorder/host.json`
on macOS (using the HOME inherited by the host, including the sandbox container).
The planned Windows path is `%LOCALAPPDATA%\OpenFader\Recorder\host.json`;
Windows desktop hosting is not yet implemented. Its
schema is `{"version":1,"instance_id":"<random instance identifier>",
"endpoint":"http://127.0.0.1:<port>","token":"<random bearer secret>"}`.
The descriptor and containing directory are restricted to the current user
(Unix file mode 0600, directory 0700; Windows current-user ACL). The client
must reject any non-loopback endpoint, unsupported version, insecure descriptor,
or identity mismatch, and never log or transmit the token elsewhere. Requests
use `Authorization: Bearer <token>` including WebSocket upgrades. Authenticated
`GET /api/recorder/v1/host` returns `{"version":1,"instance_id":"..."}`;
the instance ID must match the descriptor before attaching. A missing, malformed,
or stale descriptor is not evidence that a host can be terminated.

Authenticated `POST /api/recorder/v1/shutdown` with an empty JSON object returns
`{"ok":true}` only when the host has accepted shutdown. It returns HTTP 409
with `{"ok":false,"error":"..."}` while recording, a routing transaction,
or unresolved routing recovery is active. Unauthenticated calls receive 401.
Closing the client does not request shutdown; the host persists and can be
reattached. The macOS implementation has passed an unsandboxed discovery, authentication,
single-instance and shutdown smoke test; sandbox packaging and reattachment
still need verification. Windows desktop hosting and remote mobile pairing are
unavailable. Physical routing and recovery integration are not implemented, so
the journal check does not make soundcheck safe to enable.

## Waveform peaks contract

Authenticated `POST /api/recorder/v1/waveform` accepts JSON
`{"take_id":"...","start_frame":0,"end_frame":48000,"buckets":512}`.
The interval is half-open `[start_frame,end_frame)`. `take_id` is a single
component. Requests are bounded to at most 1,000,000 frames and 2,048 buckets;
`1 <= buckets <= end_frame-start_frame`. Responses are
`{"take_id":"...","start_frame":0,"end_frame":48000,"buckets":512,
"channels":[[[min,max],...],...]}` with exactly 18 channel arrays. Each bucket
covers consecutive frames partitioned by integer boundaries across the requested
range. Min/max are normalized signed 24-bit sample values in `[-1,1]`, computed
from actual stored samples. A missing, incomplete, corrupt, or out-of-range take
returns an error, never synthetic peaks. The endpoint is read-only and does not
use the audio callback or send raw audio samples. Clients request only their
visible range and should discard stale responses when zooming or changing takes.
Bounded concurrency may return HTTP 503; clients can retry.

During capture, `state.live_peaks` is an array of 18 normalized absolute peak
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
and does not download one. The secure Windows desktop host is not yet implemented;
the bundled executable fails closed when started with `--desktop-host`.

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

Windows CI runs on GitHub's `windows-2022` image on pushes, pull requests,
and manual dispatch. It downloads Steinberg's official ASIO SDK 2.3.4 archive,
checks its SHA-256, and installs LLVM 18.1.8 through Chocolatey before running
locked workspace checks, Clippy and tests (including the Windows descriptor ACL
test), then builds the Flutter Windows release bundle and checks its host,
helper and bridge files. The SDK is GPLv3; review Steinberg's license and
trademark terms when distributing. The build job does not exercise an ASIO
driver, device capture, or Windows desktop interaction; those require native
Windows runtime testing.
