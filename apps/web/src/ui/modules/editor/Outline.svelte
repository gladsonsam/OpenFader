<!-- Visual-editor outline: the layout tree as a navigable list. Selecting
     focuses the inspector; HTML5 drag re-parents/reorders via `move` ops;
     drop on the background removes nothing (delete is an explicit button).
     Hidden and missing-module nodes are badged so the editor state is honest. -->
<script lang="ts">
  import { CONTAINERS, MISSING_MODULE, type LayoutNode } from '../schema.ts';
  import { getModule } from '../registry.ts';

  let {
    tree,
    selectedId,
    onSelect,
    onMove,
    onToggleHidden,
  }: {
    tree: LayoutNode;
    selectedId: string | null;
    onSelect: (id: string) => void;
    onMove: (id: string, parent: string, index: number) => void;
    onToggleHidden: (id: string) => void;
  } = $props();

  function isContainer(n: LayoutNode): boolean {
    return CONTAINERS.has(n.module);
  }

  function dropOn(e: DragEvent, target: LayoutNode, index?: number): void {
    e.preventDefault();
    e.stopPropagation();
    const moveId = e.dataTransfer?.getData('application/x-openmix-node');
    if (moveId && moveId !== target.id) {
      if (isContainer(target)) {
        onMove(moveId, target.id, index ?? (target.children?.length ?? 0));
      } else {
        // Dropping on a leaf means "into its parent at its position" — the
        // parent resolves the caller's intent without extra lookups here.
        onMove(moveId, `__sibling:${target.id}`, index ?? 0);
      }
      return;
    }
  }

  function dragNode(e: DragEvent, n: LayoutNode): void {
    e.dataTransfer?.setData('application/x-openmix-node', n.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
</script>

{#snippet row(n: LayoutNode, depth: number)}
  {@const missing = n.module === MISSING_MODULE || getModule(n.module) === undefined}
  <div
    role="treeitem"
    aria-selected={selectedId === n.id}
    aria-label={n.id}
    tabindex="0"
    draggable="true"
    ondragstart={(e) => dragNode(e, n)}
    ondragover={(e) => {
      e.preventDefault();
    }}
    ondrop={(e) => dropOn(e, n)}
    onclick={() => onSelect(n.id)}
    onkeydown={(e) => {
      if (e.key === 'Enter') onSelect(n.id);
    }}
    style="padding:2px 6px 2px {6 + depth * 12}px;font-size:12px;cursor:pointer;border-radius:4px;{selectedId === n.id
      ? 'background:var(--u4-line-ctl);'
      : ''}{n.hidden ? 'opacity:0.55;' : ''}"
  >
    <span style="color:var(--u4-text-5)">{isContainer(n) ? '▤' : '▫'}</span>
    {n.id}
    <span style="color:var(--u4-text-5)">· {n.module}</span>
    {#if n.hidden}<span style="color:var(--u4-text-5)"> · hidden</span>{/if}
    {#if missing}<span style="color:#f0b44a"> · missing</span>{/if}
    <button
      aria-label={`Toggle hidden ${n.id}`}
      onclick={(e) => {
        e.stopPropagation();
        onToggleHidden(n.id);
      }}
      style="font-size:10px;margin-left:4px">{n.hidden ? 'show' : 'hide'}</button>
  </div>
  {#each n.children ?? [] as c (c.id)}
    {@render row(c, depth + 1)}
  {/each}
{/snippet}

<div role="tree" aria-label="Layout outline" style="display:flex;flex-direction:column;gap:1px;overflow-y:auto;max-height:340px;min-width:180px">
  {@render row(tree, 0)}
</div>
