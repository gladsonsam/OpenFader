# Output processing implementation and verification handoff

This is a partial implementation of DL32S output processing; full coverage is unverified.
The inventory contains 691 controls: Main/Aux1–8 (62 each), Subgroup1–6
(22 each), and one shared system temperature. No hardware connection or writes
were made by this implementation agent. Native evidence below was supplied by
the exclusive verifier in `/tmp/output-hardware-mapping-handoff.json`.

## Module boundaries

- `src/output_processing/map.rs`: descriptors, engineering domains, raw bank,
  encodings, addresses and processor-mode dependencies. No socket/UI logic.
- `src/output_processing.rs`: bank-qualified push hydration, per-word revisions,
  names, descriptor publication, generation/expected-value guarded writes.
- `src/native.rs`: generation-checked type-parameter/type-2 push dispatch. Other
  destination/FX caches continue to receive parameter-bank values only.
- Existing generic Flutter output workspace and destination editors render the
  new descriptors unchanged. No frontend provider switches, new timers,
  palette/fader-law changes, mode selectors or input store-contract changes.

## Address geometry and provenance

DSP base **D** and PEQ mode **E**:

| Output | D | E |
|---|---:|---:|
| Main | 4352 | D+50 |
| Aux n (1–8) | 4442+(n−1)×90 | D+50 |
| Subgroup n (1–6) | 4048+(n−1)×51 | D+8 |

`Parameter` means `VALUE_TYPE_PARAM` (wire value type 5, capture `words`).
`System` means wire value type 2 (`VALUE_TYPE_MUTE_GROUP`, capture `type2`);
that bank also carries system parameters, despite the transport constant's name.
Cache keys include the bank. Seed/readback/write all select the same bank.

| Control | Parameter address | Encoding / guarded domain |
|---|---|---|
| Compressor mode (read guard only) | D | integer; numeric/knee require 0 |
| Compressor enable | D+1 | integer 0/1 |
| Makeup, threshold | D+2/+3 | float32 0..20 / −80..0 dB |
| Attack, release | D+4/+5 | float32 0.1..300 / 50..3000 ms |
| Ratio, knee | D+6/+7 | float32 1..101 / integer 0 Soft, 1 Hard |
| PEQ enable | E+1 | integer 0/1 |
| Band b quartet | E+2+(4−b)×4 | type, gain, frequency, Q; mode E must be 0 |
| Band 1/4 shape | quartet+0 | 2 Bell / 3 Low shelf / 4 High shelf |
| Band gain/frequency/Q | quartet+1/+2/+3 | float32 ±15 dB / 20..20000 Hz / 0.5..16 |
| Main/Aux GEQ enable | D+9 | integer 0/1 |
| Main/Aux GEQ band k (1..31) | D+10+k | float32 ±12 dB; ascending 20 Hz..20 kHz |
| Main/Aux HPF enable/frequency/slope | D+42/+43/+44 | 0/1; float32 20..20000 Hz; integer 0..3 |
| Main/Aux LPF enable/frequency/slope | D+46/+47/+48 | same domains |
| Main/Aux delay enable/distance | D+68/+69 | 0/1; float32 0..115.8 m |

System target `system`, control `delay_temperature`, uses **type-2 word 7**,
float32 **0..40 °C**. It is published once as “System (all outputs)”, never
as a per-output temperature. The guide describes fixed speaker distances and
shared temperature compensation changing time. No unproven speed-of-sound
formula or synthetic ms/ft readback is introduced.

The supplied control reference guide v5.1, pp131/139/146, documents
6/12/18/24 dB/oct slopes, ±12 dB GEQ bands/master, 0..115.8 m alignment distance
(0..341.7 ms), and 0..40 °C. Distance raw metres agrees with the supplied native
observation (1.65895748 raw displayed as 1.7 m, 5.4 ft, 4.8 ms). Exact endpoint
and global temperature behavior across outputs still need native verification.
No delay/filter/GEQ descriptors are inferred onto the different Subgroup schema.

### Supplied native movement evidence

- Main GEQ enable4361; 20Hz4363, 630Hz4378, 20kHz4393. Corrected the previous
  band's one-word offset. Removed unproven GEQ master4360; do not substitute
  suspected4362 until a native movement proves it. The later parent revision
  exposes4362 as inferred GEQ master gain from desk snapshot metadata;
  the earlier movement capture alone did not establish it.
- Main compressor threshold4355, ratio4358, knee4359; PEQ band1 gain4417,
  shape4416 (2 Bell/3 Low shelf).
- Main HPF enable4394, frequency4395, slope4396: 0=6,1=12,2=18 dB/oct.
  LPF enable4398, frequency4399, slope4400: 2=18,3=24 dB/oct. Shared full
  0..3 enum on both filters is an inference from these anchors and the guide.
  Off is a separate enable, not another slope enum value.
- Main delay enable4420 and distance4421; global temperature type2/7.
- Aux1 compressor threshold4445, GEQ20Hz4453 and PEQ1 frequency4508 confirm
  +90 relative to Main for those controls.
- Verifier reports each supplied movement was restored to exact complete
  5257 parameter /107 type2 /75 name baseline. This is native movement evidence,
  not a product write or bidirectional verification of this new build.

### Remaining inferences / gaps

Aux filter/delay addresses and Aux2–8 +90 geometry, Subgroup +51 geometry,
unprobed PEQ/compressor controls, delay endpoints and shared temperature
semantics remain inferred. Native slope choices unprobed on each individual
filter remain inferred, not independently verified. Ratio101 infinity sentinel
retains the existing input-like inference.

GEQ master gain is exposed as inferred from desk snapshot metadata. Output
levels use the corrected native layout described below; output GR availability
and calibration are described in the 301-word extension below. Measured PEQ response, linked-output propagation, presets, GEQ
draw/reset, mode-1 editing and shelf-Q semantics remain incomplete. Only pushed values have readback authority; accepted writes retain
unconfirmed authority. Unknown/out-of-domain values stay unavailable. Mode,
expected value, per-word revision and connection-generation guards remain.

## Batched parent verification recipe

1. On Main and Aux1, capture one native enable/frequency/slope per filter and
   delay enable/distance, including 6/12/18/24 slopes and delay endpoints.
   Aux1 expected HPF4484/4485/4486, LPF4488/4489/4490, delay4510/4511.
   Establish raw metre scaling at two exact distances. Confirm temperature is
   one shared type2/7 parameter from both output views; probe 0/40 °C, checking
   distance stays fixed while displayed time changes. Identify GEQ master
   independently; do not write unknown4360 or treat inferred4362 as physically
   calibrated.
2. Batch stride anchors: Aux2–8 one filter or delay and terminal GEQ band on
   Aux8; Sub1–6 one compressor/PEQ anchor (no subgroup delay assumption).
   Restore exact original raw words and require full5257/107/75 equality after
   every probe, including delayed peer/link side effects.
3. Rebuild integrated product; iPad/iPhone open Main/Aux/Subgroup workspace,
   verify all tabs, 31 GEQ bands, slope choices, metre units, and single System
   temperature editor. Run representative product→native→peer→exact-restore
   loops across each established schema and both banks. Check negative values,
   NaN/Inf, fractional enum, stale editor/drag, peer change, disconnect/reconnect,
   accepted-versus-readback labels and unknown pushes. Type2/7 must never
   overwrite parameter7 or feed destination/FX parameter caches.
4. Parent owns full workspace gates, iOS builds/boot/connect/layout checks,
   Android and serialized hardware verification. No device/native control,
   hardware writes, pushes or PRs were performed here.

## Local validation of this revision

- `cargo check -p openfader-proto-dl32s --locked`: passed.
- `cargo clippy -p openfader-proto-dl32s --locked -- -D warnings`: passed.
- `cargo test -p openfader-proto-dl32s --lib --locked`: all18 existing tests passed;
  no test cases added.
- `flutter pub get`, then `flutter analyze --no-pub` in `app`: no issues. Initial
  no-pub analysis lacked this fresh worktree's dependency configuration; normal
  dependency restoration resolved it without tracked Flutter changes.
- Standalone read-only diagnostic compiled against the actual map and captured
  baseline: all691 bank-qualified addresses unique, every baseline raw value in
  domain, exact decode/encode roundtrips, NaN/Inf and fractional enum rejection.
  This checks consistency, not wire semantics. Diagnostic remains in `/tmp`,
  not in the repository; no frontend tests or Rust test cases were added.
- No full builds/workspace suites, native/device control or hardware access.

## DL32S meter geometry correction (2026-10-27)

The DL32S-only override now requests 62 strip words, then 31 passive RTA
words (93 total), then 32 six-word input dynamics rows (285 total).
`transport::mackie::dl32s_meters` owns the exact list and derived offsets;
the shared 60-word family handshake/default remains unchanged for other
callers. Legacy 60-word frames supply only input IN in the DL32S driver;
their output labels, RTA and dynamics fail closed. Exact shape and list ID
are required. The list is fixed per connection; generation/freshness and the
single shell poll loop remain unchanged. Length alone does not authenticate
a different same-length list, so concurrent layout replacements are unsupported.

Zero-based sample positions:

- 0..31: input IN (`34+7c`). Generic strip topology/meter frames stay 32 inputs.
- 32..33: Return IN (`258`, `262`).
- 34..37: FX SEND (`266..269`), output targets `fx/01..04`; tap timing unknown.
- 38..45: Aux OUT (`310+4a`), output targets `bus/01..08`.
- 46..53: four FX-return PREFADER L/R pairs (`270..277`); distinct from FX sends,
  not flattened into generic strip meters or exposed as send output levels.
- 54..59: Subgroup OUT (`278+4s`), targets `subgroup/01..06`.
- 60..61: Main OUT L/R (`302/303`), target `main`.
- 62..92: RTA (`3..33`); 93..284: input dynamics (`35+7c..40+7c`).

Provenance: supplied desk-observed meter-decoding evidence (DL32S hardware
getters and raw f32 receiver/list ordering). The supplied
output-meters evidence file contains 73 passive
explicit-116-word frames, list ID 1, with requested-address order retained;
14 old 60-word frames were discarded. `start=1` is a list ID, not address 1.
The 62-word grouping is our proposed product list. Scratch diagnostics project
captured raw words by address into it; that is not a captured 285-word stream.
The JSON has no serialized outbound request body, so a derived body hash is
not an observed wire hash. No new physical calibration is claimed.

At the geometry-correction stage, all 18 captured output COMP_GR slots were
zero. That stage's production list omitted
output GR; that revision emitted `reduction_db: None` with
`inferred: true` for Aux/Subgroup/Main, and no compressor contract for FX
sends. Nonzero output GR scale/sign calibration, physical connector correlation,
OUT/SEND tap timing, and integrated device verification remain open. The
existing GEQ master contract is unaffected by this geometry correction.

Validation for this correction: `cargo test --workspace --locked` passed
443 tests, 3 ignored. The first run failed the existing engine
`pipelined_writes_still_preempt` timing threshold (69.9 ms p99 versus 25 ms)
while Clippy was compiling; the complete rerun after compilation passed.
Clippy for `openfader-proto-dl32s` and `openfader-transport` passed with
`-D warnings`. No Dart changed, no dependencies or permanent tests were added.
Scratch production-source diagnostics checked 73 captured frames projected
into each 62/93/285-word shape (219 decodes), 19 output targets/20 samples,
all lengths 0..349, list ID/type and nonfinite rejection, RTA sentinel,
dynamics row/GR validity, unity at 0 dB, generation/staleness, and the derived
serialized-prefix hash. RTA/dynamics suffix samples in this projection were
synthetic. No hardware, mixer network, GUI/device or full app build was used.

## Calibrated output GR extension (2026-10-27)

The DL32S-only per-client request is now `dl32s_output_meter_addresses()`:
the unchanged 62/93/285 prefixes plus 16 output GR words, derived length301.
The shared family handshake, XAir and WING remain unchanged. Geometry lives in
`transport/src/mackie/dl32s_meters.rs`; `protocol.rs` constructs the request;
DL32S `lifecycle.rs` activates it; `telemetry.rs` accepts301 but bounds input
dynamics to93..285 and RTA to62..93; `output_meters.rs` normalizes output GR.
No engine, Dart contract, UI, timer, bounded delivery or output-level inferred
flag changes are needed. All modified Rust modules remain below700 lines.

Zero-based appended positions are Aux285..292 (`312+4a`), Subgroup293..298
(`280+4s`), Main L/R299/300 (`306/307`). Only exact301, type1, list ID1 carries
GR. Explicit62/93/285 frames publish absent GR, replacing prior samples;
legacy60 clears output telemetry. Negative, NaN and infinite GR samples become
absent for that output; no zero substitution. Main retains two ordered samples;
an invalid side withholds the pair. Legitimate zero attenuation stays valid.

Supplied physical evidence is `/tmp/openfader-output-gr-calibration-20261027.json`
and its minus80/minus78 captures: Aux1 native312 is network-order f32,
positive1:1 dB attenuation. Across144 frames, IN311 minus compressorOUT313
agrees within0.05106473 dB. Aux1 GR is therefore `inferred:false`; Aux2–8,
Sub1–6 and both Main sides remain `inferred:true` from static native evidence.
This does not calibrate output levels or OUT/SEND tap timing. Calibration
reports exact full restore in `/tmp/dl32s-aux1-gr-probe-restored-20261027.json`,
SHA256 `370587b45503dfe19dc9c72be3ad404087fb8d98e309e78c1ce3e428ab6062d9`,
5257 parameter words,107 type2 words,75 names. This revision made no hardware
contact or writes; restore is supplied evidence, not a new restore operation.

Separate Return gap: only Return IN258/262 is requested. Native compressor
triples259/260/261 and263/264/265 are absent from this301 list; the input
dynamics decoder still emits exactly32 input rows. Return output frames have
no compressor GR capability. Return compression metering requires its own
layout/contract/calibration work and is intentionally not added here.

Local proof: existing `cargo test -p openfader-proto-dl32s -p openfader-transport
--lib --locked` passed84 tests (including existing loopback tests), with
`TOKIO_WORKER_THREADS=2 RUST_TEST_THREADS=1`. Scoped Clippy passed with
`--locked -- -D warnings`. No Dart, dependencies or permanent tests changed.
Scratch `/tmp/openfader-output-gr-diagnostic.rs` imports production output and
telemetry modules, linked against this worktree's production transport/core.
It projects all144 actual calibration frames by requested address into301;
RTA/input dynamics are synthetic. It checks25 targets, unchanged level flags,
32 bounded dynamics rows,31 RTA bins, all lengths0..349, wrong type/list ID,
negative/nonfinite GR, independent MainL/R, short-shape unavailability,
generation mismatch and expiry. The serialized301 request body is in
`/tmp/openfader-output-gr-301-layout.bin` (derived, not observed on wire).
No live301 stream, device UI or full build was verified here.

### Parent-only live301 and both-simulator recipe

1. Integrate this commit into the parent's current branch and finish the full
   workspace gate with `TOKIO_WORKER_THREADS=2 RUST_TEST_THREADS=1 cargo test
   --workspace --locked`. Rebuild the integrated bridge/app, not an older
   engine binary. Request301 only on the DL32S connection; capture MeterLayout
   body `[00 00 00 01]` plus301 BE-i32 addresses (1208 bytes total), its ACK,
   then unchanged BroadcastControl and ACK. Compare the first285 addresses
   exactly to `dl32s_dynamics_meter_addresses()` and suffix exactly to
   `[312,316,320,324,328,332,336,340,280,284,288,292,296,300,306,307]`.
2. Capture pushed type1/list1 frames with exactly301 raw words. Confirm input
   IN0..31, Return32..33, sends34..37, Aux38..45, FX returns46..53,
   Sub54..59, Main60..61, RTA62..92, dynamics93..284 are unchanged. Verify
   `output_meters.frames`: bus/01 mono GR equals f32(word285), inferred false;
   bus/02..08, subgroup/01..06 and Main GR inferred true; Main has ordered
   `[f32(word299),f32(word300)]`. Every output level remains inferred true.
   A301 capture cannot itself reproduce IN311-OUT313: request those native
   meters in a separate parent-owned calibration session, never reinterpret
   unrelated301 slots as compressor IN/OUT. Serialize layout sessions.
3. Parent may repeat the established Aux1 -80/-78 threshold probe using the
   supplied baseline/restore protocol; require GR agreement <=0.052 dB and
   exact5257/107/75 raw equality after restore. Other outputs need their own
   nonzero physical probes before dropping inferred status. In an offline
   injected feed check301->285 clears GR, legacy60 clears outputs, malformed
   shapes are rejected, >1s samples expire, reconnect clears generations, and
   input dynamics/RTA stay live. Do not send malformed meter data to hardware.
4. In the integrated `app` directory run `PATH="$HOME/flutter-3.47/bin:$PATH"
   flutter build ios --simulator --debug`. Parent calls `device_list` and
   `device_open` for iPad Pro13-inch and iPhone18 Pro, using the returned
   independent agent-device sessions. Install `build/ios/iphonesimulator/Runner.app`
   on each UDID with `xcrun simctl install <udid> <app-path>` and launch
   `xcrun simctl launch <udid> com.openfader.flutter.experimental`.
   First Work offline verifies boot/navigation; then connect each sequentially
   to the parent-owned live DL32S session. Open Aux1 output Compressor and
   verify positive calibrated GR, Aux2/Sub/Main inferred labels, separate
   MainL/R, no GR capability on FX/Return, and input/RTA regression views.
   Check idle, nonzero, disconnect/stale and reconnect states on both sizes;
   capture `xcrun simctl io <udid> screenshot <path>` and compare at the
   specified design dimensions. These live/device checks remain parent work.
