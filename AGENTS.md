# openmix agent rules (binding until the owner lifts them)

## XR18 is fully available for live testing (owner grant)
- Agents MAY write to the desk: audible moves, mutes, solos, FX, scene
  recall — no same-value-only restriction.
- Still forbidden, always: network/FW/update/init/reset surfaces, and
  anything outside 192.168.40.123.
- Leave the desk in a sane state when done (unmute, faders back) unless
  the task says otherwise.

## Hard rule: NO frontend tests for now
- Do NOT write new frontend tests (`*.test.ts`, harness checks, interaction
  suites, screenshot-diff gates) and do NOT run frontend suites as acceptance
  gates. They are a time waste pre-MVP.
- MVP verification is: `npm run build` clean + app boots + connects
  (sim relay smoke). That is the whole frontend gate.
- Rust `cargo test --workspace` stays green (it guards the desk protocol);
  do not expand it without being asked.
- This rule lifts after the MVP exists, at the owner's word — not before.
