# openmix-web — mixer UI (static build)

Single UI: `src/ui/` (ui4, design-faithful). It is the default and only
route — `/` renders it, `?ui=4` is an alias. Fixture mode: `?ui=4&fixture=1&freeze=1&screen=<id>` (screens and
galleries listed in `src/ui/screens/index.ts`; current UI + screenshots
are the baseline, no pixel harness).

Prior UI versions (v1 vanilla `ui/`, v2/v3 Svelte trees) were removed at
the ui4 cutover; the UI lives entirely under `src/ui/` with its own
transport (`src/ui/world/transport.ts`) speaking the same engine
`Cmd`/`Event` JSON over WS (`:8080`) or Tauri IPC.

## dev

```sh
npm install
npm run dev      # hot-reload on http://localhost:5173
```

The UI auto-connects the `sim` driver, so it is usable with no hardware
and no relay server. For real hardware, run the Rust WS relay
(`crates/serve`) on the LAN and either open the dev server from a LAN
browser or pass `?server=192.168.1.10` (host or `host:port`; `ws://`
prefix optional).

## demo (static)

```sh
npm run build    # emits dist/ with index.html (no SSR)
npx serve dist/  # or: python3 -m http.server -d dist/
```

`dist/` and `node_modules/` are git-ignored (see repo root `.gitignore`).
Any static file server works — no backend required for the sim demo.

## Tauri wiring

`src-tauri/tauri.conf.json` `frontendDist` points at `apps/web/dist`.
With `base: './'` the built assets resolve from any location, including
`tauri://localhost`.

## layout

- `src/main.ts` — mounts `src/ui/` (sole entry)
- `src/ui/` — screens, shell, state, fixtures, world (engine transport),
  primitives, modules, layouts, gallery
- `tools/` — headless helpers (no pixel gates)
