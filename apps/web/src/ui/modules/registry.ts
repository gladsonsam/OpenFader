// L0 Module system: module registry (MODULES.md §1 + extension API §4).
//
// Extension API (for other packages): drop a manifest file in
// `ui/modules/manifests/<yourpackage>.ts` that exports one manifest, an array
// of manifests, or named `manifest`/`manifests` exports. `loadManifestModules()`
// picks them up via `import.meta.glob` (eager) — no other wiring needed.
// See `manifests/_example.ts` for the exact shape.
//
// The registry itself never imports .svelte files statically, so it stays
// loadable by plain node tests.

import type { ModuleManifest } from './types.ts';

const byId = new Map<string, ModuleManifest>();

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

const CATEGORIES = new Set(['strip', 'channel', 'screen', 'control', 'layout', 'display']);

/** Validate a manifest without throwing. Returns error strings (empty = ok). */
export function validateManifest(m: unknown): string[] {
  const errors: string[] = [];
  if (!isRecord(m)) return ['manifest must be an object'];
  if (typeof m['id'] !== 'string' || (m['id'] as string).length === 0) {
    errors.push('manifest.id must be a non-empty string');
  } else if (!/^[a-z0-9][a-z0-9-]*$/.test(m['id'] as string)) {
    errors.push(`manifest.id "${m['id'] as string}" must be kebab-case [a-z0-9-]`);
  }
  if (typeof m['title'] !== 'string' || (m['title'] as string).length === 0) {
    errors.push('manifest.title must be a non-empty string');
  }
  if (typeof m['category'] !== 'string' || !CATEGORIES.has(m['category'] as string)) {
    errors.push(`manifest.category must be one of ${[...CATEGORIES].join('|')}`);
  }
  if (typeof m['component'] !== 'function' && typeof m['component'] !== 'object') {
    errors.push('manifest.component must be a Svelte component');
  }
  if (!isRecord(m['props'])) errors.push('manifest.props must be a PropSchema object');
  if (m['defaults'] === undefined) errors.push('manifest.defaults must be present');
  if (!isRecord(m['size'])) errors.push('manifest.size must be an object');
  if (m['binds'] !== undefined && !Array.isArray(m['binds'])) errors.push('manifest.binds must be an array');
  if (m['requires'] !== undefined && !Array.isArray(m['requires'])) errors.push('manifest.requires must be an array');
  if (
    m['fallback'] !== undefined &&
    m['fallback'] !== 'remove' &&
    m['fallback'] !== 'collapse' &&
    m['fallback'] !== 'disabled'
  ) {
    errors.push('manifest.fallback must be remove|collapse|disabled');
  }
  if (m['roles'] !== undefined) {
    const roles = m['roles'];
    if (!Array.isArray(roles) || roles.some(r => r !== 'engineer' && r !== 'operator' && r !== 'musician')) {
      errors.push('manifest.roles must be an array of engineer|operator|musician');
    }
  }
  if (m['events'] !== undefined) {
    const ev = m['events'];
    if (!Array.isArray(ev)) {
      errors.push('manifest.events must be an array');
    } else {
      ev.forEach((e: unknown, i: number) => {
        if (typeof e !== 'object' || e === null || typeof (e as Record<string, unknown>)['name'] !== 'string' || ((e as Record<string, unknown>)['name'] as string).length === 0) {
          errors.push(`manifest.events[${i}].name must be a non-empty string`);
        }
        const args = (e as Record<string, unknown>)?.['args'];
        if (args !== undefined && (!Array.isArray(args) || (args as unknown[]).some(a => typeof a !== 'string'))) {
          errors.push(`manifest.events[${i}].args must be string[]`);
        }
      });
    }
  }
  return errors;
}

/**
 * Register a module manifest. Returns { ok:true } on success, or
 * { ok:false, errors } — never throws. Duplicate ids are rejected; the
 * stored manifest is frozen so later edits cannot mutate the registry.
 */
export function registerModule(manifest: ModuleManifest): { ok: true } | { ok: false; errors: string[] } {
  const errors = validateManifest(manifest);
  if (errors.length > 0) return { ok: false, errors };
  if (byId.has(manifest.id)) {
    return { ok: false, errors: [`duplicate module id: "${manifest.id}"`] };
  }
  byId.set(manifest.id, Object.freeze({ ...manifest }));
  return { ok: true };
}

/** Look up a manifest by module id (undefined when unknown). */
export function getModule(id: string): ModuleManifest | undefined {
  return byId.get(id);
}

/** All manifests, optionally filtered by category, in registration order. */
export function listModules(category?: ModuleManifest['category']): ModuleManifest[] {
  const all = [...byId.values()];
  return category ? all.filter(m => m.category === category) : all;
}

/**
 * Module palette query helper for Edit mode: free-text match on id/title
 * plus optional category filter. Never throws; empty query returns all.
 */
export function queryPalette(query: string, category?: ModuleManifest['category']): ModuleManifest[] {
  const q = query.trim().toLowerCase();
  return listModules(category).filter(
    m => q === '' || m.id.toLowerCase().includes(q) || m.title.toLowerCase().includes(q),
  );
}

/** Remove all registrations (tests only). */
export function clearRegistry(): void {
  byId.clear();
}

/**
 * Load every manifest file under `ui/modules/manifests/*.ts` (eager glob).
 * Each file may `export default manifest | manifest[]`, or named `manifest` /
 * `manifests`. Call once at startup (browser only — never call from node
 * tests, since manifests import .svelte components). Returns per-file results.
 */
export function loadManifestModules(): Array<{ file: string; registered: string[]; errors: string[] }> {
  const files = import.meta.glob('./manifests/*.ts', { eager: true }) as Record<string, unknown>;
  const out: Array<{ file: string; registered: string[]; errors: string[] }> = [];
  for (const [file, mod] of Object.entries(files)) {
    const registered: string[] = [];
    const errors: string[] = [];
    const found: unknown[] = [];
    if (isRecord(mod)) {
      const def = (mod as Record<string, unknown>)['default'];
      if (def !== undefined) found.push(...(Array.isArray(def) ? def : [def]));
      const named = (mod as Record<string, unknown>)['manifest'];
      if (named !== undefined) found.push(named);
      const namedPl = (mod as Record<string, unknown>)['manifests'];
      if (namedPl !== undefined) found.push(...(Array.isArray(namedPl) ? namedPl : [namedPl]));
    }
    for (const m of found) {
      const r = registerModule(m as ModuleManifest);
      if (r.ok) registered.push((m as ModuleManifest).id);
      else errors.push(...r.errors.map(e => `${file}: ${e}`));
    }
    out.push({ file, registered, errors });
  }
  return out;
}
