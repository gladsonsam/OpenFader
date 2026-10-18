<!-- Tiny icon renderer for ui/icons.ts definitions.
     fill="none" stroke="currentColor" like the template; callers set width,
     height, colour and stroke colour via props/style. -->
<script lang="ts">
  import { icons, type IconName } from '../icons.ts';

  let {
    name,
    size = 22,
    stroke = 'currentColor',
    label = null,
  }: {
    name: IconName;
    size?: number;
    stroke?: string;
    /** accessible label; null renders aria-hidden (decorative, like the template) */
    label?: string | null;
  } = $props();

  const icon = $derived(icons[name]);
</script>

<svg
  width={size}
  height={size}
  viewBox={icon.viewBox}
  fill="none"
  stroke={stroke}
  stroke-width={icon.strokeWidth}
  stroke-linecap={icon.linecap ?? 'butt'}
  stroke-linejoin={icon.linejoin ?? 'miter'}
  aria-hidden={label === null ? 'true' : undefined}
  aria-label={label ?? undefined}
  role={label !== null ? 'img' : undefined}
>
  {@html icon.body}
</svg>
