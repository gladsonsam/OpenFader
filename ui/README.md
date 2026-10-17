# openmix UI — manual checklist (v1)

Serve: `python3 -m http.server 8000` in `ui/` → http://localhost:8000
(or `npx serve`). Tauri serves the same files via `tauri.conf.json`
`frontendDist`. WS relay target: same host `:8080`, override with
`?server=host:port` or a full `ws(s)://…` URL.

1. **Connect sim**: load page → badge flips `offline → online`,
   status says `connected via …`; 8 `CHxx` strips appear.
2. **Drag fader**: touch/click-drag the fader track on ch1 → fill,
   handle and dB readout follow; meter canvas dances (sim meters).
3. **Mute**: tap `M` → button glows red, meter drops to 0. Tap `S`
   → strip solos (others dim; local monitor solo in v1).
4. **Detail**: tap a scribble → right panel shows Input / Filter /
   EQ / Aux / FX sections. Over sim/Tauri-stub every `set_param`
   fails, so controls disappear one by one as you touch them —
   that is the "hide unsupported" behaviour. Over a real WS relay
   they stay and drive the mixer (dB/Hz engineering units).
5. **Scenes**: tap `store` (arms, glows) → tap `S1` (status:
   `stored`). Move faders → tap `S1` again → faders jump back.
   Slots persist in `localStorage`.
6. **Layers**: Inputs shows all; Buses shows `BUS*` strips when the
   backend reports them (sim 32+8 over WS), else a hint; Mains is a
   local monitor-dim until the backend exposes a main fader.
7. **Bridge**: `bridge` button toggles the top meter-bridge canvas.
   (No FFT source exists yet, so true RTA is deferred — bridge only.)
8. **Keyboard**: select a strip, `↑/↓` nudges ±0.01 (`shift`: ±0.05),
   `M` mutes. Typing in the name field is not stolen by hotkeys.
9. **Offline**: kill the relay / go airplane → badge returns to
   `offline` within ~1.5 s (no meter frames); WS auto-reconnects.
10. **Driver switch**: pick `xair` + addr, connect → status shows the
    backend reply or the error text (blank addr is rejected by design).

Known v1 limits: Tauri shell exposes 8 channels and no `set_param`
command, so detail/EQ/sends only light up over the WS relay to a
full backend. Rename sends `param/name` and reverts on rejection.
