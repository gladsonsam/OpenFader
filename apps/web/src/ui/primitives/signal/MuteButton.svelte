<!-- MUTE button, pixel-matched to the design's stripVals mute styling.
     Off (idle): border #6a1b1e, bg #4b0d0f, text #f2d6d6.
     On (active): border #ff7a73, bg #e5322b, text #ffffff.
     Dimmed (muteEnable off or screen locked): opacity 0.45. -->
<script lang="ts">
  let {
    muted = false,
    known = true,
    enabled = true,
    height = 40,
    label = 'Mute',
    onpress = null,
  }: {
    muted?: boolean;
    known?: boolean;
    /** False when muteEnable is off or the screen is locked (opacity 0.45, ignores taps). */
    enabled?: boolean;
    /** 40 (mixer/master strips) or 44 (channel strip). */
    height?: number;
    /** Accessible name, e.g. 'Mute Ch 1'. */
    label?: string;
    onpress?: (() => void) | null;
  } = $props();
</script>

<button
  aria-pressed={known ? muted ? 'true' : 'false' : undefined}
  aria-label={label}
  aria-disabled={enabled ? undefined : 'true'}
  disabled={enabled ? undefined : true}
  onclick={enabled && onpress ? onpress : undefined}
  class="tpress"
  style="height:{height}px;flex:none;border-radius:var(--u4-r-3);border:1px solid {muted
    ? 'var(--u4-red-hi)'
    : 'var(--u4-red-line)'};background:{muted
    ? 'var(--u4-red-btn)'
    : 'var(--u4-red-btn-bg-off)'};color:{muted
    ? '#ffffff'
    : 'var(--u4-red-btn-tx-off)'};opacity:{enabled
    ? 1
    : 0.45};font-size:15px;cursor:{enabled ? 'pointer' : 'default'};padding:0;width:100%"
  >{known ? 'MUTE' : 'MUTE ?'}</button
>
