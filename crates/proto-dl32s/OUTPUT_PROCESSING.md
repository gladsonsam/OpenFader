# Output processing implementation and verification handoff

This module is **inferred, not hardware verified**. It adds a working path for
645 controls on Main, Aux 1–8 and Subgroup 1–6 through the existing destination
control contract. No hardware connection or writes were made during implementation.
It is not a claim of full Master Fader parity.

## Module boundaries

- `src/output_processing/map.rs`: output inventory, inferred addresses, encoding,
  control domains and processor-mode dependencies. No socket or UI logic.
- `src/output_processing.rs`: push-only hydration, per-word revisions, names,
  descriptor publication and generation/expected-value guarded writes.
- `src/lib.rs`: seven small integration points: module/cache member, descriptor
  extension, write dispatch, connect seed, value ingest and name ingest.
- `app/lib/core/output_processing.dart`: constructor-injected generic screen
  contract; existing destination commands and epoch checks remain unchanged.
- `app/lib/screens/output_processing.dart`: provider-derived output/processor
  selection, enable/choice controls, numeric editors, release-to-commit sliders,
  horizontally scrolling 31-band GEQ, stale/pending/error handling.
- `app/lib/screens/output_processing_graph.dart`: observed PEQ band positions,
  GEQ band gains and compressor transfer reference. PEQ is deliberately labelled
  band positions rather than claiming a filter-response curve. Compressor plot
  is a hard-knee static reference, not measured dynamics or soft-knee telemetry.
- `AppStore`, mixer controller/page, shell router: selection/navigation hooks.
  Output selection follows the matching generic master bus. Existing input
  navigation clears output selection, including reopening the same input.

There are no frontend provider/model branches, new polling loops, new canonical
input capabilities, strip-count changes, fader-law changes or mutable colors.
Output inventory uses destination IDs, independently of pollable input StripIds.

## Address geometry and provenance

Source: `/tmp/dl32s-five-fx-before-20261026.json` (5257 raw words), existing
`addrs.rs` output block bases/strides, input compressor/EQ quartet layouts,
private `dl32s-protocol.md`, and official Master Fader 5.1 guide pp131,139,142–146.
A raw domain/overlap audit found all 645 proposed values in domain and all
645 addresses distinct. This checks consistency, **not address semantics**.

Define DSP base **D**:

| Output | D | PEQ mode E |
|---|---:|---:|
| Main | 4352 | D + 50 |
| Aux n (1–8) | 4442 + (n−1)×90 | D + 50 |
| Subgroup n (1–6) | 4048 + (n−1)×51 | D + 8 |

All writes use type-1 parameter words (`VALUE_TYPE_PARAM`, not meter/type-2).

| Control | Address | Encoding / domain |
|---|---|---|
| Compressor internal mode (read guard only) | D | integer; numeric/knee descriptors require 0 |
| Compressor enable | D+1 | integer 0/1 |
| Makeup, threshold | D+2, D+3 | float32; 0..20, −80..0 dB |
| Attack, release | D+4, D+5 | float32; 0.1..300, 50..3000 ms |
| Ratio | D+6 | float32 1..101; inferred input-like 101 = infinity |
| Knee | D+7 | integer 0 soft / 1 hard |
| PEQ internal mode (read guard only) | E | integer; band descriptors require 0 |
| PEQ enable | E+1 | integer 0/1 |
| Band b quartet (b=1..4) | E+2+(4−b)×4 | type, gain, frequency, Q |
| Band 1/4 shape | quartet+0 | integers 2 bell / 3 low shelf / 4 high shelf, inferred from input mapping |
| Band gain/frequency/Q | quartet+1/+2/+3 | float32; ±15 dB / 20..20000 Hz / 0.5..16 |
| Main/Aux GEQ master | D+8 | float32 ±12 dB |
| Main/Aux GEQ enable | D+9 | integer 0/1 |
| Main/Aux GEQ band k (1..31) | D+9+k | float32 ±12 dB; ascending standard 20 Hz..20 kHz |
| Main/Aux HPF/LPF frequency | D+43, D+47 | float32 20..20000 Hz |

The GEQ master/enable ordering and band order, output knee/type enum meanings,
compressor infinity sentinel and all output write semantics still need native
movement/readback evidence. Current raw values are genuine pushed observations;
`verified` authority refers to readback provenance, not proof of these mappings.
Own accepted writes are published as `accepted`, never upgraded to readback.
Mode changes invalidate dependent cached values; mode 1 is not exposed as a
selector or label. Values stay unavailable until the relevant pushed data returns.

## Deliberately remaining gaps

- Alignment delay: guide p146 gives 0..341.7 ms and enable/temperature controls,
  but no distinguishing baseline signature establishes their word positions.
  Do not repurpose zero-valued tail words without a native movement capture.
- HPF/LPF enable and slope: neighboring zero words may be mode/enable fields or
  unused gain words in a filter quartet. Frequencies have clear 20/20000 anchors;
  enable/slope enums remain unresolved. No speculative switch is published.
- Output meters/GR, measured PEQ response, linked-output processing propagation,
  presets, GEQ draw/reset, mode-1 editing and shelf-Q semantics are incomplete.
- No simulator/native UI, Android, hardware or visual layout verification ran.
  Offline providers without output descriptors correctly show no output entry.

## Bounded parent verification batch

Use the parent's serialized hardware process, with a fresh complete baseline,
quiet/muted output policy, independent native observation and peer capture.
Restore the **exact original raw word after each probe**, then require complete
5257/107/75 equality before the next probe. Do not leave compressors, filters or
EQ gains altered. Do not infer hardware proof from an accepted local write.

1. Establish each schema on Main, Aux1, Subgroup1 using native movements first:
   compressor enable/threshold/attack/ratio/knee (Main 4353/4355/4356/4358/4359;
   Aux1 add90; Sub1 4049/4051/4052/4054/4055), and PEQ enable + band1
   type/gain/frequency/Q (Main 4403,4416–4419; Aux1 add90;
   Sub1 4057,4070–4073). Probe one control at a time and restore exactly.
2. Main/Aux1 GEQ: native master + enable and 20Hz/630Hz/20kHz movements
   (Main 4360/4361/4362/4377/4392; Aux1 add90). Confirm order and units.
   Native HPF/LPF frequency movements should isolate Main4395/4399 and
   Aux1 4485/4489. Capture enable/slope and delay native movements in the same
   session to resolve remaining unknown words without product guesses.
3. Once each schema is established, cover equivalent geometry in a batch:
   Aux2–8 one PEQ frequency and one compressor threshold each; Sub2–6 one
   PEQ gain and one threshold each; terminal GEQ bands on Aux8. Read back and
   restore every probe. This bounds the stride checks without manually
   rediscovering each equivalent slot.
4. Product on iPad and iPhone: open selected Main/Aux/Subgroup processing from
   the master EQ preview; verify output picker tracks the master bus, all
   processor tabs/31 bands/precise editors fit, input reopening retains its
   existing behavior, then do representative product→native→peer→exact-restore
   loops for the established schema. Exercise peer changes while an editor is
   open/during a drag, disconnect, reconnect and invalid/out-of-range commands.
   Require stale edits rejected and uncertain failed writes unavailable.
5. Parent final integration gates: `flutter analyze` in app,
   `cargo test --workspace --locked`, normal workspace Clippy/build gates,
   both iOS boot/connect/layout checks and serialized hardware verification.

## Local validation

- `cargo check -p openfader-proto-dl32s --locked`: passed.
- `cargo clippy -p openfader-proto-dl32s --locked -- -D warnings`: passed.
- `cargo test -p openfader-proto-dl32s --lib --locked`: 18 existing tests passed;
  no Rust test cases were added.
- `flutter analyze --no-pub` from `app`: no issues.
- Baseline domain/duplicate-address consistency audit: 645/645 in range,
  645 unique addresses across 15 output targets. Not a semantic/wire proof.
- No frontend tests, full workspace suites, app builds, native/device control,
  hardware connections, pushes or PRs were performed.
