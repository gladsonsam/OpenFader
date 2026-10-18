<!-- W5b `channel-tab-button`: one Processing rail tab button.
     Data-driven: repeated over the tab descriptor list in scope (id, title,
     sub-label note, selected visuals). Presentational: the entry object in
     via layout `bind`, `pick` (with the tab id) out via `onEvent`.
     Styles verbatim from design-template.html (channel tab rail): 56px
     buttons, 2px selected border in the channel colour, title 15px +
     note 12px #7f8a9c. -->
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
    props.entry ?? { id: '', title: '', note: '', current: false, border: '#2b3038', bg: '#2b3038' },
  );
</script>

<button
  onclick={() => onEvent?.('pick', { id: e.id })}
  aria-current={e.current ? 'page' : 'false'}
  style="box-sizing:border-box;font-variant-numeric:normal;height:56px;flex:none;position:relative;border-radius:5px;border:2px solid {e.border};background:{e.bg};color:#eef2f7;font-size:15px;cursor:pointer;text-align:left;padding:0 10px"
  >{e.title}
  <span style="display:block;font-size:12px;color:#7f8a9c">{e.note}</span>
</button>
