<!-- Musician (monitor) mode — Mixing Station access-restriction parity.
//
// A performer picks their own wedge/IEM bus, then gets ONLY that bus:
// giant touch faders driving that bus's per-channel sends, big channel
// names, one big master (bus) volume, a CALL FOH flash button, and a
// single-tap exit. No LR, no mutes/solos, no layers, no scenes, no detail
// panel, no renames — everything else is hidden, not just disabled.
//
// Send writes reuse the existing sends-on-fader path (cached dB pushed via
// the `param` command as `auxN_level`); no protocol change. The bus master
// reuses the normal bus-fader path.
//
// CALL FOH: fires a best-effort `param` `attention` event on the bus strip,
// which the relay already broadcasts to every client as a `param` frame
// (see crates/serve param_tx fan-out) — plus a local flashing banner so
// the performer gets feedback even when FOH runs a build without a toast
// listener.
//
// TODO(kiosk hardening, follow-up): musician mode is PIN-less by design
// (entry and exit are one tap each), so a locked-down device still needs
// OS-level kiosk work outside this repo: guided-access / screen-pinning,
// no browser chrome or URL bar, and ideally a dedicated musician URL that
// boots straight into this mode. Also FOH-side: toast on `param`
// `attention` frames so the flash is seen at the desk, not just locally. -->

<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { Strip as StripData } from '../lib/backend';

  interface Props {
    /** Non-bus (input) strips — the only faders a musician ever sees. */
    inputs: StripData[];
    /** Bus strips — the pick-your-bus choices (wedges / IEMs). */
    buses: StripData[];
    /** Selected destination bus ch, or null when still picking. */
    busCh: number | null;
    /** Display name of the selected bus. */
    busName: string;
    /** Yamaha-style owner color of the selected bus. */
    busColorHex: string;
    /** Send fader 0..1 for (input ch -> selected bus). */
    sendValue: (ch: number) => number;
    /** Send readout (dB) for (input ch -> selected bus). */
    sendLabel: (ch: number) => string;
    /** Selected bus master fader 0..1 (the wedge volume). */
    masterValue: number;
    /** Selected bus master readout. */
    masterLabel: string;
    onPickBus: (bus: number | null) => void;
    /** One input send rode (channel -> selected bus). */
    onSend: (ch: number, v: number) => void;
    /** Bus master rode (the big wedge volume). */
    onMaster: (v: number) => void;
    /** Best-effort `attention` broadcast; the local flash always fires. */
    onCallFoh: () => void;
    /** Single-tap exit back to FOH (same PIN-less simplicity as entry). */
    onExit: () => void;
  }

  let {
    inputs,
    buses,
    busCh,
    busName,
    busColorHex,
    sendValue,
    sendLabel,
    masterValue,
    masterLabel,
    onPickBus,
    onSend,
    onMaster,
    onCallFoh,
    onExit,
  }: Props = $props();

  const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

  // One shared drag session (single touch at a time is fine on a wedge box).
  let dragCh: number | null = $state(null);
  let dragEl: HTMLElement | null = null;
  let dragStartRaw = 0;
  let dragStartVal = 0;

  function rawFromClientY(el: HTMLElement, clientY: number): number {
    const r = el.getBoundingClientRect();
    return clamp01(1 - (clientY - r.top) / r.height);
  }

  function down(ch: number, e: PointerEvent): void {
    const el = e.currentTarget as HTMLElement | null;
    if (!el) return;
    el.setPointerCapture(e.pointerId);
    dragCh = ch;
    dragEl = el;
    dragStartRaw = rawFromClientY(el, e.clientY);
    dragStartVal = sendValue(ch);
    onSend(ch, dragStartVal);
  }

  function move(ch: number, e: PointerEvent): void {
    if (dragCh !== ch || dragEl === null || !e.buttons) return;
    onSend(ch, clamp01(dragStartVal + (rawFromClientY(dragEl, e.clientY) - dragStartRaw)));
  }

  function up(): void {
    dragCh = null;
    dragEl = null;
  }

  function faderKey(ch: number, e: KeyboardEvent): void {
    const v = sendValue(ch);
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') {
      onSend(ch, clamp01(v + 0.02));
      e.preventDefault();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') {
      onSend(ch, clamp01(v - 0.02));
      e.preventDefault();
    } else if (e.key === 'Home') {
      onSend(ch, 1);
      e.preventDefault();
    } else if (e.key === 'End') {
      onSend(ch, 0);
      e.preventDefault();
    }
  }

  // CALL FOH flash: local strobe; auto-clears so it can't latch on.
  let calling: boolean = $state(false);
  let callTimer = 0;
  function callFoh(): void {
    onCallFoh();
    calling = true;
    if (callTimer) window.clearTimeout(callTimer);
    callTimer = window.setTimeout(() => {
      calling = false;
      callTimer = 0;
    }, 3000);
  }

  onDestroy(() => {
    if (callTimer) window.clearTimeout(callTimer);
  });
</script>

<div class="mus" style={busCh !== null ? `--mus: ${busColorHex}` : ''}>
  {#if busCh === null}
    <!-- Step 1: pick your wedge. Nothing else on screen. -->
    <div class="mus-pick">
      <h2>pick your mix</h2>
      {#if !buses.length}
        <div class="empty">no monitor buses on this backend yet</div>
      {:else}
        <div class="mus-buses">
          {#each buses as b (b.ch)}
            <button class="mus-bus" onclick={() => onPickBus(b.ch)} aria-label={`mix my ${b.name}`}>
              <b>{b.name}</b>
              <small>tap to mix</small>
            </button>
          {/each}
        </div>
      {/if}
      <button class="mus-exit" onclick={onExit} title="back to FOH (single tap, no PIN)">EXIT ✕</button>
    </div>
  {:else}
    <!-- Step 2: your mix only — sends to this bus + its master. -->
    {#if calling}
      <div class="mus-calling" role="alert">📣 CALLING FOH — hang on…</div>
    {/if}
    <div class="mus-head">
      <div class="mus-title">
        <small>YOUR MIX</small>
        <b>{busName}</b>
      </div>
      <button class="mus-foh" class:ringing={calling} onclick={callFoh} title="flash the FOH desk">CALL FOH</button>
      <button class="mus-exit" onclick={onExit} title="back to FOH / pick another mix (single tap, no PIN)">EXIT ✕</button>
    </div>

    <div class="mus-master">
      <label for="musvol">WEDGE VOLUME</label>
      <input
        id="musvol"
        type="range"
        min="0"
        max="100"
        step="1"
        value={Math.round(masterValue * 100)}
        oninput={(e) => onMaster(Number(e.currentTarget.value) / 100)}
        aria-label={`${busName} wedge volume`}
      />
      <b>{masterLabel}</b>
    </div>

    <div class="mus-strips">
      {#if !inputs.length}
        <div class="empty">no inputs on this backend yet</div>
      {:else}
        {#each inputs as s (s.ch)}
          {@const v = sendValue(s.ch)}
          {@const pct = Math.round(v * 100)}
          <div class="mus-strip" class:grab={dragCh === s.ch}>
            <div class="mus-name" title={s.name}>{s.name}</div>
            <div
              class="mus-track"
              onpointerdown={(e) => down(s.ch, e)}
              onpointermove={(e) => move(s.ch, e)}
              onpointerup={up}
              onpointercancel={up}
              role="slider"
              aria-label={`${s.name} in ${busName}`}
              aria-valuemin="0"
              aria-valuemax="100"
              aria-valuenow={pct}
              aria-valuetext={sendLabel(s.ch)}
              aria-orientation="vertical"
              onkeydown={(e) => faderKey(s.ch, e)}
              tabindex="0"
            >
              <div class="mus-fill" style="height: {pct}%"></div>
              <div class="mus-handle" style="bottom: calc({pct}% - 12px)"></div>
            </div>
            <div class="mus-db">{sendLabel(s.ch)}</div>
          </div>
        {/each}
      {/if}
    </div>
    <button class="mus-switch" onclick={() => onPickBus(null)} title="pick a different wedge / IEM mix">switch mix</button>
  {/if}
</div>

<style>
  .mus {
    --mus: var(--acc);
    display: flex; flex-direction: column; gap: 10px;
    padding: 12px 10px 20px; min-height: 60vh;
  }
  .mus-pick { display: flex; flex-direction: column; gap: 14px; max-width: 700px; margin: 0 auto; width: 100%; }
  .mus-pick h2 { margin: 6px 0 0; font-size: 28px; text-align: center; }
  .mus-buses { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; }
  .mus-bus {
    min-height: 96px; font-size: 18px; display: flex; flex-direction: column;
    gap: 4px; align-items: center; justify-content: center;
  }
  .mus-bus b { font-size: 24px; }
  .mus-bus small { color: var(--dim); }
  .mus-head {
    display: flex; gap: 10px; align-items: center;
    border-bottom: 3px solid var(--mus); padding-bottom: 10px; flex-wrap: wrap;
  }
  .mus-title small { display: block; color: var(--mus); font-weight: 800; letter-spacing: 1px; font-size: 12px; }
  .mus-title b { font-size: 30px; }
  .mus-foh {
    margin-left: auto; min-height: 60px; font-size: 20px; font-weight: 800;
    border-color: var(--yel); color: var(--yel);
  }
  .mus-foh.ringing { background: #4a2b00; animation: musblink 0.5s steps(2) infinite; }
  .mus-calling {
    text-align: center; font-size: 22px; font-weight: 800; color: #000;
    background: var(--yel); border-radius: 12px; padding: 10px;
    animation: musblink 0.5s steps(2) infinite;
  }
  @keyframes musblink { 50% { opacity: 0.5; } }
  .mus-exit { min-height: 60px; font-size: 18px; font-weight: 800; }
  .mus-master {
    display: flex; gap: 12px; align-items: center;
    background: var(--panel); border: 2px solid var(--mus); border-radius: 14px; padding: 12px;
  }
  .mus-master label { font-weight: 800; font-size: 16px; color: var(--mus); white-space: nowrap; }
  .mus-master input[type='range'] { flex: 1; height: 56px; accent-color: var(--mus); min-width: 0; }
  .mus-master b { font-size: 22px; min-width: 76px; text-align: right; font-variant-numeric: tabular-nums; }
  .mus-strips {
    display: flex; gap: 10px; overflow-x: auto;
    border: 2px solid var(--mus); border-radius: 14px; padding: 12px 10px;
    background: color-mix(in srgb, var(--mus) 6%, var(--bg));
  }
  .mus-strip {
    flex: 1 0 84px; max-width: 130px; display: flex; flex-direction: column;
    gap: 8px; align-items: stretch; background: var(--panel);
    border: 1px solid var(--line); border-radius: 12px; padding: 10px 8px;
    transition: transform 0.08s ease-out, box-shadow 0.08s ease-out;
  }
  .mus-strip.grab { transform: scale(1.05); box-shadow: 0 0 18px var(--mus); border-color: var(--mus); z-index: 5; }
  .mus-name {
    text-align: center; font-weight: 800; font-size: 20px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .mus-track {
    position: relative; height: 240px; background: #0a0c0e;
    border: 1px solid var(--line); border-radius: 8px;
    touch-action: none; cursor: ns-resize;
  }
  .mus-fill {
    position: absolute; left: 0; right: 0; bottom: 0; background: var(--mus);
    border-radius: 0 0 7px 7px;
  }
  .mus-handle {
    position: absolute; left: -4px; right: -4px; height: 24px;
    background: #dfe6ee; border-radius: 6px; box-shadow: 0 1px 4px #000;
  }
  .mus-db { text-align: center; font-size: 16px; color: var(--dim); font-variant-numeric: tabular-nums; }
  .mus-switch { align-self: center; min-height: 52px; font-size: 16px; }
</style>
