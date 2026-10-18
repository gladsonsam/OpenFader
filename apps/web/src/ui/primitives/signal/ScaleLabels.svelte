<!-- Signal scale labels, pixel-matched to the design's fader scale.
     Marks (pos,label): 1/10, .875/5, .75/0, .625/-5, .5/-10, .375/-20,
     .25/-30, .125/-50. The '0' tick is var(--u4-text-1), the rest #566175;
     labels 10/11px var(--u4-text-7) right-aligned. Fills its parent (the fader
     area); the parent sets the top/bottom insets per variant. -->
<script lang="ts" module>
  export type { ScaleMark } from './signalGeometry.ts';
  export { SCALE_MARKS } from './signalGeometry.ts';
  import { SCALE_MARKS } from './signalGeometry.ts';
</script>

<script lang="ts">
  import { scaleTickColor } from './signalGeometry.ts';
  let {
    variant = 'strip',
  }: {
    /** 'strip' (mixer/master: 17px labels, 10px font) or 'aside' (channel strip: 20px, 11px). */
    variant?: 'strip' | 'aside';
  } = $props();

  const labelW = variant === 'aside' ? 20 : 17;
  const fontSize = variant === 'aside' ? 11 : 10;
  const tickX = variant === 'aside' ? 23 : 19;
  const tickW = variant === 'aside' ? 9 : 7;
</script>

<div aria-hidden="true" style="position:absolute;inset:0;display:block">
  {#each SCALE_MARKS as m (m.label)}
    <span
      style="position:absolute;left:0;width:{labelW}px;bottom:{m.pct}%;transform:translateY(50%);font-size:{fontSize}px;line-height:1;color:var(--u4-text-7);text-align:right"
      >{m.label}</span
    >
    <!-- TODO token: scale tick grey #566175 is not in tokens.css (only the '0' tick uses --u4-text-1). -->
    <span
      style="position:absolute;left:{tickX}px;width:{tickW}px;height:1px;bottom:{m.pct}%;background:{scaleTickColor(
        m.label,
      )}"
    ></span>
  {/each}
</div>
