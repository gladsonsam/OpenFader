<!-- W5b `channel-tab-button`: one Processing rail tab button.
     Data-driven: repeated over the tab descriptor list in scope (id, title,
     sub-label note, selected visuals). Presentational: the entry object in
     via layout `bind`, `pick` (with the tab id) out via `onEvent`.
     Styles verbatim from design-template.html (channel tab rail): 56px
     buttons, 2px selected border in the channel colour, title 15px +
     note 12px var(--u4-text-9). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      entry?: {
        id: string;
        title: string;
        note: string;
        current: boolean;
        border: string;
        bg: string;
      };
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const e = $derived(
    props.entry ?? { id: '', title: '', note: '', current: false, border: 'var(--u4-btn-bg)', bg: 'var(--u4-btn-bg)' },
  );

  // Phone rail is a horizontal proximity snap scroller: keep the selected
  // tab in view on tab switches (no-op on desktop where the rail fits).
  // Smooth + nearest so the rail glides instead of jumping mid-swipe; the
  // rail's scroll-padding (not an exact edge) absorbs the landing.
  let btn: HTMLButtonElement | null = $state(null);
  $effect(() => {
    if (e.current && btn) btn.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
  });
</script>

<button
  bind:this={btn}
  onclick={() => onEvent?.('pick', { id: e.id })}
  aria-current={e.current ? 'page' : 'false'}
  style="box-sizing:border-box;font-variant-numeric:normal;height:56px;flex:none;position:relative;border-radius:5px;border:2px solid {e.border};background:{e.bg};color:{e.current
    ? 'var(--u4-channel-tab-fg, #eef2f7)'
    : 'var(--u4-text-2)'};font-size:15px;cursor:pointer;text-align:left;padding:0 10px;scroll-snap-align:start"
  >{e.title}
  <span style="display:block;font-size:12px;color:var(--u4-text-9)">{e.note}</span>
</button>
