<!-- L0 box-neutrality test page (?ui=4&screen=l0-boxtest): renders a layout JSON
     next to hand-written reference HTML producing the same boxes. A Playwright
     check measures every paired getBoundingClientRect equal within 0.5 px for
     a header-like row (56px), a flex strip row (12 fixed-width children) and
     a 2-column grid. -->
<script lang="ts">
  import LayoutRenderer from '../LayoutRenderer.svelte';
  import { fixtureSource } from '../context.ts';
  import type { LayoutNode } from '../schema.ts';

  const source = fixtureSource({});
  const scope = {};

  const stripKids: LayoutNode[] = Array.from({ length: 12 }, (_, i) => ({
    id: `s${i}`,
    module: 'spacer',
    props: { w: 48, h: 40 },
  }));

  const layout: LayoutNode = {
    id: 'root',
    module: 'col',
    props: { gap: 8, width: 600 },
    children: [
      {
        id: 'hdr',
        module: 'row',
        props: { height: 56 },
        children: [
          { id: 'h0', module: 'spacer', props: { w: 100, h: 56 } },
          { id: 'h1', module: 'spacer', props: { w: 100, h: 56 } },
          { id: 'h2', module: 'spacer', props: { w: 100, h: 56 } },
        ],
      },
      { id: 'strips', module: 'row', props: { gap: 2 }, children: stripKids },
      {
        id: 'grid',
        module: 'grid',
        props: { columns: '1fr 1fr', gap: 8, width: 600 },
        children: [
          { id: 'g0', module: 'spacer', props: { h: 30 } },
          { id: 'g1', module: 'spacer', props: { h: 30 } },
          { id: 'g2', module: 'spacer', props: { h: 30 } },
          { id: 'g3', module: 'spacer', props: { h: 30 } },
        ],
      },
    ],
  };

  const stripRef: string[] = Array.from({ length: 12 }, (_, i) => `s${i}`);
</script>

<div id="l0-boxtest" style="display:flex;flex-direction:column;gap:24px;padding:8px;background:#05070b">
  <style>
    [data-side] [data-node],
    [data-side] [data-box] {
      outline: 1px solid rgba(255, 80, 80, 0.65);
    }
  </style>
  <div data-side="layout">
    <LayoutRenderer {layout} {source} {scope} />
  </div>
  <div data-side="ref">
  <div data-box="root" style="display:flex;flex-direction:column;gap:8px;width:600px">
    <div data-box="hdr" style="display:flex;flex-direction:row;height:56px">
      <div data-box="h0" style="width:100px;height:56px"></div>
      <div data-box="h1" style="width:100px;height:56px"></div>
      <div data-box="h2" style="width:100px;height:56px"></div>
    </div>
    <div data-box="strips" style="display:flex;flex-direction:row;gap:2px">
      {#each stripRef as key (key)}
        <div data-box={key} style="width:48px;height:40px"></div>
      {/each}
    </div>
    <div
      data-box="grid"
      style="display:grid;grid-template-columns:1fr 1fr;gap:8px;width:600px"
    >
      <div data-box="g0" style="height:30px"></div>
      <div data-box="g1" style="height:30px"></div>
      <div data-box="g2" style="height:30px"></div>
      <div data-box="g3" style="height:30px"></div>
    </div>
  </div>
  </div>
</div>
