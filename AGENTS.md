# openmix agent rules (binding until the owner lifts them)

## Live desk testing (owner rig — details are private)
- The owner's rig (desk IP, write grant, leave-sane rules) lives in the
  private `openmix-docs-private` repo (`RIG.md`). Never commit rig details
  (IPs, hostnames, desk names) to this repo.

## Internal docs live in the private repo (not here)
- BACKLOG.md, ARCHITECTURE.md, CHANGELOG.md and `docs/` were purged from
  this repo's history for open-source cleanliness. They live in the private
  `openmix-docs-private` repo.
- Do NOT recreate BACKLOG.md, ARCHITECTURE.md, CHANGELOG.md or `docs/` here.
  BACKLOG.md is gitignored on purpose.

## Agent notes: Flutter dev loop (agent-maintained)
- Local dev scripts (iOS sim, macOS, real-desk relay) live in the private
  repo (`local-scripts/`); copy into `scripts/` to use (gitignored there).
- This machine has no git identity or HTTPS credentials configured; the
  exact commit/push commands live in the private repo (`RIG.md`). Never
  print tokens.
- Full simulator playbook (AXe commands, fader numbers, WKWebView gotchas):
  private `openmix-docs-private` repo (`docs/sim-verify.md`).

## Hard rule: NO frontend tests for now
- Do NOT write new frontend tests (widget tests, harness checks,
  interaction suites, screenshot-diff gates) and do NOT run frontend suites
  as acceptance gates. They are a time waste pre-MVP.
- MVP verification is: `flutter analyze` clean + app boots + connects
  (sim smoke). That is the whole frontend gate.
- Rust `cargo test --workspace` stays green (it guards the desk protocol);
  do not expand it without being asked.
- This rule lifts after the MVP exists, at the owner's word — not before.
