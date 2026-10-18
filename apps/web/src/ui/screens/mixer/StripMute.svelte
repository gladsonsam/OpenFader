<!-- W5c module `strip-mute`: one strip's MUTE button (ModuleProps wrapper over
     the P1 MuteButton). Taps emit `press` for the layout `on` action
     (ui.toggleMute; muteEnable + lock are enforced in the store, and the
     button itself dims + ignores taps when disabled). -->
<script lang="ts">
  import MuteButton from '../../primitives/signal/MuteButton.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { muted?: boolean; enabled?: boolean; height?: number; label?: string };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<MuteButton
  muted={props.muted ?? false}
  enabled={props.enabled ?? true}
  height={props.height ?? 40}
  label={props.label ?? 'Mute'}
  onpress={() => onEvent?.('press')}
/>
