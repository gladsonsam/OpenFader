<!-- W5b `channel-fader`: master strip fader + meter (aside variant, 46x60
     cap) with its dB readout line. Thin wrapper over the P1 Fader: props
     in (the live meter feed arrives as a bound object from scope),
     `input` (with the 0..1 value) out. Local-first drag/keyboard semantics
     stay inside the primitive (SPEC 2a). -->
<script lang="ts">
  import Fader from '../../primitives/signal/Fader.svelte';
  import type { MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      pos?: number;
      readout?: string;
      aria?: string;
      accent?: 'chan' | 'sof';
      readColor?: string;
      fine?: boolean;
      meter?: MeterSource | null;
      meterFrozen?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<Fader
  value={props.pos ?? 0}
  dbText={props.readout ?? ''}
  ariaLabel={props.aria ?? 'Fader'}
  variant="aside"
  accent={props.accent ?? 'chan'}
  readColor={props.readColor ?? '#c9d1dc'}
  fine={props.fine ?? false}
  meter={props.meter ?? null}
  meterFrozen={props.meterFrozen ?? false}
  oninput={(v) => onEvent?.('input', { value: v })}
/>
