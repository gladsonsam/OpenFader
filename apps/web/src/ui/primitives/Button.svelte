<!-- Generic button, pixel-matched to the template's grey/green/danger buttons.
     Template values: radius 3px (tabs 5px via `radius` prop), border
     var(--u4-line-ctl), bg var(--u4-btn-bg) / green #5cdb5c ink #10240f / danger #e5322b on,
     #4b0d0f off; heights 30/34/36/40/44/46/48; font 12/13/14/15px; w600 opt. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  export type ButtonVariant = 'default' | 'primary' | 'danger' | 'ghost';
  export type ButtonTone = 'on' | 'off';

  let {
    variant = 'default',
    tone = 'on',
    height = 40,
    fontSize = 14,
    bold = false,
    radius = 3,
    selected = false,
    disabled = false,
    pressed = null,
    label = null,
    ariaLabel = null,
    minWidth = null,
    padding = '0 16px',
    onclick = null,
    children,
  }: {
    variant?: ButtonVariant;
    /** danger buttons have on/off tones (MUTE lit vs unlit) */
    tone?: ButtonTone;
    height?: number;
    fontSize?: number;
    bold?: boolean;
    radius?: number;
    selected?: boolean;
    disabled?: boolean;
    /** aria-pressed value; null omits the attribute */
    pressed?: boolean | null;
    label?: string | null;
    ariaLabel?: string | null;
    minWidth?: number | null;
    padding?: string;
    onclick?: ((e: MouseEvent) => void) | null;
    children?: Snippet;
  } = $props();

  const style = $derived.by(() => {
    let border = 'var(--u4-btn-border)';
    let bg = 'var(--u4-btn-bg)';
    let fg = 'var(--u4-btn-fg)';
    if (variant === 'primary') {
      border = 'var(--u4-green)';
      bg = 'var(--u4-green)';
      fg = 'var(--u4-green-ink)';
    } else if (variant === 'danger') {
      if (tone === 'on') {
        border = 'var(--u4-red-hi)';
        bg = 'var(--u4-red-btn)';
        fg = '#ffffff';
      } else {
        border = 'var(--u4-red-line)';
        bg = 'var(--u4-red-btn-bg-off)';
        fg = 'var(--u4-red-btn-tx-off)';
      }
    } else if (variant === 'ghost') {
      bg = 'transparent';
    } else if (selected) {
      bg = 'var(--u4-chip-on-bg)';
      fg = 'var(--u4-chip-on-fg)';
      border = 'var(--u4-chip-on-bg)';
    }
    return [
      `height:${height}px`,
      `font-size:${fontSize}px`,
      `font-weight:${bold ? 600 : 400}`,
      `border-radius:${radius}px`,
      `border:1px solid ${border}`,
      `background:${bg}`,
      `color:${fg}`,
      `padding:${padding}`,
      minWidth !== null ? `min-width:${minWidth}px` : '',
      disabled ? 'opacity:0.45' : '',
    ]
      .filter(Boolean)
      .join(';');
  });
</script>

<button
  style={style + ';cursor:pointer'}
  aria-pressed={pressed === null ? undefined : pressed}
  aria-label={ariaLabel ?? undefined}
  aria-disabled={disabled ? 'true' : undefined}
  disabled={disabled || undefined}
  onclick={disabled ? undefined : onclick}
>{#if children}{@render children()}{:else}{label}{/if}</button>
