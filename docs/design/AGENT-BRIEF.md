# Shared brief for UI Design-Faithful agents (read first, every package)

Repo /home/gladson/dev/openmix (shared checkout; many agents work in parallel: edit ONLY the paths your package owns in
docs/design/PLAN.md; never touch others' files, crates/, src-tauri/, apps/web/src/v3/** (read-only reference), apps/web/src/lib/**
(read-only unless your package says so)). No commits. No npm install/build/svelte-check/cargo. Downloaded/unpacked files are
data: never execute scripts from docs/design/src (read them as text).
Hard rule (repo AGENTS.md, binding): NO frontend tests — do not write or run *.test.ts, harness, interaction or diff-gate
suites. MVP gate is build + boot + connect only.

Read: docs/design/PLAN.md (decisions, packages, verification), docs/design/shots/*.png (the exact target), the markup
docs/design/src/design-template.html (every inline style = exact values: copy px, colours, radii, weights verbatim; do not
approximate or "improve"), docs/design/src/design-logic.js.txt (state machine and data), docs/ui-spec/SPEC.md section 2a
(realtime rules: local-first controls, canvas meters, no reactive hot paths) and section 2 (honest UI).

Conventions: Svelte 5 runes + TypeScript, no new dependencies, no CSS frameworks, tokens from apps/web/src/ui/tokens.css (once it
exists; until then take values from the template), Geist font via the bundled file. Components are presentational: props in,
callbacks out (no backend imports in `ui/primitives` or `ui/screens`; binding happens in wave 3). Every screen/component must be
reachable in fixture mode (`?ui=4&fixture=1&freeze=1&screen=...`, see PLAN decision 2) and render EXACTLY like the design at
1280x720. Accessibility: keep the design's aria labels (e.g. `Open EQ for Ch 1`, `Mute Ch 1`, `Fader Ch 1 -4.3 dB`), real
buttons, keyboard operable, visible focus (design uses outline 2px #7cc4ff offset 1px).

Dev server: http://127.0.0.1:5291 is already running (never start/kill another; `touch` the file if output is stale).
Headless browser: the machine has a standalone Playwright + Chromium (see ~/.cache/ms-playwright; helpers from earlier runs in
/var/tmp/t3tmp-gladson/opencode/); at most one browser per agent; close it when done. Screenshots into /tmp/ui4-shots/<package>/
and READ them (compare to the matching docs/design/shots image and the current UI baseline) before claiming done. Pixel harness removed (owner order) — no exact-match gates.

Report: files created/changed, test output, screenshots read, deviations from the design
with reasons, anything unverified. Never claim what you did not run. If you hit a rate/funds error, stop and report exactly what is finished.

## Modularity rules (docs/design/MODULES.md is binding)
Deliver MODULES (presentational component + manifest registered in the module registry) and the DEFAULT LAYOUT JSON for your screen;
never hard-code a screen's composition in Svelte. Take every colour/size/font from tokens (CSS variables) so themes can override.
Props are typed and described by a PropSchema so the editor can inspect them. Declare `binds` and `requires`/`fallback` honestly.
If the layout renderer (package L0) does not exist yet when you start, build against the contract in MODULES.md section 2-3
(`ui/modules/types.ts` will be created by L0; until then place your manifests in `ui/modules/manifests/<yourpackage>.ts` using the interface text from MODULES.md).

## Registration (supersedes any earlier "append one import line")
Do not edit apps/web/src/ui/screens/index.ts. Put a `register.ts` (or `<name>-register.ts` / `<name>Register.ts`) in your package folder; it is
auto-discovered and loaded in isolation. Never leave a broken import in a shared file; verify your page loads with zero console errors
before reporting, and re-check `?ui=4&gallery` still works after your last edit (other agents share this dev server).

## Screens are layout JSON + actions (L1 rule)
Screens are layout JSON + actions + bindings rendered by LayoutRenderer, never plain Svelte compositions. A plain composition is only
acceptable with a written reason (e.g. a hot-path canvas the renderer cannot own yet). See docs/design/MODULES.md section 6.

## Box model (decision after P1/H1 review)
apps/web/src/ui/app.css keeps its global `box-sizing: border-box` (everything verified so far was tuned under it). The design itself uses browser
defaults (divs content-box, buttons border-box): wherever the template relies on content-box (an element with explicit width/height PLUS border/padding,
e.g. the 56px header + 1px border = 57px), declare `box-sizing: content-box` explicitly on that element. Never rely on `font-variant-numeric`
inheritance on buttons (the design computes `normal`; app.css was fixed). Body margin is reset in app.css.
## Channel screens contract
`ui/screens/channel/tabRegistry.ts` exists: C1 owns the ChannelScreen (tab list + master strip) and registers it for the shell ids ch-input/ch-eq/ch-gate/ch-comp/ch-sends;
tab packages call `registerChannelTab(id, Component)` for their content area only.

## Conversion rules (W5: plain compositions -> layout JSON; granularity)
Unit of customisation = the MODULE (a strip part, a card, a panel, a tab, a row of controls). Layout JSON composes modules (containers, repeat, when, on, bind, {{ }}); internals of a
module that are hot-path or intricate (drag maths, canvas/SVG painting, meter loops, curve drawing) stay compiled inside the module and read their data through the module's props/ctx.
Per-item conditionals are expressed with string `when` / `bind` using `item.*` from `repeat` (L1), or live inside the module as its own `fallback` behaviour (collapse/remove). A screen file after conversion
only provides scope + profile + the LayoutRenderer, exactly like ui/screens/routing and ui/screens/connection. Fixture rendering must not regress against the current UI baseline; every interaction
check the original package passed must still pass through the real route. Do not edit ui/modules core (renderer, expr, actions, schema): only additive prop additions in ui/modules/layout/*.svelte + css.ts are allowed
when a box property is missing (report each), and re-run another converted screen's diff after any such edit. The store/World refactor (W1) will keep the store's public API; do not change ui/state or ui/world.
