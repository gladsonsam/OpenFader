# design-diff harness (package D1)

Automated, deterministic visual regression for the ui=4 screens against the
frozen design. Compares our fixture-mode pages to reference captures of
`docs/design/mixing-station-mixer-eq.html` at 1280x720, DPR 1.

## Usage

```sh
node apps/web/tools/design-diff/run.mjs --screen <id> [--url-base http://127.0.0.1:5291] [--threshold 1.0] [--delta 24]
node apps/web/tools/design-diff/run.mjs --all
node apps/web/tools/design-diff/run.mjs --screen <id> --update-ref   # regenerate ref + mask + boxes for one screen
node apps/web/tools/design-diff/run.mjs --update-ref --all           # regenerate everything
node apps/web/tools/design-diff/run.mjs --selftest                   # prove the metric works
```

Screen ids: `mixer ch-eq ch-comp ch-gate ch-sends ch-input fx routing
connection settings scenes mixer-locked mixer-sof mixer-fine
mixer-bank-mix mixer-bank-dca mixer-bank-mtx`.

Per-screen output lands in `/tmp/ui4-diff/<id>/`: `ours.png`, `diff.png`
(ours with mismatching unmasked pixels painted red), `heat.png` (per-8x8-cell
mismatch fractions, yellow->red), `compare.png` (ref | ours | diff
side-by-side). Stdout carries one JSON summary line per screen, e.g.

```json
{"screen":"mixer","mode":"diff","mismatchPct":0.42,"threshold":1,"pass":true,...}
```

Exit code is 0 when every screen passes (metric <= threshold AND every box
within 1 px), 1 on mismatch, 2 on harness error.

## How it works

1. **Reference capture.** A fresh page loads the design over
   `http://127.0.0.1:5301/` (started by the tool itself if it is down),
   viewport 1280x720, DPR 1, reduced motion, fonts awaited. The design
   animates meters on an 80 ms `setInterval` (`stepMeters`); an
   `addInitScript` neutralises exactly that timer so levels freeze at -inf
   and two consecutive captures are pixel-identical (the tool asserts this on
   every `--update-ref`). Navigation uses real clicks resolved by accessible
   name / visible text (see `screens.mjs`), e.g. `Open EQ for Ch 2`, then the
   `Compressor` tab, or `Settings` > `Screen lock` > `On` > `Back`.
2. **Our capture.** Loads
   `<url-base>/?ui=4&fixture=1&freeze=1&screen=<id>` (+ `&ch=2`,
   `&bank=…`, `&locked=1`, `&sof=1`, `&fine=1` per screen) with the same
   viewport, waits for fonts and for `window.__uiReady === true` (warns and
   captures anyway after 8 s). If our screen does not exist yet, whatever
   renders is captured and compared — mismatch will be huge, and nothing
   crashes.
3. **Diff** (`diff.py`, run as `python3 -I`; the script re-adds the user
   site-packages so Pillow/numpy import under `-I`). Metric: % of unmasked
   pixels whose max channel delta exceeds `--delta` (default 24).
4. **Box check.** `docs/design/refs/<id>.boxes.json` lists key elements with
   a matcher and the design-side rect. At diff time the same matchers run
   against our page and each of x/y/w/h must agree within 1 px. Missing
   element = failed box. Our pages must expose the same aria-labels
   (`Mute Ch 1`, `Fader Ch 1 …`, `Open EQ for Ch 2`, `Master strip`, …).

## Masks and boxes: authoring, reading, updating

- **Masks** (`docs/design/refs/<id>.mask.json`, `{rects:[[x,y,w,h]…],
  coveragePct}`) cover animated regions: strip meters, level bars, GR,
  transfer-curve level dots, input level bar. Generated semi-automatically:
  five live (non-neutralised) captures ~1.2 s apart, union of consecutive
  pairwise diffs (sensitivity 8/255), dilated and merged to rectangles, with
  a `mask-preview.png` (cyan outlines) written next to the run output for
  review. Static screens (routing/connection/settings/scenes) legitimately
  have empty masks.
- **Reading the heatmap.** `heat.png` is a 4x-upscaled 160x90 grid (one cell
  per 8x8 px): dark = clean, yellow->red = fraction of bad pixels in that
  cell. The JSON `worstCells` lists the top 12. `diff.png` shows exactly
  which pixels differ; `compare.png` puts ref/ours/diff side by side.
- **Updating refs.** Design data is seeded, so refs are stable; only
  re-run `--update-ref` when the design itself changes (it reports
  `stableRef`, mask rect count/coverage, and any unresolvable boxes — fix
  `screens.mjs` matchers if boxes go missing). Commit the updated
  `docs/design/refs/` files.

## Selftest

`--selftest` diffs the design against itself (must be exactly 0.00%),
diffs it against a copy shifted 2 px (must fail at ~9%), and re-measures the
mixer boxes against the committed `boxes.json` (must all pass).

## Notes / limits

- Playwright comes from the machine's standalone install
  (`/tmp/vantyr-pw/node_modules/playwright-core`, Chromium under
  `~/.cache/ms-playwright`); nothing is installed. One browser at a time.
- The design's `SENDS MASTER` tag (`msTag`) is computed but never rendered;
  SOF state is verified via the `Aux bus` sub-label and `Mute Mix 1` instead.
- Fine mode has no visual difference from the default mixer (faders move
  less per drag); its ref is kept as a separate screen for URL coverage.
- Current `?ui=4` serves the old app / in-progress screens (no
  `window.__uiReady`), so every screen currently fails by a wide margin —
  that is the expected starting point, not a harness bug.
