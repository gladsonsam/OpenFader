<!-- W5b `send-row`: one Mix row in the Sends tab. Presentational: the
     SendRowVm object in via layout `bind` (repeated over the scope send
     list), `input` ({value}) + `togglePre` out. Horizontal fader verbatim
     from design-template.html (sendRows): 6px track, fill (white pre /
     cyan post), 75% unity tick, 18x36 metal thumb with fill-coloured centre
     line; value text; 56x40 Pre/Post button. Drag maps through
     uiLogic.sendFromDrag (design's 'h' branch); role=slider +
     arrows/PageUp/PageDown/Home/End make it keyboard operable. -->
<script lang="ts">
  import { sendFromDrag } from '../../state/uiLogic.ts';
  import type { ModuleContext } from '../../modules/types.ts';
  import type { SendRowVm } from './channelModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { row?: SendRowVm | null; fine?: boolean };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const row = $derived(
    props.row ?? {
      index: 0,
      name: '',
      value: 0,
      pct: '0.0',
      text: '',
      aria: '',
      fill: '#22d3e6',
      pre: false,
      preLabel: 'Post',
      preAria: '',
      preBg: 'var(--u4-btn-bg)',
      preFg: 'var(--u4-text-4)',
      grpon: null,
      grponSupported: false,
      grponAria: '',
      grponBg: 'var(--u4-btn-bg)',
      grponFg: 'var(--u4-text-4)',
    },
  );
  const fine = $derived(props.fine ?? false);

  let area: HTMLButtonElement | null = $state(null);
  // Tablet direction lock: touch/pen stays pending until horizontal intent
  // locks the send drag; vertical intent aborts so the page scrolls.
  let drag: {
    x0: number;
    y0: number;
    start: number;
    w: number;
    active: boolean;
    el: HTMLElement | null;
  } | null = null;

  function down(e: PointerEvent): void {
    const rect = area?.getBoundingClientRect();
    const w = Math.max(60, (rect?.width ?? 64) - 18);
    const target = e.currentTarget as HTMLElement;
    // All pointers start pending with NO capture: no value change until
    // horizontal intent locks the send drag (then we capture); vertical
    // intent aborts so the page scrolls.
    drag = { x0: e.clientX, y0: e.clientY, start: row.value, w, active: false, el: target };
  }
  function move(e: PointerEvent): void {
    if (!drag) return;
    if (!drag.active) {
      const ax = Math.abs(e.clientX - drag.x0);
      const ay = Math.abs(e.clientY - drag.y0);
      if (ay > 12 && ay > ax) {
        drag = null;
        return;
      }
      if (ax <= 10 || ax <= ay) return;
      try {
        drag.el?.setPointerCapture(e.pointerId);
      } catch {
        /* noop */
      }
      drag.active = true;
    }
    onEvent?.('input', { value: sendFromDrag(drag.start, e.clientX - drag.x0, drag.w, fine) });
  }
  function up(): void {
    drag = null;
  }
  function key(e: KeyboardEvent): void {
    const k = fine ? 0.25 : 1;
    let v: number | null = null;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') v = row.value - 0.01 * k;
    else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') v = row.value + 0.01 * k;
    else if (e.key === 'PageDown') v = row.value - 0.1 * k;
    else if (e.key === 'PageUp') v = row.value + 0.1 * k;
    else if (e.key === 'Home') v = 0;
    else if (e.key === 'End') v = 1;
    else return;
    e.preventDefault();
    onEvent?.('input', { value: Math.max(0, Math.min(1, v)) });
  }
</script>

<div
  style="box-sizing:content-box;height:58px;display:flex;align-items:center;gap:10px;padding:0 10px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-panel-bg)"
>
  <span style="width:54px;flex:none;font-size:15px;font-weight:600">{row.name}</span>
  <button
    bind:this={area}
    role="slider"
    aria-label={row.aria}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(row.value * 100)}
    aria-valuetext={row.text + ' dB'}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onkeydown={key}
    style="box-sizing:border-box;font-variant-numeric:normal;flex:1;min-width:0;height:46px;position:relative;padding:0;border:0;background:transparent;touch-action:pan-y;cursor:ew-resize"
  >
    <span style="position:absolute;left:9px;right:9px;top:0;bottom:0;display:block">
      <span
        style="position:absolute;left:0;right:0;top:50%;height:6px;margin-top:-3px;border-radius:2px;background:var(--u4-io-tile)"
      ></span>
      <span
        style="position:absolute;left:0;width:{row.pct}%;top:50%;height:6px;margin-top:-3px;border-radius:2px;background:{row.fill}"
      ></span>
      <span
        style="position:absolute;left:75%;top:50%;width:1px;height:16px;margin-top:-8px;background:#566175"
      ></span>
      <span
        style="position:absolute;left:{row.pct}%;top:50%;width:18px;height:36px;transform:translate(-50%, -50%);border-radius:3px;background:linear-gradient(to right, #646a73, #30343b 45%, #1c1f24 50%, #30343b 55%, #646a73);box-shadow:0 3px 6px rgba(0,0,0,.7), inset 0 1px 0 rgba(255,255,255,.25)"
      >
        <span
          style="position:absolute;top:3px;bottom:3px;left:50%;width:2px;margin-left:-1px;background:{row.fill}"
        ></span>
      </span>
    </span>
  </button>
  <span style="width:50px;flex:none;text-align:right;font-size:13px">{row.text}</span>
  {#if row.grponSupported}
    <button
      onclick={() => onEvent?.('toggleGrpon')}
      aria-pressed={row.grpon === true ? 'true' : 'false'}
      aria-label={row.grponAria}
      title={row.grpon === null ? 'Group-on state unknown — tap to set' : undefined}
      style="box-sizing:border-box;font-variant-numeric:normal;width:56px;height:40px;flex:none;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{row.grponBg};color:{row.grponFg};font-size:12px;font-weight:600;cursor:pointer"
      >{row.grpon === true ? 'GRP' : row.grpon === false ? 'grp' : 'GRP ?'}</button
    >
  {/if}
  <button
    onclick={() => onEvent?.('togglePre')}
    aria-pressed={row.pre ? 'true' : 'false'}
    aria-label={row.preAria}
    style="box-sizing:border-box;font-variant-numeric:normal;width:56px;height:40px;flex:none;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{row.preBg};color:{row.preFg};font-size:12px;font-weight:600;cursor:pointer"
    >{row.preLabel}</button
  >
</div>
