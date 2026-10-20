// L0 profile ops: pure layout-override operations on layout trees by node id.
// A profile stores ONLY overrides (ops) over shipped defaults, so shipped
// defaults can improve without wiping user changes. Never throws.

import { cloneNode, findNode, findParent } from '../schema.ts';
import type { LayoutNode } from '../schema.ts';
import type { BankDef, PageDef, TokenOverrides, UserProfile } from './io.ts';

export type ProfileOp =
  | { op: 'set'; id: string; path: string; value: unknown }
  | { op: 'remove'; id: string }
  | { op: 'insert'; parent: string; index: number; node: LayoutNode }
  | { op: 'move'; id: string; parent: string; index: number }
  | { op: 'hide'; id: string; hidden: boolean };

export type OpResult = { ok: true } | { ok: false; error: string };

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function setPath(obj: Record<string, unknown>, path: string, value: unknown): boolean {
  const parts = path.split('.');
  let cur = obj;
  for (let i = 0; i < parts.length - 1; i++) {
    const p = parts[i];
    if (!isRecord(cur[p])) cur[p] = {};
    cur = cur[p] as Record<string, unknown>;
  }
  cur[parts[parts.length - 1]] = cloneNode(value);
  return true;
}

/** Validate one op (unknown ids are NOT errors here — checked at apply time). */
export function validateOp(op: unknown): OpResult {
  if (!isRecord(op)) return { ok: false, error: 'op must be an object' };
  const kind = op['op'];
  if (kind === 'set') {
    if (typeof op['id'] !== 'string' || !op['id']) return { ok: false, error: 'set needs id' };
    if (typeof op['path'] !== 'string' || !op['path']) return { ok: false, error: 'set needs path' };
    if (!/^[a-zA-Z0-9_.]+$/.test(op['path'] as string)) return { ok: false, error: 'set path has invalid characters' };
    return { ok: true };
  }
  if (kind === 'remove') {
    if (typeof op['id'] !== 'string' || !op['id']) return { ok: false, error: 'remove needs id' };
    return { ok: true };
  }
  if (kind === 'insert') {
    if (typeof op['parent'] !== 'string' || !op['parent']) return { ok: false, error: 'insert needs parent' };
    if (typeof op['index'] !== 'number' || !Number.isInteger(op['index']) || op['index'] < 0) {
      return { ok: false, error: 'insert needs a non-negative integer index' };
    }
    if (!isRecord(op['node'])) return { ok: false, error: 'insert needs node' };
    return { ok: true };
  }
  if (kind === 'move') {
    if (typeof op['id'] !== 'string' || !op['id']) return { ok: false, error: 'move needs id' };
    if (typeof op['parent'] !== 'string' || !op['parent']) return { ok: false, error: 'move needs parent' };
    if (typeof op['index'] !== 'number' || !Number.isInteger(op['index']) || op['index'] < 0) {
      return { ok: false, error: 'move needs a non-negative integer index' };
    }
    return { ok: true };
  }
  if (kind === 'hide') {
    if (typeof op['id'] !== 'string' || !op['id']) return { ok: false, error: 'hide needs id' };
    if (typeof op['hidden'] !== 'boolean') return { ok: false, error: 'hide needs hidden boolean' };
    return { ok: true };
  }
  return { ok: false, error: `unknown op: ${String(kind)}` };
}

/**
 * Apply one op to a tree IN PLACE (pass a clone). Locked profiles reject
 * every mutation. Returns ok / error; unknown ids are errors (skipped by
 * applyProfile, which collects them).
 */
export function applyOp(tree: LayoutNode, op: ProfileOp, locked = false): OpResult {
  if (locked) return { ok: false, error: 'profile is locked' };
  const v = validateOp(op);
  if (!v.ok) return v;
  if (op.op === 'set') {
    const node = findNode(tree, op.id);
    if (!node) return { ok: false, error: `unknown node: ${op.id}` };
    setPath(node as unknown as Record<string, unknown>, op.path, op.value);
    return { ok: true };
  }
  if (op.op === 'hide') {
    const node = findNode(tree, op.id);
    if (!node) return { ok: false, error: `unknown node: ${op.id}` };
    node.hidden = op.hidden;
    return { ok: true };
  }
  if (op.op === 'remove') {
    if (tree.id === op.id) return { ok: false, error: 'cannot remove the root' };
    const parent = findParent(tree, op.id);
    if (!parent) return { ok: false, error: `unknown node: ${op.id}` };
    parent.children = (parent.children ?? []).filter(c => c.id !== op.id);
    return { ok: true };
  }
  if (op.op === 'insert') {
    const parent = findNode(tree, op.parent);
    if (!parent) return { ok: false, error: `unknown parent: ${op.parent}` };
    if (findNode(tree, op.node.id)) return { ok: false, error: `duplicate node id: ${op.node.id}` };
    const kids = (parent.children ??= []);
    kids.splice(Math.min(op.index, kids.length), 0, cloneNode(op.node));
    return { ok: true };
  }
  // move
  if (tree.id === op.id) return { ok: false, error: 'cannot move the root' };
  const from = findParent(tree, op.id);
  const to = findNode(tree, op.parent);
  if (!from) return { ok: false, error: `unknown node: ${op.id}` };
  if (!to) return { ok: false, error: `unknown parent: ${op.parent}` };
  const idx = (from.children ?? []).findIndex(c => c.id === op.id);
  const [node] = (from.children ?? []).splice(idx, 1);
  // Moving into own descendant is rejected.
  if (findNode(node, op.parent)) {
    from.children!.splice(idx, 0, node);
    return { ok: false, error: 'cannot move a node into its own descendant' };
  }
  const kids = (to.children ??= []);
  kids.splice(Math.min(op.index, kids.length), 0, node);
  return { ok: true };
}

export interface ApplyReport {
  tree: LayoutNode;
  skipped: string[];
}

/**
 * Apply a user profile's layout ops over shipped defaults. Ops that no
 * longer apply (node removed by a defaults update) are skipped, never fatal —
 * so an override on node X survives a defaults update that adds node Y.
 */
export function applyProfile(defaults: LayoutNode, profile: Pick<UserProfile, 'ops' | 'locked'>): ApplyReport {
  const tree = cloneNode(defaults);
  const skipped: string[] = [];
  if (profile.locked) {
    skipped.push(...profile.ops.map((_, i) => `op[${i}]: profile is locked`));
    return { tree, skipped };
  }
  profile.ops.forEach((op, i) => {
    const r = applyOp(tree, op, false);
    if (!r.ok) skipped.push(`op[${i}]: ${(r as { error: string }).error}`);
  });
  return { tree, skipped };
}

function propsDiff(base: Record<string, unknown>, cur: Record<string, unknown>, prefix: string, out: ProfileOp[], id: string): void {
  for (const k of Object.keys(cur)) {
    const b = base[k];
    const c = cur[k];
    if (JSON.stringify(b) === JSON.stringify(c)) continue;
    if (isRecord(b) && isRecord(c)) {
      propsDiff(b as Record<string, unknown>, c as Record<string, unknown>, `${prefix}.${k}`, out, id);
    } else {
      out.push({ op: 'set', id, path: `${prefix}.${k}`, value: cloneNode(c) });
    }
  }
}

/**
 * Diff a current tree against shipped defaults, producing the minimal op
 * list that reproduces it (move/insert/remove/hide/set). Pure. L1: `bind`,
 * `on` and `when` differences also become `set` ops, so a user-added working
 * button (insert op with `on.click`) and computed labels (`bind.*`) survive
 * export/import round-trips.
 */
export function diffFromDefault(defaults: LayoutNode, current: LayoutNode): ProfileOp[] {
  const ops: ProfileOp[] = [];
  // Index defaults by id for membership tests.
  const defIds = new Set<string>();
  const walk = (n: LayoutNode): void => {
    defIds.add(n.id);
    (n.children ?? []).forEach(walk);
  };
  walk(defaults);

  const diff = (d: LayoutNode | undefined, c: LayoutNode, parentId: string, index: number): void => {
    if (!d) {
      ops.push({ op: 'insert', parent: parentId, index, node: cloneNode(c) });
      return;
    }
    if ((d.hidden ?? false) !== (c.hidden ?? false)) {
      ops.push({ op: 'hide', id: c.id, hidden: c.hidden ?? false });
    }
    propsDiff((d.props ?? {}) as Record<string, unknown>, (c.props ?? {}) as Record<string, unknown>, 'props', ops, c.id);
    // L1 overlay keys: whole-record `set` ops when they differ (an added
    // `on.click` or `bind.title` becomes settable / exportable user data).
    for (const key of ['bind', 'on', 'when'] as const) {
      const b = (d as unknown as Record<string, unknown>)[key];
      const n = (c as unknown as Record<string, unknown>)[key];
      if (n !== undefined && JSON.stringify(b ?? null) !== JSON.stringify(n)) {
        ops.push({ op: 'set', id: c.id, path: key, value: cloneNode(n) });
      }
    }
    const dKids = d.children ?? [];
    const cKids = c.children ?? [];
    const dById = new Map(dKids.map(k => [k.id, k]));
    // Removals + order changes.
    const dOrder = dKids.map(k => k.id).filter(id => cKids.some(k => k.id === id));
    const cOrder = cKids.map(k => k.id).filter(id => dById.has(id));
    if (JSON.stringify(dOrder) !== JSON.stringify(cOrder)) {
      cOrder.forEach((id, i) => {
        if (dOrder[i] !== id) ops.push({ op: 'move', id, parent: c.id, index: i });
      });
    }
    for (const dk of dKids) {
      if (!cKids.some(k => k.id === dk.id)) ops.push({ op: 'remove', id: dk.id });
    }
    cKids.forEach((ck, i) => diff(dById.get(ck.id), ck, c.id, i));
  };

  if (defaults.id !== current.id) {
    // Different roots: reproduce by replacing children + root props.
    propsDiff(
      (defaults.props ?? {}) as Record<string, unknown>,
      (current.props ?? {}) as Record<string, unknown>,
      'props',
      ops,
      current.id,
    );
  }
  diff(defaults, current, '', 0);
  void defIds;
  return ops.filter(o => !(o.op === 'insert' && o.parent === ''));
}

/** Restore one node (subtree) to its shipped-default state. Pure. */
export function resetNode(tree: LayoutNode, defaults: LayoutNode, id: string): { tree: LayoutNode; ok: boolean } {
  const next = cloneNode(tree);
  const def = findNode(defaults, id);
  if (!def) return { tree: next, ok: false };
  if (next.id === id) return { tree: cloneNode(defaults), ok: true };
  const parent = findParent(next, id);
  if (!parent) return { tree: next, ok: false };
  parent.children = (parent.children ?? []).map(c => (c.id === id ? cloneNode(def) : c));
  return { tree: next, ok: true };
}

/** Drop every override: back to shipped defaults. Pure. */
export function resetAll(defaults: LayoutNode): LayoutNode {
  return cloneNode(defaults);
}

export type { BankDef, PageDef, TokenOverrides };
