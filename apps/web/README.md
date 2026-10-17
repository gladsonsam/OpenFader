# openmix-web — Svelte mixer UI (static build)

Svelte 5 port of the `../../ui/` vanilla mixer UI. Same screens, same
backend contract (`src/lib/backend.js` mirrors `../../ui/api.js`):
`TauriBackend` + `WsBackend`, same WebSocket JSON protocol on `:8080`
(`connect`/`fader`/`mute`/`param` out, `state`/`meters` in).

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

`TauriBackend` (selected automatically when `window.__TAURI__.core.invoke`
exists) calls the same IPC commands as the vanilla UI: `connect`,
`set_fader`, `set_mute`, `get_meters`, `set_param` (rejected controls
hide themselves). To load this UI in the Tauri shell instead of `ui/`,
point the webview at `apps/web/dist/index.html` (with `base: './'` the
built assets resolve from any location, including `tauri://localhost`).

## layout

- `src/lib/backend.ts` — transports (contract port, keep in sync with `ui/api.js`)
- `src/lib/levels.ts` — shared mutable meter frames (not reactive: no re-render at 60fps)
- `src/lib/meters.ts` — imperative canvas painting (staging strips + bridge)
- `src/lib/sections.ts` — detail-panel control defs (ranges mirror `proto-dl32s` limits)
- `src/lib/format.ts` — fader→dB readout
- `src/lib/persist.ts` — localStorage prefs (layers, orders, mute groups, safety)
- `src/lib/sends.ts` — SoF send cache, pushed via existing `param` cmd (`auxN_level`)
- `src/components/Strip.svelte` — name, staging meter, touch fader (pointer capture,
  fine mode, Grow&Glow, exact-entry), mute/solo, clip latch, layout handles
- `src/components/Overview.svelte` — whole-show health rows, tap-to-jump
- `src/components/ValueEntry.svelte` — tap-a-value exact-entry modal
- `src/components/Detail.svelte` — gain/phantom/pan, HPF/gate/comp, 4-band EQ, aux/FX sends
- `src/components/Spectrum.svelte` — per-channel spectrum (fallback until relay sends it)
- `src/App.svelte` — views (Overview/Mixer/Channel), aux rail + SoF, layers, groups,
  safety, scenes, bridge, keyboard, rAF loop

See `V2.md` for what the v2 structural pass shipped vs what stays deferred to DSP.
