<!-- L1 generic exact-style button leaf: one label, fully data-driven colours
     and geometry via bound props (chips, scan, connect, disconnect, unpatch
     ...). Clicks emit `press`; the node's `on.press` action(s) run through
     the action registry. The inner span (default) keeps the design's
     measured text boxes identical where the design wraps chip text in an
     element (diff box `route-bank` measures it); `bare: true` renders the
     label as a bare text node like the design's action buttons. -->
<script lang="ts">
  import type { ModuleContext } from '../types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      bg?: string;
      fg?: string;
      border?: string;
      height?: number;
      padding?: string;
      minWidth?: number | string;
      fontSize?: number;
      pressed?: string;
      ariaLabel?: string;
      /** Render the label as a bare text node (design's action buttons). */
      bare?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const mw = $derived(
    props.minWidth === undefined
      ? ''
      : `min-width:${typeof props.minWidth === 'number' ? props.minWidth + 'px' : props.minWidth};`,
  );
  const style = $derived(
    'box-sizing:border-box;font-variant-numeric:normal;' +
      `height:${props.height ?? 36}px;` +
      mw +
      `padding:${props.padding ?? '0 12px'};border-radius:3px;` +
      `border:${props.border ?? '1px solid #3a414c'};` +
      `background:${props.bg ?? '#2b3038'};color:${props.fg ?? '#e8edf5'};` +
      `font-size:${props.fontSize ?? 13}px;cursor:pointer`,
  );
</script>

<button
  onclick={() => onEvent?.('press', { node: nodeId })}
  aria-pressed={props.pressed}
  aria-label={props.ariaLabel}
  {style}>{#if props.bare}{props.label ?? ''}{:else}<span>{props.label ?? ''}</span>{/if}</button
>
