<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { Backend, Strip } from '../lib/backend';
  import { levels } from '../lib/levels';
  import { isBusCh } from '../lib/sends';

  interface Props {
    backend: Backend;
    strips: Strip[];
    say: (t: string) => void;
  }

  let { backend, strips, say }: Props = $props();

  /** Gain-staging target: peaks should kiss this line, never park above it. */
  const TARGET_DBFS = -18;
  const targetLevel = Math.pow(10, TARGET_DBFS / 20);
  /** Clip latch threshold — same as the mixer view (App.svelte loop). */
  const CLIP_LEVEL = 0.985;
  /** Headamp dB range (proto-xair headamp, via xair-api-python). */
  const GAIN_MIN = -12;
  const GAIN_MAX = 60;
  const STEP = 1;

  // Input strips only: no buses, no MAIN pseudo-strip.
  const inputs = $derived(strips.filter((s) => s.ch >= 0 && !isBusCh(s.ch)));
  const total = $derived(inputs.length);

  let pos = $state(0);
  /** Local gain dB per ch (backend never echoes params — like Detail). */
  let gains: Record<number, number> = $state({});
  /** 48V state per ch (local mirror; desk is the truth on connect). */
  let phantom: Record<number, boolean> = $state({});
  /** Armed confirm for enabling +48V on the current channel. */
  let armPhantom = $state(false);
  /** Manual "this channel is staged" checks. */
  let done: Record<number, boolean> = $state({});
  /** Latched clips per ch (tap the meter to clear). */
  let latched: Record<number, boolean> = $state({});

  const cur = $derived(inputs[Math.min(pos, Math.max(0, total - 1))] ?? null);
  const doneCount = $derived(inputs.filter((s) => done[s.ch]).length);

  function levelDbfs(v: number): string {
    if (v <= 0.0001) return '-inf';
    return `${(20 * Math.log10(Math.max(v, 0.0001))).toFixed(1)}`;
  }

  function gainOf(ch: number): number {
    return gains[ch] ?? 0;
  }

  async function stepGain(d: number): Promise<void> {
    const s = cur;
    if (!s) return;
    const next = Math.min(GAIN_MAX, Math.max(GAIN_MIN, gainOf(s.ch) + d));
    gains[s.ch] = next;
    try {
      await backend.setParam(s.ch, 'gain', next);
    } catch (e) {
      say(`gain ch${s.ch + 1}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function togglePhantom(): Promise<void> {
    const s = cur;
    if (!s) return;
    if (phantom[s.ch]) {
      // Switching OFF is safe: single tap, no confirm.
      phantom[s.ch] = false;
      armPhantom = false;
      try {
        await backend.setParam(s.ch, 'phantom', false);
      } catch (e) {
        say(`phantom ch${s.ch + 1}: ${e instanceof Error ? e.message : String(e)}`);
      }
      return;
    }
    // Switching ON needs an explicit confirm — never accidental 48V.
    if (!armPhantom) {
      armPhantom = true;
      say(`+48V on ${s.name}: tap again to confirm`);
      return;
    }
    armPhantom = false;
    phantom[s.ch] = true;
    try {
      await backend.setParam(s.ch, 'phantom', true);
    } catch (e) {
      phantom[s.ch] = false;
      say(`phantom ch${s.ch + 1}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  function toggleDone(): void {
    const s = cur;
    if (!s) return;
    done[s.ch] = !done[s.ch];
  }

  function go(d: number): void {
    if (!total) return;
    pos = (pos + d + total) % total;
  }

  function clearClip(): void {
    const s = cur;
    if (s && latched[s.ch]) latched = { ...latched, [s.ch]: false };
  }

  // Disarm the phantom confirm whenever the channel changes.
  $effect(() => {
    if (cur) armPhantom = false;
  });

  /* ---------- big meter: imperative canvas paint on rAF (meters.ts rules) ---------- */
  let meter: HTMLCanvasElement | null = $state(null);
  let raf = 0;

  function paint(): void {
    const cv = meter;
    if (cv && cur) {
      const ctx = cv.getContext('2d');
      if (ctx) {
        const W = cv.width;
        const H = cv.height;
        const v = Math.min(1, levels[cur.ch] ?? 0);
        if (!latched[cur.ch] && v >= CLIP_LEVEL) latched = { ...latched, [cur.ch]: true };
        ctx.clearRect(0, 0, W, H);
        const bw = Math.max(1, v * W);
        ctx.fillStyle = latched[cur.ch] ? '#e5484d' : v > 0.75 ? '#f5b83f' : '#46c46a';
        ctx.fillRect(0, 0, bw, H);
        // Target marker: -18 dBFS line.
        const tx = Math.min(W - 2, targetLevel * W);
        ctx.fillStyle = '#fff';
        ctx.fillRect(tx, 0, 2, H);
        ctx.fillStyle = 'rgba(255,255,255,.7)';
        ctx.font = `${Math.max(10, H * 0.32)}px sans-serif`;
        ctx.fillText(`${TARGET_DBFS}`, tx + 5, H * 0.62);
        // Clip latch LED.
        ctx.fillStyle = latched[cur.ch] ? '#ff2d2d' : 'rgba(255,45,45,.18)';
        ctx.fillRect(0, 0, W, Math.max(3, H * 0.1));
      }
    }
    raf = requestAnimationFrame(paint);
  }

  onMount(() => {
    raf = requestAnimationFrame(paint);
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
  });
</script>

<section id="wizard" aria-label="gain staging wizard">
  <div id="wprog" role="status">
    <b>gain staging</b>
    <span>{doneCount}/{total} done</span>
    <div id="wbar" aria-hidden="true"><div id="wfill" style={`width: ${total ? (doneCount / total) * 100 : 0}%`}></div></div>
  </div>

  {#if !cur}
    <div class="empty">no input channels on this backend yet</div>
  {:else}
    <div id="wnav">
      <button onclick={() => go(-1)} aria-label="previous channel">◀ prev</button>
      <h2>{cur.name} <small>ch{cur.ch + 1}</small></h2>
      <button onclick={() => go(1)} aria-label="next channel">next ▶</button>
    </div>

    <button id="wmeterbtn" onclick={clearClip} title="tap to clear a latched clip" aria-label={latched[cur.ch] ? 'clip latched — tap to clear' : `input level ${levelDbfs(levels[cur.ch] ?? 0)} dBFS`}>
      <canvas
        id="wmeter"
        aria-hidden="true"
        bind:this={meter}
        width={640}
        height={72}
      ></canvas>
    </button>
    <div id="wlevel">
      <span>in {levelDbfs(levels[cur.ch] ?? 0)} dBFS</span>
      <span>tgt {TARGET_DBFS} dBFS</span>
      {#if latched[cur.ch]}<b id="wclip">CLIP — tap meter to clear</b>{/if}
    </div>

    <div id="wgain">
      <button class="step" onclick={() => stepGain(-STEP)} aria-label="decrease headamp gain by {STEP} dB">−</button>
      <div id="wgval" role="status" aria-label={`headamp gain ${gainOf(cur.ch)} dB`}>
        <b>{gainOf(cur.ch) >= 0 ? '+' : ''}{gainOf(cur.ch).toFixed(1)}</b><span>dB headamp</span>
      </div>
      <button class="step" onclick={() => stepGain(STEP)} aria-label="increase headamp gain by {STEP} dB">+</button>
    </div>
    <div id="wcoarse">
      <button onclick={() => stepGain(-6)}>−6</button>
      <button onclick={() => stepGain(6)}>+6</button>
    </div>

    <div id="wphantom">
      <button
        id="wph"
        class:on={!!phantom[cur.ch]}
        class:arm={armPhantom}
        aria-pressed={!!phantom[cur.ch]}
        onclick={togglePhantom}
        title={phantom[cur.ch] ? 'tap to switch +48V off' : armPhantom ? 'tap again to confirm +48V' : 'tap, then confirm, to enable +48V'}
      >
        {phantom[cur.ch] ? '+48V ON' : armPhantom ? 'tap again: +48V?' : '+48V off'}
      </button>
    </div>

    <button id="wdone" class:on={!!done[cur.ch]} aria-pressed={!!done[cur.ch]} onclick={toggleDone}>
      {done[cur.ch] ? '✓ staged' : 'mark staged'}
    </button>
  {/if}
</section>

<style>
  #wizard {
    max-width: 560px;
    margin: 0 auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  #wprog {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
  }
  #wbar {
    flex: 1;
    height: 8px;
    background: #1a2230;
    border-radius: 4px;
    overflow: hidden;
  }
  #wfill {
    height: 100%;
    background: #46c46a;
  }
  #wnav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  #wnav h2 {
    margin: 0;
    flex: 1;
    text-align: center;
  }
  #wnav h2 small {
    color: #9aa4b2;
    font-weight: normal;
  }
  button {
    font-size: 15px;
    padding: 10px 14px;
    border-radius: 8px;
    border: 1px solid #334155;
    background: #1a2230;
    color: #e2e8f0;
    cursor: pointer;
  }
  #wmeterbtn {
    padding: 0;
    border-color: #475569;
    line-height: 0;
  }
  #wmeter {
    width: 100%;
    height: 72px;
    display: block;
    border-radius: 8px;
  }
  #wlevel {
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 13px;
    color: #9aa4b2;
  }
  #wclip {
    color: #ff2d2d;
  }
  #wgain {
    display: flex;
    align-items: stretch;
    gap: 8px;
  }
  .step {
    flex: 1;
    font-size: 28px;
    padding: 14px 0;
  }
  #wgval {
    flex: 1.4;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border: 1px solid #334155;
    border-radius: 8px;
    background: #0f172a;
  }
  #wgval b {
    font-size: 30px;
  }
  #wgval span {
    font-size: 12px;
    color: #9aa4b2;
  }
  #wcoarse {
    display: flex;
    gap: 8px;
  }
  #wcoarse button {
    flex: 1;
  }
  #wphantom {
    display: flex;
  }
  #wph {
    flex: 1;
  }
  #wph.on {
    border-color: #f5b83f;
    color: #f5b83f;
  }
  #wph.arm {
    border-color: #ff2d2d;
    color: #ff2d2d;
  }
  #wdone {
    font-size: 17px;
  }
  #wdone.on {
    border-color: #46c46a;
    color: #46c46a;
  }
  .empty {
    color: #9aa4b2;
    text-align: center;
    padding: 24px 0;
  }
</style>
