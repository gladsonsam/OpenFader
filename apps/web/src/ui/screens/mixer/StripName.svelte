<!-- W5c module `strip-name`: one strip's colour name bar (ModuleProps wrapper
     over the P1 NameBar). Taps emit `select` for the layout `on` action
     (ui.select). -->
<script lang="ts">
  import NameBar from '../../primitives/signal/NameBar.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      name?: string;
      sub?: string;
      color?: string;
      text?: string;
      icon?: string | null;
      variant?: 'strip' | 'plate';
      selectAria?: string | null;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<NameBar
  name={props.name ?? ''}
  sub={props.sub ?? ''}
  color={props.color ?? '#222a36'}
  text={props.text ?? '#e8edf5'}
  icon={props.icon ?? null}
  variant={props.variant ?? 'strip'}
  selectAria={props.selectAria ?? null}
  onselect={() => onEvent?.('select')}
/>
