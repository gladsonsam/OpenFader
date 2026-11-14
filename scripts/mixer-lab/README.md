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
python3 scripts/mixer-lab/lab.py run x32 --scenario console
python3 scripts/mixer-lab/lab.py run x32 --scenario input-processors
python3 scripts/mixer-lab/lab.py run wing wing-compact wing-rack
python3 scripts/mixer-lab/lab.py run wing --scenario source
python3 scripts/mixer-lab/lab.py run wing --scenario fx
python3 scripts/mixer-lab/lab.py run wing --scenario console
python3 scripts/mixer-lab/lab.py run wing --scenario processors
python3 scripts/mixer-lab/lab.py run all
```

With no models, `run` selects runnable fixtures; `all` also includes blocked
fixtures. Exit codes: 0 passed, 1 failed, 2 explicit coverage blockers. `--quick`
runs basic and extended checks. Full X32 runs add processing, routing, FX, native
stereo pairs, guarded preamps, scenes, meters, external updates, reconnect and
native-library workflows. JSON reports retain each check and omission.
Full WING runs cover processing, main/alternate input sources, guarded preamps,
tagged delay units, filters, sends, groups, FX, local scenes, meters and reconnect.
WING focused scenarios are `processing`, `source`, `sends`, `fx`, `lifecycle`, `processors` and `console`.

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

WING FX checks use independent factual tables from the pinned libwing schema
linked in `wing/fx-reference.json`; its Apache-2.0 license is retained at
`crates/proto-wing/src/schema/LICENSE.libwing`. All eligible algorithms are
selected in all 16 slots; complete parameter sweeps run in slots 1 and 9,
with first/last parameters elsewhere. The three WING identities share one
Sidecar DSP profile, so they do not prove differing physical I/O behavior.
Sidecar can store unbound parameters: reports distinguish reviewed engine
mappings from protocol readback whose DSP binding has not been audited.
Its headless host publishes no native show; physical output routing, native
libraries, audio, USB and physical preamps still require separate evidence.
The dedicated channel screens cover the standard GATE, STD EQ and COMP models
plus the four filter tools. Tools → Console controls additionally browses the
connected WING's live native definitions, including alternative DSP models and
their parameters, monitor/listen, inserts and native routing. Only observed,
understood native domains enable editing. Model-dependent writes pin and
revalidate the model and definition. Open-show inventory is read-only and must
be explicitly reported by the console; filesystem transfers remain unavailable.
Documented show, player and recorder actions require live context and observable
readback. Missing endpoints and unknown metadata stay visibly unavailable.
This browser is separate from the 65-model FX rack catalogue.

The X32 console browser supplies driver-owned settings, monitor/solo, talkback,
oscillator, linking, automix, main configuration, remote/MIDI, DP48, iQ speakers,
user assignments and installed-card controls. Card and speaker controls pin the
observed native model. A DP48 broadcast acknowledgement proves acceptance only;
it does not prove downstream devices changed. X-LIVE recording requires writable
media evidence and remains unavailable without it. Sidecar omits many of these
global leaves; `console.json` retains the missing endpoint receipts.

Native write attempts retire cached parameter, routing, FX and editor proof in
the same connection epoch; fresh reads must rebuild it. The console probe checks
observed revisions, malformed/stale writes, independent OSC readback, external
changes and other-client locks wherever the fixture provides a writable control.
It does not provision restricted roles or invent reversible transport actions.
WING meter transport requires every requested collection to be fresh before
publishing an aggregate. Only the documented first eight DCA meters are requested;
the remaining DCA controls retain unavailable meters rather than measured zeroes.

The pinned X32 emulator lacks main-strip group leaves, user-routing nodes and
native show/library OSC endpoints. It implements six of fourteen matrix/main EQ
types and differs from native FAC1M, PIT2 and PIT parameter schemas. These cases
remain blocked, including echoes with mismatched parameter meanings. Native
ranges follow the protocol author's [reference](https://drive.google.com/file/d/1Yt_S1mpPt3CAzeq3Dnpe_IqctQ-1GlTz/view)
and [tables](https://github.com/pmaillot/X32-Behringer).
Processing includes fixed interior conversion points. FX checks cover every
algorithm in all eight slots, with complete slot-1 parameters and first/last
parameters elsewhere; repeated interior parameters remain explicitly inventoried.

The X32 native console-control surface has typed monitor, talkback, oscillator,
linking, recorder settings, automix, remote/MIDI, user assignment, DP48, iQ and
installed-card controls. X-LIVE stop, pause and play require an observed X-LIVE
card and live transport state; play also requires an observed saved session.
Recorder audio is not verified by an OSC state readback. X-LIVE recording stays
unavailable until writable SD-media presence can be established, and USB recorder
transport remains unavailable without a verified command contract. DP48 scope
broadcast reports an acknowledgement only, never downstream speaker state. iQ
sound choices require the observed model; conflicting EQ path spellings are
used only when exactly one independently answers. User assignment editing accepts
validated documented codes; ambiguous page-jump syntax is not available. The
emulator does not establish these controls' behavior on a physical desk.

Talkback A/B destination bitmaps, the lamp's raw native dimmer value, preference
profile name and device name are also observed read-only. Their descriptors show
why editing is unavailable: destination bit ordering, lamp wire scaling, or text
write limits and identification effects still lack independent confirmation.
These observations carry no write revision and are absent from the writable
registry. A raw lamp value is not presented as a percentage. Factual paths and
wire types follow the protocol author's
[configuration table](https://github.com/pmaillot/X32-Behringer/blob/master/X32CfgMain.h)
and [preferences table](https://github.com/pmaillot/X32-Behringer/blob/master/X32PrefStat.h).

`native.json` records missing-endpoint evidence and requires incomplete inventory
receipts rather than invented empty slots. Scene-kind archive export represents
the current live mix, with that provenance; it does not export a selected saved
scene or vendor USB file. Recorder/USB, manual virtual soundcheck, LAN changes,
physical DSP, phantom power, preamps and audio need dedicated evidence. Desktop
UI checks supplement the command loop and do not replace iOS simulator checks.

## Native processor scenes and remaining integration

Application-local scene format v11 carries typed provider-owned processor blocks.
WING 3.0.5 and 3.1 have distinct registered rosters (184 and 192 strip/domain
blocks); 3.1 includes Aux dynamics. Capture requires a complete observed roster
and revalidates model, definition and values before committing. Recall validates
all available static context first, selects models when necessary, checks their
live definitions and verifies each value. A failure after a model or value write
is reported as potentially partial. OSC does not provide atomic desk snapshots,
transactions or proof of unreceived model changes.

The driver tables retain pinned libwing facts and explicitly documented 3.1
variants from the [current protocol manual](https://cdn-media.empowertribe.com/2106004e217b8eb2639d93672823eef5/WING_Remote-Protocols-3.1-03.pdf).
Native scalar domains and logarithmic/linear grids come from the connected
console's DD. Readback checks accept the requested float32 value or its predicted
native step with a bounded rounding allowance; integers and enum choices remain
exact. WARM's 3.1
`wmix` additionally requires the registered model-scoped native identity wherever
available, its index, label, unit and domain. Unknown or ambiguous state fails
closed instead of creating an incomplete scene.

`processors` independently exercises all 71 input gate/EQ/dynamics model-slot
combinations in the 3.1 reference. It writes and reads every inventoried field,
uses OpenFader for model selection and representative native field kinds, then
saves a local scene, changes model and values, recalls and verifies every saved
field. Original state is restored and the disposable fixture is stopped.
`OPENFADER_LAB_PROCESSOR_MODELS=GATE,DUCK,DEQ2` narrows this diagnostic; reports
record the selection and do not treat it as a full sweep. Historical reference
boundary differences remain recorded; valid endpoints are selected from
independently queried current DD, without changing the emulator.

X32 advertises guarded processor presets and complete strip processing for exact
provider-declared strip keys, including bus, main and matrix EQ/compressor blocks.
Output strips use their existing strip-processing page and actual six-band EQ
domains; this does not advertise an output GEQ or invent destination DSP rows.
The engine enforces the same target list carried by the UI and binds it into the
guard context. Singleton mains have an explicit non-linkable witness; other
paired strips require verified unlinked state. Individual presets preserve enable
flags, while complete processing copy includes supported enable flags.
`input-processors` checks independent capture/write/readback, differing output
domains, sibling and stale context rejection, partner locks and restoration.
Current output preset/clipboard UI verification on both iOS simulators remains
pending. Restricted-role provisioning,
unreceived ABA transitions and a safe link-transition fixture remain separate
limits. Linked X32 input blocks remain unavailable until member-aware guards
can prove the complete partner state. Writable X32 talkback destination bitmaps,
some native preferences and selected stored-scene
export still need provider integration or a verified native protocol. These are
software gaps in addition to the emulator and hardware evidence limits.
WING clipboard, input presets and output presets still fail closed;
they need scoped, typed native capture/apply and versioned preset integration.
The old positional gate/compressor selectors are unavailable on WING 3.1 or an
unknown firmware, where the native browser supplies the actual model choices.
These gaps and the physical/media limitations above prevent a claim of complete
hardware compatibility.
