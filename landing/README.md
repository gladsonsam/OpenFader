# OpenFader website

Static site built with Astro: the landing page at `/` and the user docs at `/docs`.

The product tagline is **Control your mixer from any device**. Website copy
imports it from `src/brand.ts`; keep the repo README, package descriptions and
GitHub About consistent with it. Use direct, factual supporting copy.

```sh
cd landing
npm install
npm run dev      # local preview
npm run build    # static output in landing/dist/
```

## Layout

- `src/pages/index.astro` and `src/components/`: the landing page.
- `src/content/docs/*.md`: one Markdown file per docs page. Frontmatter sets
  the title, sidebar group and order.
- `src/layouts/DocsLayout.astro`, `src/styles/docs.css`: docs sidebar and page styles.
- `public/brand/`, `public/screenshots/`, `public/fonts/`: copies of files from
  `app/assets/`. If the logo, GIFs or fonts change there, copy them here again.
- `public/docs/shots/`: real app screenshots used in the docs.

## Writing docs

Every claim in the docs should match the app's code: exact on-screen labels,
real limits, and what depends on the connected mixer. Do not describe a
feature as working on a mixer unless it has been verified there.

## Screenshots and tablet previews

The hero uses `public/screenshots/ipad-2026-11-07/mixing-active.gif` with a
separate iPad frame. The native UI is 4:3; the frame is 2896 × 2208 with
its 2752 × 2064 screen at (72, 72). Keep these proportions when replacing media.
Reduced-motion users see the matching static raw capture. The tablet moves
slightly down as the Features section passes over it.

The feature carousel uses the five `*-framed.png` images from that directory.
Docs use the same framed captures, compact phone frames, and cropped dialog
images. Clicking a framed screenshot opens the original raw capture.
`src/lib/rehype-doc-shots.mjs` adds sizing and links at build time.

## Re-taking screenshots

`screenshots/MANIFEST.md` documents the native simulator capture workflow,
image dimensions, fixture build option and known limitations.
`screenshots/capture-ipad.py --docs` captures the supporting docs views;
`screenshots/frame-ipad.py` adds consistent bezels without stretching app pixels.
The published mapping is `public/screenshots/ipad-2026-11-07/docs-assets.json`.

Recorder has no current screenshot because the native offline iPad has no
computer recorder host. The FX-return bank and BUS1 sends captures are retained
as capture artifacts but omitted from docs: the bank is empty and the sends
capture shows bus EQ rather than sends on faders. Retake these views before
publishing them.
