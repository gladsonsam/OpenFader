<!-- H1 BankRail: the bank/layer rail. One continuous surface, not floating
     cards: square segments with hairline dividers, the active bank flooded
     with its colour edge-to-edge (console layer buttons). No gaps and no
     outer radius, so nothing reads as a rounded pill stuck to the screen
     edge; segments also make bigger touch targets.
     Props: bank descriptors (key/label/color/text from bankDefs), the active
     bank key, and onSelect (no store import: presentational). Shown on the
     Mixer only (the Shell owns that condition). -->
<script lang="ts">
  import { bankBtnStyle } from './shellStyle.ts';

  let {
    banks,
    activeKey,
    onSelect,
    orientation = 'side',
  }: {
    banks: Array<{ key: string; label: string; color: string; text: string }>;
    activeKey: string;
    onSelect: (key: string) => void;
    /** 'side' (desktop rail) or 'bottom' (phone bar). */
    orientation?: 'side' | 'bottom';
  } = $props();
</script>

{#if orientation === 'bottom'}
  <nav
    aria-label="Layers"
    style="height:56px;flex:none;display:flex;flex-direction:row;overflow-x:auto;background:#101722;border-top:1px solid var(--u4-line-panel)"
  >
    {#each banks as b, i (b.key)}
      {@const s = bankBtnStyle(b, b.key === activeKey)}
      <button
        onclick={() => onSelect(b.key)}
        aria-pressed={b.key === activeKey ? 'true' : 'false'}
        title={b.label}
        class="tpress"
        style="flex:1 0 68px;min-height:48px;align-self:stretch;border:0;border-radius:0;border-left:{i ===
        0
          ? '0'
          : '1px solid var(--u4-line-panel)'};background:{s.bg};color:{s.fg};font-size:12px;font-weight:{s.weight};cursor:pointer;padding:0 4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-variant-numeric:normal"
        >{b.label}</button
      >
    {/each}
  </nav>
{:else}
  <nav
    aria-label="Layers"
    style="width:92px;flex:none;display:flex;flex-direction:column;background:#101722;border-left:1px solid var(--u4-line-panel)"
  >
    {#each banks as b, i (b.key)}
      {@const s = bankBtnStyle(b, b.key === activeKey)}
      <button
        onclick={() => onSelect(b.key)}
        aria-pressed={b.key === activeKey ? 'true' : 'false'}
        class="tpress"
        style="flex:1 1 0;min-height:56px;border:0;border-radius:0;border-top:{i ===
        0
          ? '0'
          : '1px solid var(--u4-line-panel)'};background:{s.bg};color:{s.fg};font-size:15px;font-weight:{s.weight};cursor:pointer;padding:0 4px;white-space:nowrap;font-variant-numeric:normal"
        >{b.label}</button
      >
    {/each}
  </nav>
{/if}
