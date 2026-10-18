# Catalogue v2: everything the design needs the mixer to supply

Principle: the engine must be able to FETCH (and write) every value the UI shows, on every desk that has it, and report
what a desk lacks via `caps`. No UI value is ever invented. This document is the spec for Rust packages R1/R2
(see PLAN.md). Today's catalogue (crates/core/src/params.rs, 54 ids, channel scope only) covers fader, mute, name, gain,
phantom, usbreturn, trim, polarity, hpf_on/hpf_freq, pan, gate_on, comp_on, eq_on, eqN_{gain,freq,q,type}, aux/fx sends.

## 1. Topology (new): strip families, not just "channels"
The design's banks: Ch 1-16/17-32/33-48 (inputs), FxRet, Mix, Grp/FxSnd, Mtx/Main (Mtx 1-8, Main LR, Main C), DCA 1-16.
The engine keeps ONE global strip index space (as today) and adds `caps.topology`: an ordered list of families
`{ key, kind: in|ret|bus|fxsnd|mtx|main|dca, label, first, count, name_prefix, color_default, params_scope }` so the UI can
build banks for any desk (XR18: 16 in + aux/usb + 4 FX ret + 6 bus + 4 FX send + LR + 4 DCA; X32: 32 in + 8 aux + 8 fxret +
16 mix + 6 mtx + main + mono + 8 DCA; DL32S: 32 in + 14 mix + LR ... per driver). Every catalogue entry gains `scopes: [kind..]`
(which families it applies to) replacing the single `scope`. Poller/snapshot iterate families, not "channels".

## 2. New / extended ids (units are engineering units; ranges from the cited desk docs, never from the design)
Strip identity and look: `color` (enum 0..15, desk channel colour, used for the name bar), `icon` (int), `link` (bool, stereo link),
`source` (text/enum: input source label e.g. "Lcl 02", read-only unless routing supported).
Input/preamp: `gain`, `trim`, `phantom`, `polarity` (have), `delay_on` + `delay_ms` (X32; X-Air likely unsupported), `hpf_on`, `hpf_freq`
(have), `hpf_slope` (enum 12/18/24 dB/oct, X32-only).
EQ: have `eq_on`, `eqN_*` (N=1..4 + `eq5_*` where the desk has more); band types per desk (X-Air 6: LCut, LShv, PEQ, VEQ, HShv, HCut).
Gate (full): `gate_on`, `gate_thr` (dB), `gate_range` (dB), `gate_att` (ms), `gate_hold` (ms), `gate_rel` (ms), `gate_mode` (enum), `gate_key_src`, `gate_key_filter` (as the desk offers).
Compressor (full): `comp_on`, `comp_thr` (dB), `comp_ratio` (value as ratio number from the desk's step table: 1.1..100), `comp_knee`, `comp_makeup` (dB),
`comp_att` (ms), `comp_hold` (ms), `comp_rel` (ms), `comp_mode` (comp/exp), `comp_det` (peak/rms), `comp_env` (lin/log), `comp_mix` (%), `comp_auto` (bool).
Sends: `sendN_level` (dB, N up to the desk's mix count: replaces aux1..8/fx1..4 naming with generic `sendN_*` over topology bus+fxsnd families),
`sendN_tap` (enum: pre/post/...; the design shows Pre/Post pill), `sendN_pan` (when the bus is stereo), `sendN_on` (bool, mute of the send).
Buses/mains/DCA/FX returns: fader, mute, name, color, pan (where it exists), eq + dyn sets (design shows EQ/Dyn on every kind with eq:true),
DCA: fader, mute, name, color only.
FX racks: `fxN_type` (enum from core FxRack types), `fxN_on`, `fxN_source/ret routing`, per-slot params `fxN_p<idx>` described by core::fx metadata
(label, unit, range per type) delivered in `caps.fx` (types and their param tables), the existing `set_fx_param/get_fx_param` become engine commands (R2).
Meters (new "taps"): per strip `in` (pre-fader input level, for the Input tab level bar), `post` (post-fader, the mixer meters), `gr_gate`, `gr_comp`
(gain reduction); main LR L/R. Declared in `caps.meter_taps`; transport carries them (MR formats); poller NOT used for meters.

## 3. Engine-level capabilities beyond per-strip params (R2: Cmd/Event additions, JSON + golden fixtures + README)
- Devices/connection: `{"cmd":"scan"}` -> `{"type":"devices","list":[{id,name,model,ip,kind:"console|io",state:"available|connected|in_use",fw?}]}` using
  crates/transport discovery.rs (mDNS/OSC broadcast), `connect_device {id}`, `disconnect`, status `{"type":"link","state","name","ip","fs":48000,"latency_ms":1.2}`
  with a periodic ping measuring real round-trip to the desk (OSC query timing).
- Offline editing: `{"cmd":"offline"}` engine runs on the sim/offline model keyed to the desk model; changes recorded and PUSHED on connect (explicit confirm event listing what will change).
- Scenes: numbered library slots 001.. with names; `scenes` event (list), `scene_save {n,name}`, `scene_recall {n, confirm?}`; capture/recall covers the FULL param plane
  cache (all catalogue values, faders with the existing fade), not only fader/mute; desk-native snapshots only if the desk driver supports them (otherwise engine-side scenes).
- Routing/patch: `caps.routing {groups:[{key:"Lcl",count},{key:"Net"},{key:"USB"}], per_strip:true}` and params `source` set/get when supported (XR18 insrc is live-silent on FW 1.20: report unsupported, sim supports).
- FX: `fx_set {slot,type}`, `fx_param {slot,idx,value}` and `fx` events (state of all slots), `fx_bypass`.
- Info: model, firmware, sample rate, console name for the Connection and About panels (`info` event).

## 4. Rules
Every new OSC address: cite the document it comes from in a doc comment (X-Air OSC doc, X32 OSC doc, xair-api, existing repo captures) and mark UNVERIFIED-ON-RIG;
extend docs/xair-protocol.md tables and add checklist items to docs/LIVE-RIG.md. The sim implements the ENTIRE v2 catalogue/topology/FX/routing/scenes/devices with realistic
state so the design runs end to end in development. Unsupported on a given desk = not in `caps` (UI removes/collapses; never fakes). Ranges ALWAYS come from the catalogue via `caps`.
Poller budget unchanged (20 q/s, channel-major + focus); with ~100 ids per strip the cold full sweep is long, so: poll ONLY families/strips that are visible or focused first,
slow background sweep for the rest, and prefer desk-side bulk reads where the protocol offers them (OSC /node subtree reads on X32/X-Air if documented; mark UNVERIFIED).

## 5. Known engine defects to fix in the same Rust wave (found in review)
- Focus queue (engine): repeated page loads flood it (each load queues ~17 channels x 54 params); the relay retains and appends, so the NEWEST focus
  drains last, and an uncached channel showed no prioritization within 12 s. Required: de-duplicate pending (channel,param) pairs, serve most-recent-first
  (a new focus supersedes older ones), cap the queue, and drop a client's pending focus work when it detaches; add tests (flood with 30 focus commands, then
  focus channel N: its params arrive first within ~3 s at the default budget).
- Sim: some channel slots (e.g. ch30) returned no params in one test; every catalogue id must answer for every strip the topology declares.
