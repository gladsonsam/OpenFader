<!-- Visual-editor module palette: free-text search + category filter over
     queryPalette (registry.ts), click-to-insert or HTML5 drag onto the canvas.
     Manifests whose `roles` exclude the current role are hidden (role gate);
     locked profiles disable the whole palette (parent hides it via canEdit). -->
<script lang="ts">
  import { queryPalette } from '../registry.ts';
  import type { ModuleManifest, Role } from '../types.ts';

  let {
    role = 'engineer',
    onInsert,
  }: {
    role?: Role;
    onInsert: (manifest: ModuleManifest) => void;
  } = $props();

  let query = $state('');
  let category = $state<string>('');

  const results = $derived.by(() => {
    const all = queryPalette(query, (category || undefined) as ModuleManifest['category'] | undefined);
    return all.filter(m => !m.roles || m.roles.includes(role));
  });

  function dragStart(e: DragEvent, m: ModuleManifest): void {
    e.dataTransfer?.setData('application/x-openmix-module', m.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
  }
</script>

<div aria-label="Module palette" style="display:flex;flex-direction:column;gap:6px;min-width:180px;max-width:230px">
  <input
    type="search"
    aria-label="Search modules"
    placeholder="Search modules…"
    bind:value={query}
    style="padding:4px 8px;font-size:12px"
  />
  <select aria-label="Category filter" bind:value={category} style="padding:4px;font-size:12px">
    <option value="">All categories</option>
    <option value="strip">Strip</option>
    <option value="channel">Channel</option>
    <option value="screen">Screen</option>
    <option value="control">Control</option>
    <option value="layout">Layout</option>
    <option value="display">Display</option>
  </select>
  <div style="display:flex;flex-direction:column;gap:4px;overflow-y:auto;max-height:320px">
    {#each results as m (m.id)}
      <div
        role="button"
        tabindex="0"
        aria-label={`Insert ${m.title}`}
        title={`${m.title} (${m.id})`}
        draggable="true"
        ondragstart={(e) => dragStart(e, m)}
        onclick={() => onInsert(m)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') onInsert(m);
        }}
        style="border:1px solid var(--u4-line-ctl);border-radius:6px;padding:4px 8px;font-size:12px;cursor:grab;background:var(--u4-panel-bg);color:var(--u4-text-1)"
      >
        <div style="font-weight:600">{m.title}</div>
        <div style="font-size:10px;color:var(--u4-text-5)">{m.id} · {m.category}</div>
      </div>
    {/each}
    {#if results.length === 0}
      <div style="font-size:12px;color:var(--u4-text-5)">No modules match.</div>
    {/if}
  </div>
</div>
