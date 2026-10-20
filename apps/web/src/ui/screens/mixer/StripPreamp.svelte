<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { Channel } from '../../fixtures/designData.ts';

  let { props = {}, onEvent }: {
    props: { input?: Channel['input']; name?: string };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
  const gain = $derived(Math.max(0, Math.min(100, (props.input?.gain ?? 0) / 60 * 100)));
  const trim = $derived(Math.max(0, Math.min(100, ((props.input?.trim ?? 0) + 18) / 36 * 100)));
</script>

<button class="strip-preamp" disabled={!props.input}
  onclick={() => onEvent?.('open')}
  aria-label={`Open input for ${props.name ?? 'channel'}`}
  title={props.input ? `Gain ${props.input.gain} dB · Trim ${props.input.trim} dB · 48V ${props.input.phantom ? 'on' : 'off'}` : 'No preamp'}>
  {#if props.input}
    <span class="preamp-row" aria-hidden="true">
      <span class="track"><span class="gain-fill" style={`width:${gain}%`}></span><span class="marker" style={`left:calc(${gain}% - ${gain / 100 * 4}px)`}></span></span>
      <span class:phantom-on={props.input.phantom} class="phantom">48V</span>
    </span>
    <span class="preamp-row" aria-hidden="true">
      <span class="track"><span class="trim-fill" style={`width:${trim}%`}></span><span class="marker" style={`left:calc(${trim}% - ${trim / 100 * 4}px)`}></span></span>
    </span>
  {/if}
</button>

<style>
  .strip-preamp { flex:none; height:48px; min-height:48px; padding:4px 5px; box-sizing:border-box; border:1px solid var(--u4-line-panel); border-radius:3px; background:var(--u4-well-bg-2); cursor:pointer; }
  .strip-preamp:disabled { cursor:default; opacity:1; }
  .preamp-row { display:flex; align-items:center; gap:5px; height:18px; }
  .track { display:block; position:relative; flex:1; height:8px; border:1px solid var(--u4-line-panel); border-radius:1px; overflow:hidden; background:var(--u4-well-bg); }
  .marker { position:absolute; top:0; width:4px; height:100%; background:var(--u4-preamp-marker, #a9adb3); }
  .trim-fill { display:block; height:100%; background:#759b32; }
  .gain-fill { display:block; height:100%; background:#d64545; }
  .phantom { font-size:9px; color:var(--u4-text-10); }
  .phantom-on { color:#ff5454; font-weight:700; }
</style>
