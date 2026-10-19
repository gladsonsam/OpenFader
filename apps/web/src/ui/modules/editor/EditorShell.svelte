<!-- Visual-editor shell: Edit-mode chrome around any LayoutRenderer canvas.
     Owns selection + all ProfileOp plumbing (palette insert, canvas
     drop/move, inspector set/hide/remove/move/reset, outline reorder,
     share import, banks/pages). Hidden entirely when the profile is locked
     or the role is musician (see canEditProfile); operators may not Delete
     (they Hide instead) nor edit banks/pages. Reuses applyOp-free pure
     helpers: findNode/findParent (schema.ts), diffFromDefault/resetNode
     (ops.ts) — ops commit through the profile store (lock-checked). -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { getModule } from '../registry.ts';
  import type { ModuleManifest, Role } from '../types.ts';
  import { CONTAINERS, findNode, findParent, type LayoutNode } from '../schema.ts';
  import { diffFromDefault, resetNode } from '../profile/ops.ts';
  import { importProfile } from '../profile/io.ts';
  import type { BankDef, PageDef } from '../profile/io.ts';
  import type { ProfileStore } from '../profile/store.svelte.ts';
  import { canEditProfile, canRemoveNode, canEditLibrary } from './editMode.svelte.ts';
  import Palette from './Palette.svelte';
  import Inspector from './Inspector.svelte';
  import Outline from './Outline.svelte';
  import SharePanel from './SharePanel.svelte';

  let {
    defaults,
    tree,
    store,
    role = 'engineer',
    onRole,
    children,
  }: {
    defaults: LayoutNode;
    tree: LayoutNode;
    store: ProfileStore;
    role?: Role;
    onRole?: (r: Role) => void;
    children?: Snippet;
  } = $props();

  const locked = $derived(store.profile.locked);
  const editable = $derived(canEditProfile(locked, role));
  let edit = $state(false);
  let selectedId = $state<string | null>(null);
  let showShare = $state(false);
  let idCounter = $state(0);

  const editing = $derived(edit && editable);
  const selected = $derived(selectedId ? (findNode(tree, selectedId) ?? undefined) : undefined);

  function uniqueId(base: string): string {
    let n = idCounter + 1;
    while (findNode(tree, `${base}-${n}`)) n += 1;
    idCounter = n;
    return `${base}-${n}`;
  }

  function containerForInsert(): { parent: string; index: number } {
    const sel = selectedId ? findNode(tree, selectedId) : undefined;
    if (sel && CONTAINERS.has(sel.module)) return { parent: sel.id, index: sel.children?.length ?? 0 };
    if (sel) {
      const p = findParent(tree, sel.id);
      if (p) return { parent: p.id, index: (p.children ?? []).findIndex(c => c.id === sel.id) + 1 };
    }
    return { parent: tree.id, index: tree.children?.length ?? 0 };
  }

  function insertModule(m: ModuleManifest): void {
    if (!editing) return;
    const { parent, index } = containerForInsert();
    const id = uniqueId(m.id);
    const node: LayoutNode = { id, module: m.id, props: JSON.parse(JSON.stringify(m.defaults ?? {})) };
    if (store.commit({ op: 'insert', parent, index, node })) selectedId = id;
  }

  function insertModuleAt(moduleId: string, targetId: string | null): void {
    const m = getModule(moduleId);
    if (!m || !editing) return;
    const target = targetId ? findNode(tree, targetId) : undefined;
    let parent = tree.id;
    let index = tree.children?.length ?? 0;
    if (target && CONTAINERS.has(target.module)) {
      parent = target.id;
      index = target.children?.length ?? 0;
    } else if (target) {
      const p = findParent(tree, target.id);
      if (p) {
        parent = p.id;
        index = (p.children ?? []).findIndex(c => c.id === target.id) + 1;
      }
    }
    const node: LayoutNode = { id: uniqueId(m.id), module: m.id, props: JSON.parse(JSON.stringify(m.defaults ?? {})) };
    if (store.commit({ op: 'insert', parent, index, node })) selectedId = node.id;
  }

  function moveNodeAt(nodeId: string, target: string, index: number): void {
    if (!editing) return;
    if (target.startsWith('__sibling:')) {
      const sib = findNode(tree, target.slice('__sibling:'.length));
      const p = sib ? findParent(tree, sib.id) : undefined;
      if (!sib || !p) return;
      const at = (p.children ?? []).findIndex(c => c.id === sib.id) + index;
      store.commit({ op: 'move', id: nodeId, parent: p.id, index: Math.max(0, at) });
      return;
    }
    store.commit({ op: 'move', id: nodeId, parent: target, index });
  }

  function siblingStep(id: string, dir: -1 | 1): void {
    const p = findParent(tree, id);
    if (!p) return;
    const at = (p.children ?? []).findIndex(c => c.id === id);
    store.commit({ op: 'move', id, parent: p.id, index: Math.max(0, Math.min((p.children ?? []).length - 1, at + dir)) });
  }

  function parseRaw(key: 'bind' | 'on' | 'when', raw: string): void {
    if (!selectedId || !editing) return;
    const t = raw.trim();
    if (t === '') {
      store.commit({ op: 'set', id: selectedId, path: key, value: undefined });
      return;
    }
    if (key === 'when' && !(t.startsWith('{') || t.startsWith('['))) {
      store.commit({ op: 'set', id: selectedId, path: key, value: t });
      return;
    }
    try {
      store.commit({ op: 'set', id: selectedId, path: key, value: JSON.parse(t) });
    } catch {
      // Bad JSON: keep the old value; the validate() warnings surface it.
    }
  }

  function resetSelected(): void {
    if (!selectedId || !editing) return;
    const next = resetNode(tree, defaults, selectedId);
    if (next.ok) store.setOps(diffFromDefault(defaults, next.tree));
  }

  function importJson(json: string): string | null {
    const r = importProfile(json);
    if (!r.ok) return r.errors.join('; ');
    if (!store.replace(r.profile)) return 'profile is locked';
    selectedId = null;
    return null;
  }

  function onCanvasClick(e: MouseEvent): void {
    if (!editing) return;
    const el = (e.target as HTMLElement).closest?.('[data-node]') as HTMLElement | null;
    selectedId = el?.getAttribute('data-node') ?? null;
  }

  function onCanvasDrop(e: DragEvent): void {
    if (!editing) return;
    e.preventDefault();
    const moduleId = e.dataTransfer?.getData('application/x-openmix-module');
    const nodeId = e.dataTransfer?.getData('application/x-openmix-node');
    const el = (e.target as HTMLElement).closest?.('[data-node]') as HTMLElement | null;
    const targetId = el?.getAttribute('data-node') ?? null;
    if (moduleId) insertModuleAt(moduleId, targetId);
    else if (nodeId && targetId && nodeId !== targetId) {
      const target = findNode(tree, targetId);
      if (target && CONTAINERS.has(target.module)) moveNodeAt(nodeId, target.id, target.children?.length ?? 0);
      else moveNodeAt(nodeId, `__sibling:${targetId}`, 1);
    } else if (nodeId && !targetId) {
      moveNodeAt(nodeId, tree.id, tree.children?.length ?? 0);
    }
  }

  function setRole(r: Role): void {
    onRole?.(r);
    if (r === 'musician') {
      edit = false;
      selectedId = null;
    }
  }
</script>

<div aria-label="Layout editor" style="display:flex;flex-direction:column;gap:6px">
  <div style="display:flex;flex-wrap:wrap;gap:6px;align-items:center">
    {#if editable}
      <button
        aria-pressed={edit}
        onclick={() => {
          edit = !edit;
          if (!edit) {
            selectedId = null;
            showShare = false;
          }
        }}
        style="font-size:12px;font-weight:600"
      >{edit ? 'Done' : 'Edit layout'}</button>
    {:else}
      <span style="font-size:12px;color:var(--u4-text-5)" title={locked ? 'Profile is locked' : 'Musicians get a fixed layout'}>
        {locked ? '🔒 locked profile' : 'Musician view (fixed layout)'}
      </span>
    {/if}
    {#if edit && editable}
      <label style="font-size:12px;color:var(--u4-text-4)">
        Role
        <select aria-label="Preview role" value={role} onchange={(e) => setRole((e.currentTarget as HTMLSelectElement).value as Role)}>
          <option value="engineer">engineer</option>
          <option value="operator">operator</option>
          <option value="musician">musician</option>
        </select>
      </label>
      <button onclick={() => store.undo()} disabled={!store.canUndo} style="font-size:12px">Undo</button>
      <button onclick={() => store.redo()} disabled={!store.canRedo} style="font-size:12px">Redo</button>
      <button onclick={() => store.reset()} style="font-size:12px">Reset all</button>
      <button aria-pressed={showShare} onclick={() => (showShare = !showShare)} style="font-size:12px">Share</button>
      <span style="font-size:11px;color:var(--u4-text-5)">{store.profile.ops.length} ops{selectedId ? ` · ${selectedId}` : ''}</span>
    {/if}
  </div>

  {#if editing}
    <div style="display:flex;flex-wrap:wrap;gap:8px;align-items:flex-start">
      <Palette {role} onInsert={insertModule} />
      <div style="flex:1;min-width:260px">
        <Outline
          {tree}
          {selectedId}
          onSelect={(id) => (selectedId = id)}
          onMove={moveNodeAt}
          onToggleHidden={(id) => store.commit({ op: 'hide', id, hidden: !(findNode(tree, id)?.hidden ?? false) })}
        />
      </div>
      <Inspector
        node={selected}
        canDelete={canRemoveNode(role)}
        onSetProp={(path, value) => {
          if (selectedId) store.commit({ op: 'set', id: selectedId, path, value });
        }}
        onSetRaw={parseRaw}
        onToggleHidden={() => {
          if (selectedId) store.commit({ op: 'hide', id: selectedId, hidden: !(selected?.hidden ?? false) });
        }}
        onRemove={() => {
          if (selectedId) {
            store.commit({ op: 'remove', id: selectedId });
            selectedId = null;
          }
        }}
        onMoveUp={() => selectedId && siblingStep(selectedId, -1)}
        onMoveDown={() => selectedId && siblingStep(selectedId, 1)}
        onReset={resetSelected}
      />
      {#if showShare}
        <SharePanel
          profile={store.profile}
          locked={store.profile.locked}
          canEditLibrary={canEditLibrary(role)}
          onImport={importJson}
          onBanks={(b: BankDef[]) => store.setBanks(b)}
          onPages={(p: PageDef[]) => store.setPages(p)}
        />
      {/if}
    </div>
  {/if}

  <div
    role={editing ? 'application' : undefined}
    aria-label={editing ? 'Layout canvas (drop modules here)' : undefined}
    onclick={onCanvasClick}
    ondragover={editing
      ? (e) => {
          e.preventDefault();
        }
      : undefined}
    ondrop={onCanvasDrop}
    style={editing
      ? 'outline:2px dashed var(--u4-line-ctl);outline-offset:2px;border-radius:6px;min-height:40px'
      : undefined}
  >
    {@render children?.()}
  </div>
</div>
