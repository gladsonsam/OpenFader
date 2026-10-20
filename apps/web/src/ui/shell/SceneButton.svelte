<!-- H1 SceneButton: the header scene-dropdown button, copied verbatim from
     docs/design/src/design-template.html (scene button block, lines 66-70).
     Props: padded scene number (e.g. '005'), scene name, aria label
     ('Scene 5 Changeover, open scene list'), highlighted state (Scenes page
     open: bg #232b37 / border #5a6577, else var(--u4-tile-bg) / var(--u4-divider)), onOpen. -->
<script lang="ts">
  let {
    num,
    name,
    label,
    active = false,
    narrow = false,
    onOpen = null,
  }: {
    num: string;
    name: string;
    /** aria-label, e.g. 'Scene 5 Changeover, open scene list' */
    label: string;
    active?: boolean;
    /** Phone density: cap width and ellipsize the scene name. */
    narrow?: boolean;
    onOpen?: (() => void) | null;
  } = $props();
</script>

<button
  class="u4-scene-button"
  onclick={onOpen}
  aria-label={label}
  style="box-sizing:border-box;height:{narrow ? 44 : 46}px;flex:{narrow ? '0 1 auto' : 'none'};max-width:{narrow ? 112 : 280}px;display:flex;align-items:center;gap:{narrow ? 6 : 10}px;padding:0 {narrow ? 8 : 12}px;border-radius:var(--u4-r-3);border:1px solid {active
    ? 'var(--u4-tile-border-on)'
    : 'var(--u4-tile-border)'};background:{active
    ? 'var(--u4-tile-bg-on)'
    : 'var(--u4-tile-bg)'};color:var(--u4-text-2);cursor:pointer;text-align:left;min-width:0;font-variant-numeric:normal"
>
  <span
    style="display:flex;flex-direction:column;line-height:1.15;min-width:0{narrow
      ? ';flex:1;overflow:hidden'
      : ''}"
    ><span style="font-size:11px;color:var(--u4-text-7)"
      ><span
        style="font-size:15px;white-space:nowrap;color:var(--u4-scene-num, rgb(167, 177, 192));margin-right:6px">{num}</span
      ><span
        style="font-size:15px;font-weight:600;white-space:nowrap;color:var(--u4-scene-name, rgb(238, 242, 247));font-family:inherit{narrow
          ? ';display:inline-block;max-width:100%;overflow:hidden;text-overflow:ellipsis;vertical-align:bottom'
          : ''}"
        >{name}</span
      ></span
    >
  </span>
  <svg
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="var(--u4-text-5)"
    stroke-width="2.2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    style="flex:none"><path d="M6 9l6 6 6-6"></path></svg
  >
</button>
