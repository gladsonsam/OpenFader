# Output processing implementation and verification handoff

This is a partial output-processing implementation, not full Master Fader parity.
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

Guide source: `/tmp/master-fader-v5-1-reference.pdf`, pp131/139/146. It documents
6/12/18/24 dB/oct slopes, ±12 dB GEQ bands/master, 0..115.8 m alignment distance
(0..341.7 ms), and 0..40 °C. Distance raw metres agrees with the supplied native
observation (1.65895748 raw displayed as 1.7 m, 5.4 ft, 4.8 ms). Exact endpoint
and global temperature behavior across outputs still need native verification.
No delay/filter/GEQ descriptors are inferred onto the different Subgroup schema.

### Supplied native movement evidence

- Main GEQ enable4361; 20Hz4363, 630Hz4378, 20kHz4393. Corrected the previous
  band's one-word offset. Removed unproven GEQ master4360; do not substitute
  suspected4362 until a native movement proves it.
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

GEQ master is withheld; output meters/GR, measured PEQ response, linked-output
propagation, presets, GEQ draw/reset, mode-1 editing and shelf-Q semantics remain
incomplete. Only pushed values have readback authority; accepted writes retain
unconfirmed authority. Unknown/out-of-domain values stay unavailable. Mode,
expected value, per-word revision and connection-generation guards remain.

## Batched parent verification recipe

1. On Main and Aux1, capture one native enable/frequency/slope per filter and
   delay enable/distance, including 6/12/18/24 slopes and delay endpoints.
   Aux1 expected HPF4484/4485/4486, LPF4488/4489/4490, delay4510/4511.
   Establish raw metre scaling at two exact distances. Confirm temperature is
   one shared type2/7 parameter from both output views; probe 0/40 °C, checking
   distance stays fixed while displayed time changes. Identify GEQ master
   independently; do not write unknown4360/4362 through the product.
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
