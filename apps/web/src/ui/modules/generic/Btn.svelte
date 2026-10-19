<!-- L1 generic exact-style button leaf: one label, fully data-driven colours
     and geometry via bound props (chips, scan, connect, disconnect, unpatch
     ...). Clicks emit `press`; the node's `on.press` action(s) run through
     the action registry. The inner span (default) keeps the design's
     measured text boxes identical where the design wraps chip text in an
     element (diff box `route-bank` measures it); `bare: true` renders the
     label as a bare text node like the design's action buttons. -->
<script lang="ts">
  import type { ModuleContext } from '../types.ts';
  import Icon from '../../primitives/Icon.svelte';
  import type { IconName } from '../../icons.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      icon?: IconName;
      iconSize?: number;
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
      `border:${props.border ?? '1px solid var(--u4-line-ctl)'};` +
      `background:${props.bg ?? 'var(--u4-btn-bg)'};color:${props.fg ?? 'var(--u4-text-1)'};` +
      `font-size:${props.fontSize ?? 13}px;cursor:pointer` +
      (props.icon ? ';display:inline-flex;align-items:center;justify-content:center' : ''),
  );
</script>

<button
  onclick={() => onEvent?.('press', { node: nodeId })}
  aria-pressed={props.pressed}
  aria-label={props.ariaLabel}
  {style}>{#if props.icon}<Icon name={props.icon} size={props.iconSize ?? 16} />{:else if props.bare}{props.label ?? ''}{:else}<span>{props.label ?? ''}</span>{/if}</button
>
