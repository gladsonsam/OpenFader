<!-- W5c module `strip-fader`: one strip's fader + readout + meter (ModuleProps
     wrapper over the P1 Fader). Drags stay local-first inside Fader
     (faderFromDrag, fine quarter-step); the committed value emits `input`
     (payload.value) for the layout `on` action (ui.setStripFader, which picks
     send vs fader; lock-checked in the store). The accent prop updates alone
     (SOF cyan / master gold need no remount: Fader derives capLine/capGlow).
     Meters are imperative MeterSource closures over the shared meterSim
     (bound through layout `bind`, never reactive per frame). -->
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
      value?: number;
      dbText?: string;
      ariaLabel?: string;
      variant?: 'strip' | 'aside' | 'master';
      accent?: 'chan' | 'sof' | 'master';
      readColor?: string;
      disabled?: boolean;
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
  value={props.value ?? 0.75}
  dbText={props.dbText ?? ''}
  ariaLabel={props.ariaLabel ?? 'Fader'}
  variant={props.variant ?? 'strip'}
  accent={props.accent ?? 'chan'}
  readColor={props.readColor ?? 'var(--u4-text-4)'}
  disabled={props.disabled ?? false}
  fine={props.fine ?? false}
  meter={props.meter ?? null}
  meterFrozen={props.meterFrozen ?? false}
  oninput={(v) => onEvent?.('input', { value: v })}
/>
