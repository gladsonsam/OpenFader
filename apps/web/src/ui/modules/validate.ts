// L0 Module system: layout validators. NEVER throw on bad input — every
// entry returns { ok, errors[] }. A lenient normaliser rewrites unknown
// modules into a visible `missing-module` placeholder so saved profiles built
// for another build/desk still load.

import { CONTAINERS, MISSING_MODULE, cloneNode } from './schema.ts';
import type { LayoutNode, WhenClause } from './schema.ts';
import { getAction } from './actions.ts';
import { isLegacyBinding, parseExpr } from './expr.ts';

export interface Validation {
  ok: boolean;
  errors: string[];
  /** Non-fatal notes (unknown action ids, ...): the node still renders. */
  warnings: string[];
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function checkBind(bind: unknown, path: string, errors: string[]): void {
  if (bind === undefined) return;
  if (!isRecord(bind)) {
    errors.push(`${path}.bind must be an object`);
    return;
  }
  for (const [k, v] of Object.entries(bind)) {
    if (typeof v !== 'string') {
      errors.push(`${path}.bind.${k} must be a binding string`);
      continue;
    }
    if (isLegacyBinding(v)) continue;
    const r = parseExpr(v);
    if (!r.ok) errors.push(`${path}.bind.${k}: bad expression: ${r.error}`);
  }
}

function checkOnRef(ref: unknown, path: string, errors: string[], warnings: string[]): void {
  if (!isRecord(ref) || typeof ref['do'] !== 'string' || (ref['do'] as string).length === 0) {
    errors.push(`${path} must be { do: '<actionId>', args?: {...} }`);
    return;
  }
  const id = ref['do'] as string;
  const entry = getAction(id);
  if (!entry) {
    // Unknown action ids are a visible warning, never fatal: the node keeps
    // rendering and the event is skipped at dispatch (editor flags it).
    warnings.push(`${path}: unknown action "${id}"`);
  }
  if (ref['args'] !== undefined) {
    if (!isRecord(ref['args'])) {
      errors.push(`${path}.args must be an object`);
    } else {
      checkActionArgs(id, ref['args'] as Record<string, unknown>, path + '.args', errors);
      const schema = entry?.meta.args;
      if (schema) {
        for (const [name, want] of Object.entries(schema)) {
          const v = (ref['args'] as Record<string, unknown>)[name];
          if (v === undefined || (isRecord(v) && '$expr' in v)) continue;
          if (want === 'number' && typeof v !== 'number') errors.push(`${path}.args.${name} must be number`);
          if (want === 'string' && typeof v !== 'string') errors.push(`${path}.args.${name} must be string`);
          if (want === 'boolean' && typeof v !== 'boolean') errors.push(`${path}.args.${name} must be boolean`);
        }
      }
    }
  }
}

function checkActionArgs(
  _id: string,
  args: Record<string, unknown>,
  path: string,
  errors: string[],
): void {
  const check = (v: unknown, p: string): void => {
    if (Array.isArray(v)) {
      v.forEach((x, i) => check(x, `${p}[${i}]`));
      return;
    }
    if (!isRecord(v)) return;
    const keys = Object.keys(v);
    if ('$expr' in v) {
      if (keys.length !== 1 || typeof v['$expr'] !== 'string') {
        errors.push(`${p}: $expr refs must be { $expr: '<expr>' }`);
        return;
      }
      const r = parseExpr(v['$expr'] as string);
      if (!r.ok) errors.push(`${p}.$expr: bad expression: ${r.error}`);
      return;
    }
    for (const [k, x] of Object.entries(v)) {
      if (k.startsWith('$')) {
        errors.push(`${p}.${k}: unknown $ directive`);
        continue;
      }
      check(x, `${p}.${k}`);
    }
  };
  for (const [k, v] of Object.entries(args)) check(v, `${path}.${k}`);
}

function checkOn(on: unknown, path: string, errors: string[], warnings: string[]): void {
  if (on === undefined) return;
  if (!isRecord(on)) {
    errors.push(`${path}.on must be an object`);
    return;
  }
  for (const [event, target] of Object.entries(on)) {
    if (event.length === 0) {
      errors.push(`${path}.on has an empty event name`);
      continue;
    }
    const steps = Array.isArray(target) ? target : [target];
    if (steps.length === 0) {
      errors.push(`${path}.on.${event} must not be an empty array`);
      continue;
    }
    steps.forEach((s, i) => checkOnRef(s, `${path}.on.${event}${steps.length > 1 ? `[${i}]` : ''}`, errors, warnings));
  }
}

const DEVICES = new Set(['phone', 'tablet', 'desktop']);
const ROLES = new Set(['engineer', 'operator', 'musician']);

function checkWhen(when: unknown, path: string, errors: string[]): void {
  if (when === undefined) return;
  // L1 string form: an expression in the safe language.
  if (typeof when === 'string') {
    if (!isLegacyBinding(when)) {
      const r = parseExpr(when);
      if (!r.ok) errors.push(`${path}.when: bad expression: ${r.error}`);
    }
    return;
  }
  if (!isRecord(when)) {
    errors.push(`${path}.when must be an object`);
    return;
  }
  const w = when as Record<string, unknown>;
  for (const key of ['device', 'role'] as const) {
    const v = w[key];
    if (v === undefined) continue;
    const list = Array.isArray(v) ? v : [v];
    const pool = key === 'device' ? DEVICES : ROLES;
    for (const x of list) {
      if (typeof x !== 'string' || !pool.has(x)) errors.push(`${path}.when.${key} has invalid value ${JSON.stringify(x)}`);
    }
  }
  if (w['caps'] !== undefined && (!Array.isArray(w['caps']) || (w['caps'] as unknown[]).some(c => typeof c !== 'string'))) {
    errors.push(`${path}.when.caps must be string[]`);
  }
  if (w['setting'] !== undefined && (!Array.isArray(w['setting']) || (w['setting'] as unknown[]).some(c => typeof c !== 'string'))) {
    errors.push(`${path}.when.setting must be string[]`);
  }
}

/** Validate one layout node recursively (cycle-safe via the seen set of ancestor ids). */
export function validateNode(node: unknown, path = 'root', ancestors: string[] = []): Validation {
  const errors: string[] = [];
  const warnings: string[] = [];
  if (!isRecord(node)) return { ok: false, errors: [`${path} must be an object`], warnings };
  const n = node as Record<string, unknown>;
  if (typeof n['id'] !== 'string' || (n['id'] as string).length === 0) {
    errors.push(`${path}.id must be a non-empty string`);
  } else if (ancestors.includes(n['id'] as string)) {
    return { ok: false, errors: [`${path}: cycle at id "${n['id'] as string}"`], warnings };
  }
  if (typeof n['module'] !== 'string' || (n['module'] as string).length === 0) {
    errors.push(`${path}.module must be a non-empty string`);
  }
  if (n['props'] !== undefined && !isRecord(n['props'])) errors.push(`${path}.props must be an object`);
  checkBind(n['bind'], path, errors);
  checkOn(n['on'], path, errors, warnings);
  if (n['slot'] !== undefined && typeof n['slot'] !== 'string') errors.push(`${path}.slot must be a string`);
  checkWhen(n['when'], path, errors);
  if (n['variants'] !== undefined) {
    if (!isRecord(n['variants'])) errors.push(`${path}.variants must be an object`);
    else {
      for (const [dev, v] of Object.entries(n['variants'])) {
        if (!DEVICES.has(dev)) errors.push(`${path}.variants has invalid device "${dev}"`);
        else if (!isRecord(v)) errors.push(`${path}.variants.${dev} must be an object`);
      }
    }
  }
  const kids = n['children'];
  if (kids !== undefined) {
    if (!Array.isArray(kids)) errors.push(`${path}.children must be an array`);
    else {
      const ids = new Set<string>();
      const nextAnc = typeof n['id'] === 'string' ? [...ancestors, n['id'] as string] : ancestors;
      kids.forEach((k, i) => {
        const r = validateNode(k, `${path}.children[${i}]`, nextAnc);
        errors.push(...r.errors);
        warnings.push(...r.warnings);
        if (isRecord(k) && typeof k['id'] === 'string') {
          if (ids.has(k['id'] as string)) errors.push(`${path}: duplicate child id "${k['id'] as string}"`);
          ids.add(k['id'] as string);
        }
      });
    }
  }
  if (n['module'] === 'repeat' && (!Array.isArray(kids) || kids.length === 0)) {
    errors.push(`${path}: repeat needs exactly one child template`);
  }
  // Bubble child warnings up so editors can surface every unknown action id.
  return { ok: errors.length === 0, errors, warnings };
}

/** Validate a whole layout tree. */
export function validateLayout(tree: unknown): Validation {
  return validateNode(tree, 'root', []);
}

/**
 * Lenient normaliser: returns a renderable tree plus the list of problems
 * fixed. Unknown module ids become visible `missing-module` placeholders
 * (keeping id/props); malformed children are dropped; missing ids are
 * synthesised. Never throws — garbage in yields a placeholder root.
 */
export function normalizeLayout(tree: unknown, known: (id: string) => boolean): { tree: LayoutNode; fixed: string[] } {
  const fixed: string[] = [];
  let counter = 0;
  const synth = (): string => `node-${counter++}`;
  function norm(node: unknown, path: string): LayoutNode | null {
    if (!isRecord(node)) {
      fixed.push(`${path}: dropped non-object child`);
      return null;
    }
    const id = typeof node['id'] === 'string' && node['id'].length > 0 ? (node['id'] as string) : synth();
    if (id !== node['id']) fixed.push(`${path}: synthesised missing id -> ${id}`);
    let module = typeof node['module'] === 'string' && node['module'].length > 0 ? (node['module'] as string) : '';
    const original = module;
    if (!module) {
      fixed.push(`${id}: empty module -> ${MISSING_MODULE}`);
      module = MISSING_MODULE;
    } else if (!CONTAINERS.has(module) && !known(module)) {
      fixed.push(`${id}: unknown module "${module}" -> ${MISSING_MODULE}`);
      module = MISSING_MODULE;
    }
    const out: LayoutNode = { id, module };
    if (isRecord(node['props'])) out.props = cloneNode(node['props']) as Record<string, unknown>;
    if (module === MISSING_MODULE && original) {
      out.props = { ...(out.props ?? {}), missing: original };
    }
    if (isRecord(node['bind'])) {
      out.bind = {};
      for (const [k, v] of Object.entries(node['bind'] as Record<string, unknown>)) {
        if (typeof v === 'string') out.bind[k] = v;
        else fixed.push(`${id}.bind.${k}: dropped non-string binding`);
      }
    }
    // Actions travel with the node (unknown ids stay: dispatch warns once and
    // skips the event; the editor flags them from validate warnings).
    if (isRecord(node['on'])) {
      try {
        out.on = cloneNode(node['on']) as LayoutNode['on'];
      } catch {
        fixed.push(`${id}.on: dropped uncloneable actions`);
      }
    }
    if (typeof node['slot'] === 'string') out.slot = node['slot'];
    if (node['hidden'] === true) out.hidden = true;
    if (isRecord(node['when']) || typeof node['when'] === 'string') {
      out.when = cloneNode(node['when']) as unknown as WhenClause;
    }
    if (isRecord(node['variants'])) {
      out.variants = cloneNode(node['variants']) as LayoutNode['variants'];
    }
    if (Array.isArray(node['children'])) {
      const kids: LayoutNode[] = [];
      node['children'].forEach((k, i) => {
        const n = norm(k, `${id}.children[${i}]`);
        if (n) kids.push(n);
      });
      if (kids.length > 0 || module !== MISSING_MODULE) out.children = kids;
    }
    return out;
  }
  const root = norm(tree, 'root');
  if (!root) return { tree: { id: 'root', module: MISSING_MODULE, props: {} }, fixed };
  return { tree: root, fixed };
}
