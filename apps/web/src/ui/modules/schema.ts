// L0 Module system: layout node schema (MODULES.md §3) + pure helpers.
// Validators live in validate.ts. Nothing here touches the DOM.

import type { DeviceClass, Role } from './types.ts';
import type { ActionTarget } from './actions.ts';

export type RepeatOver = `bank:${string}` | 'strips' | `list:${string}` | string;

/** Conditions gating a node's visibility. Object form: all present keys must
 *  pass. String form (L1): an expression in the safe language, e.g.
 *  `conn.isConnected && caps.has('connect')`, truthy = visible. */
export type WhenClause =
  | {
      device?: DeviceClass | DeviceClass[];
      role?: Role | Role[];
      /** Caps ids that must ALL be present on the desk. */
      caps?: string[];
      /** `setting:<key>` binding ids that must ALL be truthy. */
      setting?: string[];
    }
  | string;

/** Per-device-class structural variants; the renderer picks by container width. */
export type DeviceVariants = Partial<Record<DeviceClass, Partial<LayoutNode>>>;

export interface LayoutNode {
  id: string;
  /** Module id (container ids: row/col/grid/stack/scroll/tabs/repeat, or any registered module). */
  module: string;
  props?: Record<string, unknown>;
  /** Prop name -> value: a legacy catalogue binding (`param:<id>`, bare
   *  `strip`/`bank`/`scene`, `setting:<key>`) or an L1 safe-language
   *  expression over the scope (`store.mode`, `item.id`, `index`, ...). */
  bind?: Record<string, string>;
  /** Event name -> action ref(s) run through the action registry (L1). */
  on?: Record<string, ActionTarget>;
  children?: LayoutNode[];
  /** Named slot this node fills inside a slotted parent (reserved; default trees ignore it). */
  slot?: string;
  when?: WhenClause;
  variants?: DeviceVariants;
  /** Editor hide flag (profile `hide` op); the renderer skips hidden nodes. */
  hidden?: boolean;
}

/** A layout tree = root container node (screens/pages ship one each). */
export type LayoutTree = LayoutNode;

/** Container module ids implemented by ui/modules/layout/*.svelte. */
export const CONTAINERS = new Set(['row', 'col', 'grid', 'stack', 'scroll', 'tabs', 'repeat']);

/** Node shown when a saved profile references an unknown module id: the
 *  profile still loads, and the editor flags it "not available on this build". */
export const MISSING_MODULE = 'missing-module';

export function cloneNode<T>(node: T): T {
  return JSON.parse(JSON.stringify(node)) as T;
}

function asList(v: string | string[] | undefined): string[] | null {
  if (v === undefined) return null;
  return Array.isArray(v) ? v : [v];
}

/**
 * Evaluate a `when` clause. Pure; missing context means "unknown" -> the node
 * is kept (honest UI: we never hide user content on missing data, except for
 * caps, which are authoritative about desk support). String clauses (L1) are
 * expressions evaluated by `evalWhenExpr` in RenderNode; here only the object
 * form is handled — strings evaluate to "unknown, keep".
 */
export function evalWhen(
  when: WhenClause | undefined,
  ctx: { device?: DeviceClass; role?: Role; caps?: string[]; settings?: Record<string, unknown> },
): boolean {
  if (!when) return true;
  if (typeof when === 'string') return true;
  const devices = asList(when.device);
  if (devices && (!ctx.device || !devices.includes(ctx.device))) return false;
  const roles = asList(when.role);
  if (roles && (!ctx.role || !roles.includes(ctx.role))) return false;
  if (when.caps && when.caps.length > 0) {
    const caps = ctx.caps ?? [];
    if (!when.caps.every(c => caps.includes(c))) return false;
  }
  if (when.setting && when.setting.length > 0) {
    for (const key of when.setting) {
      const name = key.startsWith('setting:') ? key.slice('setting:'.length) : key;
      if (!(ctx.settings ?? {})[name]) return false;
    }
  }
  return true;
}

/** Pick the device-class variant overlay for a node (undefined = no override). */
export function selectVariant(node: LayoutNode, device: DeviceClass): Partial<LayoutNode> | undefined {
  return node.variants?.[device];
}

/** Apply the device variant overlay to a node (shallow merge of props/bind, replace children when given). */
export function applyVariant(node: LayoutNode, device: DeviceClass): LayoutNode {
  const v = selectVariant(node, device);
  if (!v) return node;
  return {
    ...node,
    ...v,
    props: { ...(node.props ?? {}), ...((v.props as Record<string, unknown>) ?? {}) },
    bind: { ...(node.bind ?? {}), ...((v.bind as Record<string, unknown> as Record<string, string>) ?? {}) },
  };
}

export interface RepeatScope {
  bankItems?: string[];
  strips?: string[];
  /** Named lists for `over: 'list:<key>'`. String items (strip ids) or view-
   *  model row objects; object items land in `props.item` so bindings like
   *  `item.id` work, and the instance id uses `item.id ?? index`. */
  lists?: Record<string, unknown[]>;
  settings?: Record<string, unknown>;
  /** Nested-renderer instance fields (e.g. strip-frame re-rooting): carried
   *  through for expression scope, ignored by repeat expansion. */
  store?: unknown;
  item?: unknown;
  index?: number;
  frozen?: boolean;
}

/**
 * Expand a `repeat` node into one instance of its child template per item in
 * `over`. Pure (used by the renderer and by tests). Non-repeat nodes return
 * [node]. The template is `children[0]`; extra children are ignored.
 * Each instance gets id `${node.id}[${item}]`, the item in
 * `props.item`, the position in `props.index`, and (for strip repeats)
 * `bind.stripId = 'strip:<item>'`. `over: 'list:<key>'` reads scope lists;
 * object items (view-model rows) pass through as `props.item` so bindings
 * like `item.id` work; `when` strings also see `item`/`index`.
 */
export function expandRepeat(node: LayoutNode, scope: RepeatScope): LayoutNode[] {
  if (node.module !== 'repeat') return [node];
  const over = (node.props?.['over'] as string | undefined) ?? 'strips';
  let items: unknown[] = [];
  if (over === 'strips') items = scope.strips ?? [];
  else if (over.startsWith('bank:')) items = scope.bankItems ?? [];
  else if (over.startsWith('list:')) items = scope.lists?.[over.slice(5)] ?? [];
  else items = scope.lists?.[over] ?? [];
  const template = node.children?.[0];
  if (!template) return [];
  return items.map((item, index) => {
    const inst = cloneNode(template);
    const key = typeof item === 'string' ? item : String((item as Record<string, unknown>)?.['id'] ?? index);
    inst.id = `${node.id}[${key}]`;
    inst.props = { ...(inst.props ?? {}), item, index };
    if ((over === 'strips' || over.startsWith('bank:')) && typeof item === 'string') {
      inst.bind = { ...(inst.bind ?? {}), stripId: `strip:${item}` };
    }
    return inst;
  });
}

/** Depth-first find a node by id (undefined when absent). */
export function findNode(root: LayoutNode, id: string): LayoutNode | undefined {
  if (root.id === id) return root;
  for (const c of root.children ?? []) {
    const hit = findNode(c, id);
    if (hit) return hit;
  }
  return undefined;
}

/** Depth-first find a node's parent (undefined for the root / when absent). */
export function findParent(root: LayoutNode, id: string): LayoutNode | undefined {
  for (const c of root.children ?? []) {
    if (c.id === id) return root;
    const hit = findParent(c, id);
    if (hit) return hit;
  }
  return undefined;
}

/** Count nodes in a tree (repeat templates count once, unexpanded). */
export function countNodes(root: LayoutNode): number {
  return 1 + (root.children ?? []).reduce((s, c) => s + countNodes(c), 0);
}
