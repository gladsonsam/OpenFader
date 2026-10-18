<!-- H1 BankRail: the right-hand bank/layer rail, copied verbatim from
     docs/design/src/design-template.html (nav block, lines 192-196).
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
    /** 'side' (desktop right rail, verbatim design) or 'bottom' (phone bar). */
    orientation?: 'side' | 'bottom';
  } = $props();
</script>

{#if orientation === 'bottom'}
  <nav
    aria-label="Layers"
    style="height:56px;flex:none;display:flex;flex-direction:row;gap:4px;overflow-x:auto"
  >
    {#each banks as b (b.key)}
      {@const s = bankBtnStyle(b, b.key === activeKey)}
      <button
        onclick={() => onSelect(b.key)}
        aria-pressed={b.key === activeKey ? 'true' : 'false'}
        title={b.label}
        style="flex:1 0 68px;min-height:48px;align-self:stretch;border-radius:var(--u4-r-8);border:1px solid {s.border};background:{s.bg};color:{s.fg};font-size:12px;font-weight:{s.weight};cursor:pointer;padding:0 4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-variant-numeric:normal"
        >{b.label}</button
      >
    {/each}
  </nav>
{:else}
  <nav aria-label="Layers" style="width:92px;flex:none;display:flex;flex-direction:column;gap:4px">
    {#each banks as b (b.key)}
      {@const s = bankBtnStyle(b, b.key === activeKey)}
      <button
        onclick={() => onSelect(b.key)}
        aria-pressed={b.key === activeKey ? 'true' : 'false'}
        style="flex:1 1 0;min-height:48px;border-radius:var(--u4-r-8);border:1px solid {s.border};background:{s.bg};color:{s.fg};font-size:15px;font-weight:{s.weight};cursor:pointer;padding:0 4px;white-space:nowrap;font-variant-numeric:normal"
        >{b.label}</button
      >
    {/each}
  </nav>
{/if}
