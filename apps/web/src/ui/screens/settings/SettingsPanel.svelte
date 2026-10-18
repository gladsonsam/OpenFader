<!-- S910 module `settings-panel`: titled card for one Settings section
      (Display / Faders / Safety / About). L1 module form: title in via
      layout `bind`, descriptor-driven `setting-row` children from the layout
      `repeat`. Inline styles verbatim from docs/design/src/design-template.html
      (settings section). Box model: section/divs content-box like the
      design's browser defaults, so boxes hold both before and after the
      global border-box rule in app.css is removed. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    children,
  }: {
    props: {
      title?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();
</script>

<section
  aria-label={props.title ?? ''}
  style="box-sizing:content-box;display:flex;flex-direction:column;border-radius:3px;border:1px solid #1f2836;background:#0e1520"
>
  <div style="box-sizing:content-box;padding:12px 14px 4px;font-size:12px;color:#8f9bb0">{props.title ?? ''}</div>
  {@render children?.()}
</section>
