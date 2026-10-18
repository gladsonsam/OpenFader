<!-- W5b `channel-mute`: master strip MUTE button (44px channel variant).
     Thin wrapper over the P1 MuteButton: props in, `press` out. -->
<script lang="ts">
  import MuteButton from '../../primitives/signal/MuteButton.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { muted?: boolean; enabled?: boolean; label?: string };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<MuteButton
  muted={props.muted ?? false}
  enabled={props.enabled ?? true}
  height={44}
  label={props.label ?? 'Mute'}
  onpress={() => onEvent?.('press')}
/>
