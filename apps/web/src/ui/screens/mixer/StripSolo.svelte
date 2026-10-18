<!-- W5c module `strip-solo`: one strip's SOLO button (ModuleProps wrapper over
     the P1 SoloButton). Taps emit `press` for the layout `on` action
     (ui.toggleSolo; muteEnable + lock are enforced in the store like mute,
     and the button itself dims + ignores taps when disabled). Solo is
     UI-only: the desk protocol has no solo counterpart. -->
<script lang="ts">
  import SoloButton from '../../primitives/signal/SoloButton.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { solo?: boolean; enabled?: boolean; height?: number; label?: string };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<SoloButton
  solo={props.solo ?? false}
  enabled={props.enabled ?? true}
  height={props.height ?? 40}
  label={props.label ?? 'Solo'}
  onpress={() => onEvent?.('press')}
/>
