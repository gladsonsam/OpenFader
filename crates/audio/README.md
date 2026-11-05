# openfader-audio

On macOS and Windows, `RecorderService` exposes the following synchronous API
(all fallible methods return `std::io::Result`):

```rust,ignore
RecorderService::new(root: impl AsRef<Path>) -> Result<RecorderService>
service.devices() -> Result<Vec<DeviceInfo>>
service.select_device(id: &str) -> Result<()>
service.status() -> RecorderStatus
service.start_recording(take_id: &str) -> Result<()>
service.stop() -> Result<()>
service.takes() -> Result<Vec<(String, TakeManifest)>>
service.recover_take(take_id: &str) -> Result<TakeManifest>
service.load_take(take_id: &str) -> Result<TakeManifest>
service.play() -> Result<()>
service.pause() -> Result<()>
service.seek(frame: u64) -> Result<()>
service.set_loop(start: u64, end: u64, enabled: bool) -> Result<()>
```

Calls must be serialized by the host. Device IDs come from CPAL and should be
persisted as opaque strings. Capture uses the selected device’s reported input layout; playback uses its output layout and the take’s channel mapping. Stream configuration must be supported at the device’s current default sample rate. Playback is silent until
`play()`; routing is the host's responsibility. No mixer control commands are
sent. `status()` reports transport, frame position, captured frames, xruns and
errors; call `stop` after a capture error and `recover_take` before inspecting
an interrupted take. Recovery retains only the last durable checkpoint and
finalizes the recovered manifest so it can be loaded. Checkpointing and stream transitions can block;
call these methods away from a UI or real-time audio thread.

`TakeWriter` stores complete interleaved, layout-specific 24-bit PCM frames in
synchronized WAV segments. Storage runs on a worker fed by a bounded,
preallocated queue; a capture overflow fails the take rather than silently
joining discontinuous audio. Playback reads ahead on a worker, with an output
ramp on starting and stopping. At EOF output remains silent and transport reports
stopped at the final frame; play again restarts from zero. A playback underrun
outputs silence and holds the take position until queued audio resumes; `xruns`
counts starvation episodes, not silent frames. Pause, seek and loop changes
destroy the old stream and queue before starting another generation. These
transitions may briefly block; pause and loop changes retain the last rendered
position. Device or worker errors silence playback and appear in status; stop
returns the failure and releases the worker. Stream callbacks perform no heap
allocation, disk I/O or blocking locks.

During capture, the disk worker also publishes real per-channel PCM minima and
maxima in 20 ms bins. The arrangement reads this bounded preview at the existing
shell poll cadence, grows each channel's clip and follows the recording playhead.
Preview I/O failure leaves recording running and makes waveforms unavailable;
it never substitutes generated samples. The temporary peak file is removed after
successful finalization; completed-take previews read the recorded WAV data.

Run `cargo run -p openfader-audio --example capture_smoke --locked` to list devices, then repeat with a device ID as the final argument to record five seconds into a new temporary directory and verify WAV headers and frame alignment. On macOS microphone permission must be granted to
the hosting executable. The example does not create an output stream.

Windows ASIO builds use pinned CPAL 0.18.2 and require Steinberg's [ASIO SDK](https://www.steinberg.net/asiosdk),
a Windows C++ toolchain and LLVM/libclang. Set `CPAL_ASIO_DIR` to the SDK
root containing `common`, `host`, and `host/pc`; `asio-sys` otherwise attempts
to download the SDK at build time, which should not be relied on for reproducible
builds. See the
[CPAL build instructions](https://github.com/RustAudio/cpal#compiling-for-asio).
Device enumeration runs in a disposable helper with a deadline. Refresh is held
while recording or soundcheck owns the interface, and stream creation checks
the selected rate and ASIO channel layout again. Driver resynchronization or
device errors stop the affected transport. There is no silent WASAPI fallback.

The portable Windows ZIP includes the application, local host, audio helper,
and Visual C++ runtime DLLs. The recording PC needs the manufacturer's 64-bit
ASIO driver, not Rust, Flutter, Visual Studio, LLVM, or the ASIO SDK. Extract the
ZIP into one folder; keep the DLLs and `data` directory beside the executables.

The packaged `check-audio.ps1` lists ASIO interfaces without opening outputs:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\check-audio.ps1
```

To explicitly record ten seconds using every advertised input, copy the exact
device ID from that list:

```powershell
.\check-audio.ps1 -DeviceId 'ASIO:exact driver ID from list' -RecordSeconds 10 -OutputDirectory D:\AudioChecks
```

The script creates an isolated take folder and `check.json`, checks the reported
rate, channel count, completed frames, WAV lengths and xruns, and leaves all
results for inspection. It never opens outputs or changes mixer routing. Close
other audio applications before checking a driver that permits only one client.
Buffer size and hardware clock remain controlled by the manufacturer's panel;
refresh and reselect after changing them. Recordings use buffered disk I/O and
WAV segments capped below 4 GiB, including 64-channel and high-rate layouts.

Playback, seek, pause, loops, manual output mapping and routing recovery use the
same host contracts as macOS. Manual virtual soundcheck is independent of a LAN
mixer. Automatic desk routing still requires a physically verified provider
profile; in particular, DL32S automatic routing remains unavailable until its
USB send tap and selector readback are verified. Build checks and synthetic
tests do not establish physical signal routing or sustained hardware performance.
