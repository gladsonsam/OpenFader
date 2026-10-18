<!-- SOLO button, styled like the MUTE button but amber.
     Off (idle): dark amber bg, amber text. On (active): amber bg, dark ink.
     Dimmed (muteEnable off or screen locked): opacity 0.45. -->
<script lang="ts">
  let {
    solo = false,
    enabled = true,
    height = 40,
    label = 'Solo',
    onpress = null,
  }: {
    solo?: boolean;
    /** False when muteEnable is off or the screen is locked (opacity 0.45, ignores taps). */
    enabled?: boolean;
    /** 40 (mixer strips) or 44 (channel strip). */
    height?: number;
    /** Accessible name, e.g. 'Solo Ch 1'. */
    label?: string;
    onpress?: (() => void) | null;
  } = $props();
</script>

<button
  aria-pressed={solo ? 'true' : 'false'}
  aria-label={label}
  aria-disabled={enabled ? undefined : 'true'}
  disabled={enabled ? undefined : true}
  onclick={enabled && onpress ? onpress : undefined}
  class="tpress"
  style="height:{height}px;flex:none;border-radius:var(--u4-r-3);border:1px solid {solo
    ? 'var(--u4-amber-hi)'
    : '#6b4d16'};background:{solo
    ? 'var(--u4-amber)'
    : 'var(--u4-amber-deep)'};color:{solo
    ? 'var(--u4-amber-ink)'
    : 'var(--u4-amber)'};opacity:{enabled
    ? 1
    : 0.45};font-size:15px;cursor:{enabled ? 'pointer' : 'default'};padding:0;width:100%"
  >SOLO</button
>
