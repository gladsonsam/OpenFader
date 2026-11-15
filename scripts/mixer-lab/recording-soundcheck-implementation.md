# Recording and virtual soundcheck implementation

Implemented 2026-11-15. This report describes code and available software checks;
it does not certify any physical mixer, firmware, USB interface or iOS simulator.
Provider cells remain partial / **Testing needed**, with unavailable model paths
kept explicit in their inventories. No completion evidence has been registered.

## Shared flow

The host audio service owns capture and playback independently of the network
mixer. Drivers supply actual capture identities, USB/card output assignments,
route changes, guard state and snapshot dependencies through the existing shared
contracts. Application code does not select routing by mixer brand.

At record start, a matching interface and fresh provider descriptor supply
physical source identities in capture order. Unknown/manual captures remain
usable without invented identities. Names are display metadata, not routing IDs.

Play refreshes the connected provider's routing plan and interface association.
Saved source identities resolve reordered tracks to explicit outputs. An import
without usable mapping asks for track-to-source assignments; the confirmed
mapping is saved with the take separately from original capture provenance.
Unassigned tracks are silent. Native copies and numbered mono-track export/import
preserve identities with the audio and names, including reordered track folders.

The helper opens the real output stream and prebuffers silently before routing
changes. The coordinator journals and confirms native routing, then starts audio.
Return live closes and reaps output before restoring the original routing in
reverse order. Processing, faders and mute changes are retained. Restart/reconnect
recovery reconstructs guarded driver state and requires the existing explicit
rebind step; conflicting external changes are not overwritten.

The audio stream persists through pause/resume, seek and loop edits. Buffered
frames carry transport epochs so old audio and EOF cannot leak across a seek.
Short fades reduce clicks at transport, loop and EOF boundaries. Seeking may
include a deliberate prefetch interval of silence; loop edges use fades rather
than an overlapping crossfade. Device timing and audible continuity still need
real audio verification. Xruns and failures come from the actual callback/worker.

A cancelled read-only routing preparation does not place recording/manual
playback into routing recovery. Cancellation after output/routing may have changed
continues to require confirmed termination and recovery.

## Provider implementations

- **DL32S:** fresh two-pass routing receipts, physical Mic Pre USB sends, explicit
  capture/output mapping, B assignment and A/B selection, reserved live USB
  returns, link/source dependencies and independent write readback. Existing B
  assignments and selections are restored. Post-DSP sends are not assumed to be
  suitable pre-processing soundcheck sources.
- **XR18 / MR18:** explicit multichannel identities, fresh USB source/tap and input
  selectors, reordered capture sends, linked pairs through their existing USB
  selectors, stereo aux return switching, live USB reservations and restoration.
  Models/selector shapes are validated rather than accepting a sibling's defaults.
  XR12/XR16 do not inherit an 18-channel host interface profile.
- **X32 / M32:** observed supported USB card identities and channel modes,
  physical local/AES50/AUX capture sources, inactive REC/PLAY bank preparation from
  either starting mode, preserved live card/AUX returns and original-mode
  restoration. Capture and return channel widths may differ. Non-USB expansion
  cards, unresolved user/processed routes and sources absent from the available
  host channel range remain explicit unavailable cases.
- **WING family:** observed USB configuration and native output inventory,
  physical-source capture identities, tuple-aware native decoding, mono/stereo/M-S
  mapping, journaled USB source modes and automatic-source selection, inactive
  main/alternate patch preparation, preserved live USB consumers and restoration.
  Unresolved user-signal dependencies, unavailable native modes, nonadjacent
  stereo captures and return conflicts remain explicit unavailable cases.
- **Simulation:** explicit fixture capture identities and output mapping exercise
  the shared 32-channel path; they are never a hardware fallback.

A capture lane currently maps to at most one host output. Multiple mixer inputs
can share that return when their provider allows it. Configurations requiring the
same recorded lane on multiple distinct outputs fail with a reason instead of
silently dropping a return. Duplicate source recordings require explicit track
selection. Audio interface identity is associated by reported model/name and
layout; the user must choose the intended interface when several match.

## Validation and limits

Toolchains: Rust 1.100.0 and Flutter 3.50.0, as pinned by CI.

- Workspace `cargo check --workspace --locked`: passed.
- Workspace `cargo clippy --workspace --all-targets --locked -- -D warnings`:
  passed.
- Audio library: 39 tests passed, including persistent transport, stale-epoch
  rejection, masks, mapping provenance and reordered track-folder round trips.
- Recorder host library: 58 tests passed, including cancellation without
  quarantine and the generic 32-channel playback preparation fixture.
- `flutter analyze` in `app`: clean.
- Linux debug app build: passed; current native host/helper copied into the bundle.
  Isolated Xvfb launch reached Connections and the offline mixer. Screenshots are
  under `/tmp/openfader-soundcheck-smoke/`. This is not iOS simulator evidence or
  an end-to-end physical audio check.
- Landing documentation build: passed.
- Integration inventory check: consistent, 33 models / 31 features / 714 cells;
  this is inventory integrity, not support certification.
- X32 routing lab: failed at the existing attachment/initial-hello check before
  routing verification. Source-bound report:
  `/home/gladson/.cache/openfader-mixer-lab/runs/20261115-174103-486366/report.json`.
- WING source lab: 15 checks passed and 6 explicitly blocked, with no failed
  checks. The pinned fixture aliases local and AES50 sources and lacks physical
  preamp fidelity. Report:
  `/home/gladson/.cache/openfader-mixer-lab/runs/20261115-174639-679621/report.json`.
  This source scenario does not verify the new USB audio/soundcheck path.
- No matching DL32S or X Air lab emulators are available. No physical mixers,
  firmware/audio loopbacks, iPad Pro 13-inch or iPhone 18 Pro simulators were
  available on this Linux host. All remain Testing needed.

Two initial parallel workspace runs encountered failures in unchanged helper/
network tests (a three-second listener timeout and transient Linux ETXTBSY while
executing a generated device-scan helper). The final workspace run uses
`RUST_TEST_THREADS=1` to avoid parallel helper-process interference. See the final
result below and retained task logs in `/tmp/openfader-soundcheck-*.log`.

Final workspace result: **passed**, 547 tests across 43 result groups, zero failures (`RUST_TEST_THREADS=1 cargo +1.100.0 test --workspace --locked`).
