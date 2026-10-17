<script lang="ts">
  // First-run wizard: pick a desk, type its LAN address, test the link,
  // then walk a 5-step guided tour. Shown once (localStorage flag);
  // reopened any time from the top-bar "?" button. Skippable throughout.
  import type { Backend } from '../lib/backend';

  interface Props {
    backend: Backend;
    online: boolean;
    onDone: () => void;
    onGoto: (view: 'overview' | 'mixer' | 'channel' | 'shows') => void;
  }

  let { backend, online, onDone, onGoto }: Props = $props();

  type Phase = 'connect' | 'tour';
  type Target = 'sim' | 'xair' | 'dl32s';

  let phase: Phase = $state('connect');
  let target: Target = $state('sim');
  let addr: string = $state('');
  let testing: boolean = $state(false);
  let result: string = $state('');
  let failed: boolean = $state(false);
  let tourIdx: number = $state(0);

  interface TourStep {
    title: string;
    body: string;
    goto: 'overview' | 'mixer' | 'channel' | 'shows';
  }

  const TOUR: TourStep[] = [
    {
      title: 'Layers: one bank at a time',
      body: 'Mixer shows one fader bank — Inputs, Buses, Mains, or your own. Toolbar → LAYOUT reorders strips, adds spacers, or builds a custom layer from what is on screen. Layout saves per browser.',
      goto: 'mixer',
    },
    {
      title: 'Sends-on-fader: the aux rail',
      body: 'Tap a bus in the aux rail (under the view nav) and input faders flip to send levels for that bus — the app tints to the bus colour and a banner reads SENDS → name. You are NOT on LR while tinted. EXIT ✕, LR, or the same bus returns. copy LR seeds the bus from the house balance.',
      goto: 'mixer',
    },
    {
      title: 'Channel detail: one strip, big',
      body: 'Tap a strip to open Channel: big fader, gain (−20…+60 dB), HPF, gate/comp, 4-band EQ with a draggable graph, aux/FX sends, and rename (≤16 chars). Controls the desk rejects disappear with a one-line hint — nothing is broken.',
      goto: 'channel',
    },
    {
      title: 'Shows: scenes survive the night',
      body: 'Store S1–S4 slots for quick snapshots; build Shows (one per band/set, one scene per song) for the full set. Everything lives in this browser — use the same device all night or re-store on the spare. Reconnect + recall your scene after any dropout to re-assert faders and mutes.',
      goto: 'shows',
    },
    {
      title: 'Meter bridge: whole-show health',
      body: 'Top-bar bridge toggles a full-width live meter canvas — every strip side by side, clip dots latch at ≥0.985 (tap a meter to clear). Handy at FOH on a laptop; leave it off on a phone to save battery. If meters freeze, check the relay first, not the desk: offline means no meter frame for 1.5 s.',
      goto: 'mixer',
    },
  ];

  const step: TourStep = $derived(TOUR[tourIdx] ?? TOUR[0]!);

  $effect(() => {
    // Keep the app view behind the tour card on the step being described.
    if (phase === 'tour' && step) onGoto(step.goto);
  });

  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

  function pick(t: Target): void {
    target = t;
    failed = false;
    result = '';
  }

  async function testConnect(): Promise<void> {
    testing = true;
    failed = false;
    result = `connecting ${target}${addr.trim() ? ` ${addr.trim()}` : ' (sim)'}…`;
    try {
      const msg = await backend.connect(target, target === 'sim' ? '' : addr.trim());
      result = `${msg} — link is live.`;
    } catch (e) {
      failed = true;
      result = `connect failed: ${errMsg(e)}`;
    } finally {
      testing = false;
    }
  }

  function startMixing(): void {
    onGoto('mixer');
    onDone();
  }

  function next(): void {
    if (tourIdx < TOUR.length - 1) tourIdx += 1;
    else onDone();
  }
  function back(): void {
    if (tourIdx > 0) tourIdx -= 1;
  }
</script>

<div class="obwrap" role="dialog" aria-modal="true" aria-label="first-run setup">
  <button class="obback" onclick={onDone} aria-label="close setup"></button>
  <div class="obcard">
    {#if phase === 'connect'}
      <h2>Connect to a mixer</h2>
      <p class="obsub">Pick what is in front of you. Nothing here needs the internet — desk, relay, and this device just share a LAN.</p>

      <div class="obtargets" role="radiogroup" aria-label="mixer type">
        <button
          class:sel={target === 'sim'}
          role="radio"
          aria-checked={target === 'sim'}
          onclick={() => pick('sim')}
        >
          <b>Sim demo</b>
          <span>Rehearse with no desk. Auto-connects, 8 CH strips at 0.75 — safe to learn on.</span>
        </button>
        <button
          class:sel={target === 'xair'}
          role="radio"
          aria-checked={target === 'xair'}
          onclick={() => pick('xair')}
        >
          <b>Behringer XR18 (X-Air)</b>
          <span>UDP 10024. Type the mixer LAN IP. Rename works; faders quantise to ~10-bit steps.</span>
        </button>
        <button
          class:sel={target === 'dl32s'}
          role="radio"
          aria-checked={target === 'dl32s'}
          onclick={() => pick('dl32s')}
        >
          <b>Mackie DL32S</b>
          <span>TCP 50001, IP required — there is no discovery. Rename is expected to fail; re-label from Master Fader.</span>
        </button>
      </div>

      {#if target !== 'sim'}
        <label class="obaddr">Mixer LAN address
          <input
            inputmode="numeric"
            placeholder="192.168.1.10"
            autocomplete="off"
            spellcheck={false}
            bind:value={addr}
          />
        </label>
        <p class="obhint">
          Same subnet rule: this device, the relay (if any), and the desk must share one LAN —
          wired preferred for the relay. Discovery is directed IP only: broadcast crosses neither
          subnets nor Docker's default bridge, so always type the address. Find it on the desk's
          network screen (XR18: X-Air Edit → Setup; DL32S: Master Fader → network).
        </p>
      {/if}

      <div class="obrow">
        <button class="big" disabled={testing || (target !== 'sim' && !addr.trim())} onclick={testConnect}>
          {testing ? 'connecting…' : 'test-connect'}
        </button>
        <button onclick={onDone}>skip for now</button>
      </div>

      {#if result}
        <p class="obresult" class:ok={!failed && online} class:bad={failed} role="status">{result}</p>
      {/if}

      {#if failed}
        <ol class="obfix">
          <li>Confirm the desk shows a LAN link light and this device is on the same Wi-Fi / subnet — compare the first three octets (e.g. <code>192.168.1.x</code>).</li>
          <li>Ping the address from this device. No reply → wrong IP, wrong subnet, or a venue router isolating clients.</li>
          <li>Check the port path: XR18 listens on UDP 10024; DL32S on TCP 50001 (no broadcast — the IP field is mandatory).</li>
          <li>If a relay fronts the desk, pass the mixer IP to it explicitly (<code>--addr &lt;mixer-ip&gt;</code>) instead of relying on discovery.</li>
          <li>Still dark? Connect <b>Sim demo</b> and rehearse offline — scenes, shows, and layers save locally and recall after you reconnect.</li>
        </ol>
      {/if}

      {#if !failed && online && result}
        <div class="obrow">
          <button class="big" onclick={() => { phase = 'tour'; }}>take the 5-step tour →</button>
          <button onclick={startMixing}>start mixing →</button>
        </div>
      {/if}
    {:else}
      <h2>Tour {tourIdx + 1} of {TOUR.length}: {step.title}</h2>
      <p class="obsub">{step.body}</p>
      <div class="obdots" aria-label={`step ${tourIdx + 1} of ${TOUR.length}`}>
        {#each TOUR as _, i (i)}
          <span class:on={i === tourIdx} class:seen={i < tourIdx}></span>
        {/each}
      </div>
      <div class="obrow">
        <button disabled={tourIdx === 0} onclick={back}>← back</button>
        <button class="big" onclick={next}>{tourIdx === TOUR.length - 1 ? 'finish → mixer' : 'next →'}</button>
        <button onclick={onDone}>skip tour</button>
      </div>
      {#if tourIdx === TOUR.length - 1}
        <p class="obhint">Replay any time from the top-bar <b>?</b> button. Same device all night — your scenes live in this browser.</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .obwrap {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }
  .obback {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    border: 0;
  }
  .obcard {
    position: relative;
    width: min(560px, 100%);
    max-height: 88vh;
    overflow: auto;
    background: #14161a;
    color: #e8eaed;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 18px;
  }
  .obcard h2 { margin: 0 0 6px; font-size: 1.1rem; }
  .obsub { margin: 0 0 12px; color: #b9bec7; font-size: 0.9rem; }
  .obtargets { display: grid; gap: 8px; margin-bottom: 12px; }
  .obtargets button {
    text-align: left;
    background: #1d2026;
    color: inherit;
    border: 1px solid #3a3f47;
    border-radius: 8px;
    padding: 10px 12px;
    display: grid;
    gap: 2px;
    cursor: pointer;
  }
  .obtargets button.sel { border-color: #4da3ff; box-shadow: 0 0 0 1px #4da3ff; }
  .obtargets button span { color: #b9bec7; font-size: 0.85rem; }
  .obaddr { display: grid; gap: 4px; font-size: 0.9rem; margin-bottom: 6px; }
  .obaddr input {
    background: #0f1114;
    color: inherit;
    border: 1px solid #3a3f47;
    border-radius: 6px;
    padding: 10px;
    font-size: 1rem;
    min-height: 44px;
  }
  .obhint { color: #b9bec7; font-size: 0.85rem; }
  .obrow { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .obrow button {
    min-height: 44px;
    padding: 8px 14px;
    border-radius: 8px;
    border: 1px solid #3a3f47;
    background: #1d2026;
    color: inherit;
    cursor: pointer;
  }
  .obrow button.big { background: #2b6cb0; border-color: #2b6cb0; color: #fff; }
  .obrow button:disabled { opacity: 0.5; cursor: default; }
  .obresult { margin-top: 10px; padding: 8px 10px; border-radius: 6px; font-size: 0.9rem; }
  .obresult.ok { background: #12351f; border: 1px solid #2c7a3f; }
  .obresult.bad { background: #3a1d1d; border: 1px solid #a33; }
  .obfix { margin: 10px 0 0; padding-left: 20px; font-size: 0.88rem; color: #d6dae1; display: grid; gap: 6px; }
  .obfix code { background: #0f1114; padding: 1px 5px; border-radius: 4px; }
  .obdots { display: flex; gap: 6px; margin: 4px 0 2px; }
  .obdots span { width: 22px; height: 6px; border-radius: 3px; background: #3a3f47; }
  .obdots span.seen { background: #2b6cb0; }
  .obdots span.on { background: #4da3ff; }
</style>
