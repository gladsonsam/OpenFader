# OpenFader WING provider

Documentation/schema-based WING integration. **No physical WING has been used
for verification. This is not a claim of complete WING feature parity.**

The provider is registered as `wing` in the embedded engine and CLI. Automatic
and explicit WING discovery/manual connection use the same provider registry as
other consoles. The Flutter Connections screen includes Behringer WING.

## Module layout

`lib.rs` owns only the driver state and module wiring. Session lifecycle and
meter subscriptions live in `lifecycle.rs`; discovery in `discovery.rs`;
source/preamp routing in `routing.rs`; guarded writes in `control.rs`; and the
shared `MixerDriver` adapter in `driver.rs`. Capabilities/ranges/colors live
in `catalogue.rs`, with conditional UI metadata in `presentation.rs`.

DSP behavior has dedicated `delay`, `filters`, `fx`, `groups`, `sends` and
`source` modules; native definition parsing/querying is in `native.rs`, and
wire conversions/topology in `mapping.rs`. The large FX table under
`src/schema/` is generated data with a reproducible importer, not a driver
implementation. Engine-only capability projection is isolated in
`crates/engine/src/drivers/wing.rs`. Protocol tests are grouped by feature
under `tests/protocol/` with one shared loopback fixture.

## Reference material

- [WING Remote Protocols supplied for this task](https://stage-one.co.za/wp-content/uploads/2026/09/BEHWINGCOMPACT-WING-Remote-Protocols.pdf).
  That host returned HTTP 403 to the local downloader, and the browser fetch
  exceeded its size limit. Implementation uses the **V3.0.5-1** copy in libwing.
- [libwing protocol manual](https://github.com/dannyfiresnake/libwing/blob/74c2ff649aabcc62a788fcf034c41d7d82e40b3c/Wing-Remote-Protocols.pdf),
  especially pp.18–28 (OSC), 41–58 (channel/output controls), 82–87 (native
  framing/meters), and 190 (color identities).
- [libwing discovery specification](https://github.com/dannyfiresnake/libwing/blob/74c2ff649aabcc62a788fcf034c41d7d82e40b3c/Discovery.md).
- [FX schema provenance and license](src/schema/README.md).
- [Independent recorded hardware observations of comma-separated group tags](https://github.com/bawaaaaah/wing-mcp-server/blob/315ad992c795f83d2cc1f2be3a6a1f41a76ba74a/src/plugins/wing/wing-group-tags.ts).
  Used to cross-check tag delimiters, exact token boundaries and the `#M1`–`#M8`
  mute-membership tokens (undocumented in the manual copy), not as a
  claim that OpenFader itself has passed hardware testing.

## Implemented surfaces

| Surface | Implementation and limits |
| --- | --- |
| Discovery | Broadcast and directed `WING?` on UDP 2222, strict CSV parsing, console names containing spaces. OSC `/?` validates identity at connect. |
| Control session | UDP 2223, bounded queries and real desk readback. Polling avoids taking the desk's single global OSC event subscription from another application. |
| Live parameter definitions | Read-only native `DD` requests in OSC blobs, exact child-name selection, short/extended record lengths, numeric domains/steps, units, enum choices and read-only flags. Typed reply matching keeps ordinary values and definition blobs separate. Delay controls use these live domains; no fixed distance/time conversion is inferred. |
| Topology | 40 channels, 8 aux inputs, 16 buses, 4 mains, 8 matrices, 16 DCAs. All 92 strips have distinct stable keys. |
| Strip controls | Fader, mute, solo, name, 12 native color identities; pan on audio strips. Clear strip solos. DCA names are limited to 8 bytes; channel names allow 16. Source-linked effective names/colors still need hardware assessment. |
| Input controls | Trim, polarity, main/alternate source selection, selected physical Local/AES50 preamp gain and phantom. Physical targets are pinned and checked against captured source context for guarded edits. |
| Source mode / stereo | Read-only effective input M/ST/M-S mode; editable shared external-source modes use source-tagged values, live definition read-only checks and actual readback. The UI identifies the shared port and binds confirmation to its old mode, strip and session. Automatic alternate-source switching and -9..+9 dB input balance apply to channels/auxes; -150..+150% width applies to every audio family. Oscillator mode is read-only; internal bus/main/matrix sources expose the effective channel mode. |
| Channel input filters | Low cut on/frequency and 6/12/18/24 dB/oct slopes; high cut on/frequency and 6/12 dB/oct slopes. Tool-filter enable/model plus TILT tilt, MAX low contour/high process, AP1 frequency, AP2 frequency/Q. Model-specific reads/writes are guarded. The native `flt` node exists on channels, not aux/output strips. Controls remain independent of EQ bypass; the EQ graph explicitly excludes their unverified response. |
| Delay | Input/aux and bus/main/matrix enable, native unit selection (metres/feet/ms/samples) and amount controls. Coherent snapshots bind the displayed amount to its unit, live numeric range/steps and read-only flag. Amount writes carry a unit tag, recheck the unit around definition reads and require real desk readback. Local scenes keep the tag, restore unit before amount and enable last. Stale controls/queued writes are bound to strip/session. |
| Input patching | Main and alternate patches, Off, Local/Aux, AES50 A/B/C, StageConnect, USB, card/module, player, AES, user signals, oscillator, buses/mains/matrices. One node command sets group/index; readback must match. Physical port counts vary by model. |
| Sends | 28 destinations: 16 buses, 8 matrices, 4 mains. Input/aux bus and matrix sends have level/on/pan and PRE/POST/GRP mode. Bus-to-bus and bus/main-to-matrix sends have level/on and their native pre-fader switch. Input/aux/bus main assignments have individual level/on/pre controls. The channel pre-tap selector is exposed separately. Self bus sends are rejected and hidden. |
| DCA assignments | All 16 reserved `#D<n>` memberships, preserving unrelated tags and enforcing the native 80-byte tag bound. |
| Mute groups | All 8 masters and membership for all 76 audio strips via reserved `#M<n>` tags. Serialized edits share the DCA tag path, preserve other tags and require exact desk readback. Existing `_mg1`–`_mg8` engine controls and local scene capture carry membership. |
| EQ | STD six-band input/aux EQ and eight-band bus/main/matrix EQ, gain/frequency/Q and PEQ/shelf edge types. Model identity and selection use provider choices; alternate plugin parameters are intentionally withheld from the STD controls. |
| Dynamics | Standard GATE and COMP controls, documented ratio choices, makeup/mix, detector/envelope/auto, and processor model selectors. Nonstandard model parameter layouts are not aliased to standard layouts. Aux PSE/LA only has its on/off switch through the current shared controls. |
| FX | 16 slots; 65 model definitions and model-specific parameter conversions, names, units and discrete choices. Premium algorithms are limited to slots 1–8. Constant properties are omitted. FX on/bypass is not advertised because WING has different insertion/state semantics. |
| Meters | Native TCP 2222 requests plus UDP meter reports. Escaped channel framing, signed big-endian 1/256 dB values, stereo peaks, pre/post levels, report/length/source validation, renewal and cleanup. Separate collections prevent unsupported DCA reports from suppressing channel meters. Stale collections clear. GR/RTA/FX meters are not advertised. |
| Local library | Canonical scene/fader plane uses existing engine workflows; main/alternate input IDs can be restored and are ordered before preamp writes. Derived metadata is omitted from scene writes. Shared source-mode capture uses fresh native read-only checks; preamp recall carries stored main/alternate source context and skips mismatched targets. Rejected hardware parameter writes emit a bounded error summary and no accepted value echo. Scoped API recall separates all 16 bus sends (`sends_bus`), 8 matrix sends (`sends_matrix`) and 4 main sends (`sends_main`); source patches and every numbered EQ band use their corresponding groups. This is not native WING snapshot/show support. |

Faders in the OpenFader protocol use a documented linear representation of
`-144..+10 dB`. Wire writes always use engineering dB; the WING reply's middle
normalized argument is never mistaken for its engineering value. The matching
Flutter law leaves existing desks' laws unchanged. Send minimum values come
from provider metadata, so the bottom position sends WING's silence floor.

## Verification

`tests/protocol.rs` uses a UDP desk fixture that follows the manual's query
triplets and does not echo writes. It exercises external changes, model guards,
physical-preamp targeting, stale source context, input patch node grammar,
FX conversions across every advertised model, all topology boundaries/stable
keys, tag edits, scope/metadata consistency, malformed data and silent peers.
Mute-group fixtures cover all five audio families, concurrent DCA/mute tag edits,
native tag limits and ignored writes that must fail rather than confirm membership.
Matrix/main send fixtures check native paths and distinguish input send modes
from output pre switches. Engine capability checks verify destination keys,
source-family restrictions and native level bounds. Send numeric entry keeps
the native dB value even below the slider's visual floor.
Engine loopback recall checks exercise the actual UDP writes and emitted
parameter events for bus/matrix/main-only scopes; an FX-send-only scope must
write none of those destinations. The same checks require zero excluded-axis
writes during sends-only, mute-only and fader-only recalls. Provider group overrides also cover source
patches and numbered EQ bands, while legacy desks keep their prior grouping.
Transport tests cover native framing/escaping, lengths, report IDs, dB units,
UDP reports and TCP/task cleanup. These establish implementation behavior,
not hardware interoperability.
`tests/native.rs` anchors node-definition decoding to the manual's published
DAW ENABLE/CONNECTION bytes, covers every documented definition kind and
rejects every truncation of a complete mixed reply. Live loopback queries
check exact child selection, duplicate/missing names and ordinary node replies
interleaved at the same OSC address. Transport checks also cover delayed blobs,
wrong reply types, timeout cleanup and failed sends.

Run the repository gates from the root:

```sh
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Latest local verification (2026-10-26): **443 workspace tests passed**, with
3 existing live-rig tests ignored; strict workspace Clippy and Flutter analysis
were clean. The WING provider's 26 protocol tests and 5 native-definition tests use local
fixtures and published manual bytes. Four delay checks additionally cover all
five audio strip families and all four units, live range disagreement with
static tables, stale units/scene values, read-only metadata, wrong types and
ignored writes. The UI uses existing input controls and the shell poll loop;
no frontend test suites or new page timers were added.
Two channel-filter checks additionally cover the native cut slopes/ranges,
channel-only scopes, EQ-bypass independence, all four tool models, real wire
names without synthetic model segments, invalid choices and stale-model guards.
The EQ numeric editor follows the provider's six/eight-band count and rejects
stale strip/connection targets; numeric edits queued before a connection change
are discarded.
Six source checks cover external source groups, read-only oscillator modes,
main/alternate port targeting, reroutes during schema reads, old-mode conflicts,
ignored writes, read-only scene omission, effective-mode protection, balance,
width and explicit preamp context while auto switching is active. Engine
loopback recall additionally proves stored source context reaches the physical
preamp write; mismatches are reported without accepted value echoes. Full
scene capture spans multiple reads/cache samples, not an atomic desk snapshot.

Run `flutter analyze` in `app` with Flutter 3.47.7. No frontend test suites
were added. iPad Pro 13-inch and iPhone 18 Pro boot/connection/screenshots
remain required; this Linux environment cannot perform them. Linux GUI build
also needs CMake and the normal Flutter desktop development prerequisites.

## Work remaining before full compatibility

- Alternative gate/dynamics/EQ plugin parameter editors, auxiliary PSE/LA
  controls, full output EQ cut/filter types and their correct response curves.
- Verified combined response/phase display for native input cut/tool filters;
  channel processing order and pre/post inserts. Confirm live delay definitions, unit changes and
  quantization against real firmware; physical desk edits can still race the
  final OSC write because the protocol supplies no compare-and-set operation.
- Output patching, user signals, send pan-link/pre-always-on behavior,
  and the remaining source controls.
- WING Rack's additional four headphone main paths and model/firmware-specific
  topology, preamp ranges and valid source counts.
- Source-linked identity behavior, native icons and named group metadata.
  Confirm the independently recorded mute-group tag semantics on real hardware.
- Gain-reduction, RTA, stereo/main and FX telemetry beyond the current peak
  projection; confirm whether all 16 DCA meters exist (the manual's meter table
  lists 8 while its control table lists 16).
- Native snapshot/show/file operations, safes/recall scope, monitor/solo
  configuration, playback/recording, cards/clock and other system functions.
- Real hardware and both required Apple simulator checks. Firmware/plugin
  changes can alter schema; the frozen FX table must not be treated as a
  universal future-firmware contract.

Keep this list explicit. Do not label the integration fully compatible while
these surfaces remain absent or unverified.
