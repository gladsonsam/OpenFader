# openmix agent rules (binding until the owner lifts them)

## Hard rule: NO frontend tests for now
- Do NOT write new frontend tests (`*.test.ts`, harness checks, interaction
  suites, screenshot-diff gates) and do NOT run frontend suites as acceptance
  gates. They are a time waste pre-MVP.
- MVP verification is: `npm run build` clean + app boots + connects
  (sim relay smoke). That is the whole frontend gate.
- Rust `cargo test --workspace` stays green (it guards the desk protocol);
  do not expand it without being asked.
- This rule lifts after the MVP exists, at the owner's word — not before.
