# OpenFader mixer verification lab

Manual driver verification against external, pinned Sidecar emulators. The real
OpenFader engine receives Flutter commands over WebSocket; independent OSC reads
verify writes, and external OSC writes verify incoming state. Reports establish
only their listed emulator checks, not physical mixer compatibility.

## Run

Requires Python 3.10+, Git, stable Rust, OpenFader's native build dependencies and
Dart from the pinned Flutter SDK. Set `DART=/path/to/flutter/bin/dart` if needed.
No display, audio interface, JUCE or Sidecar GUI is required.

```sh
python3 scripts/mixer-lab/lab.py setup
python3 scripts/mixer-lab/lab.py matrix
python3 scripts/mixer-lab/lab.py run x32
python3 scripts/mixer-lab/lab.py run x32 --quick
python3 scripts/mixer-lab/lab.py run x32 --scenario processing
python3 scripts/mixer-lab/lab.py run x32 --scenario fx
python3 scripts/mixer-lab/lab.py run x32 --scenario routing
python3 scripts/mixer-lab/lab.py run x32 --scenario reconnect
python3 scripts/mixer-lab/lab.py run x32 --scenario native
python3 scripts/mixer-lab/lab.py run wing wing-compact wing-rack
python3 scripts/mixer-lab/lab.py run all
```

With no models, `run` selects runnable fixtures; `all` also includes blocked
fixtures. Exit codes: 0 passed, 1 failed, 2 explicit coverage blockers. `--quick`
runs basic and extended checks. Full X32 runs add processing, routing, FX, native
stereo pairs, guarded preamps, scenes, meters, external updates, reconnect and
native-library workflows. JSON reports retain each check and omission.

`setup` fetches the clean revision in `matrix.json` into
`~/.cache/openfader-mixer-lab` (override `OPENFADER_LAB_HOME`). The separate adapter
seeds dependencies from Sidecar's lockfile once, then builds with `--locked`.
Runs rebuild OpenFader, resolve Cargo's configured target directory, and record
executable, tooling and adapter-lock SHA-256 fingerprints. Logs, locks and build
outputs stay outside the repository. Use a new `--output /absolute/path` for a
chosen evidence directory. Rerun `setup` after adapter changes. Never patch the
emulator to make a compatibility check pass.
Source and probe files must stay unchanged throughout a run; the runner rejects
results if their fingerprints change. Each stage records its own exit status.

Each fixture gets fresh emulator and host processes, bounded probes and cleanup
on failure. Listeners use 127.0.0.1; occupied ports fail without killing existing
processes. Run sessions sequentially and stop emulators before repository tests,
which use the same discovery ports. Do not run these probes against live hardware.

Sidecar is AGPL-3.0-only and stays external: its code is neither copied into the
product nor linked into shipped binaries. Review licensing before distributing
the adapter or incorporating upstream code.

## Manual app connection

```sh
python3 scripts/mixer-lab/lab.py emulator x32
```

The emulator stays alive until Ctrl-C. Connect a locally running OpenFader app
to 127.0.0.1 using its matching provider or automatic identification. Other
listeners: wing, wing-compact, wing-rack, sq5 and dlive. SQ/dLive have no registered
OpenFader driver; do not substitute another provider. The lab has no LAN binding.

The automated loop does not launch Flutter. On a Mac, run the lab locally and
follow AGENTS.md for both iOS simulator boot, connection and screenshot checks.
Physical phones cannot reach this host through their own loopback address.

For a Linux desktop check, build the native host first and pass Cargo's actual
target directory to Flutter's bundle step. Otherwise a global Cargo config can
leave the bundle using an older engine from the repository's `target` directory:

```sh
cargo build --locked -p openfader-serve -p openfader-audio
export CARGO_TARGET_DIR="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
cd app
flutter build linux --debug
```

Verify the bundled `openfader-serve` SHA-256 matches the lab executable and
restart the isolated UI and desktop host before checking controls.

## Development loop

1. Select one provider and feature from `matrix.json` and existing reports.
2. Keep mappings, native ranges/options and validation beside the driver. Audit
   shared assumptions about counts, stereo topology, laws and unavailable controls.
3. Extend the named fixture probe. Require independent wire readback and fresh
   incoming events; restore mutations on failure. Do not add frontend suites or
   expand desk protocol tests.
4. Verify a differing descriptor where relevant, then stop emulators and run
   `cargo test --workspace --locked` and `flutter analyze` in `app`. Existing
   simulator checks remain required.
5. Report exact evidence and gaps. Hardware verification requires physical desk
   evidence with model and firmware. The lab neither starts agents nor runs in CI.

## Coverage limits

The matrix includes Sidecar profiles plus X Air and DL32S. WING variants and X32
have runnable command/readback probes. SQ/dLive need drivers; Yamaha needs a MIDI
adapter and driver. PRO2 lacks a published control protocol in Sidecar's profile;
XENYX Q1202USB has no physical remote control protocol. Universal is a software
console. X Air and DL32S have no matching Sidecar emulator. These stay blocked.

The pinned X32 emulator lacks main-strip group leaves, user-routing nodes and
native show/library OSC endpoints. It implements six of fourteen matrix/main EQ
types and differs from native FAC1M, PIT2 and PIT parameter schemas. These cases
remain blocked, including echoes with mismatched parameter meanings. Native
ranges follow the protocol author's [reference](https://drive.google.com/file/d/1Yt_S1mpPt3CAzeq3Dnpe_IqctQ-1GlTz/view)
and [tables](https://github.com/pmaillot/X32-Behringer).
Processing includes fixed interior conversion points. FX checks cover every
algorithm in all eight slots, with complete slot-1 parameters and first/last
parameters elsewhere; repeated interior parameters remain explicitly inventoried.

`native.json` records missing-endpoint evidence and requires incomplete inventory
receipts rather than invented empty slots. Scene-kind archive export represents
the current live mix, with that provenance; it does not export a selected saved
scene or vendor USB file. Recorder/USB, manual virtual soundcheck, LAN changes,
physical DSP, phantom power, preamps and audio need dedicated evidence. Desktop
UI checks supplement the command loop and do not replace iOS simulator checks.
