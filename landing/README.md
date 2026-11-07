# OpenFader website

Static site built with Astro: the landing page at `/` and the user docs at `/docs`.

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

## Re-taking docs screenshots

`screenshots/MANIFEST.md` lists every shot, how to reach it in the app, and the
exact commands to build the Linux app, run it headless and capture it with
`screenshots/cap.sh`.
