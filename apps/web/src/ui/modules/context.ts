// L0 ModuleContext providers (MODULES.md §2).
//
// FixtureContext: resolves bindings from fixtures/designData + uiStore +
// meterSim NOW (fixture mode, wave 0-2). EngineContext (wave 3, NOT
// implemented here) will resolve the same interface from lib/backend +
// paramStore + caps. Modules only ever see ModuleContext, so they work on
// fixtures, the sim and a real desk unchanged.

import type { Binding, DeviceClass, ModuleContext, Role } from './types.ts';

export interface ContextSource {
  role?: Role;
  device?: DeviceClass;
  caps?: string[];
  settings?: Record<string, unknown>;
  param?: (ref: string) => unknown;
  meter?: (ref: string) => number | null;
  strip?: (ref: string) => unknown;
}

function parseRef(binding: string): { kind: string; id: string } {
  const i = binding.indexOf(':');
  if (i < 0) return { kind: binding, id: '' };
  return { kind: binding.slice(0, i), id: binding.slice(i + 1) };
}

/** Build a ModuleContext from a source. Pure assembly; reads are delegated. */
export function makeContext(bindings: string[], source: ContextSource): ModuleContext {
  return {
    bindings: bindings as Binding[],
    caps: () => source.caps ?? [],
    role: () => source.role ?? 'engineer',
    device: () => source.device ?? 'desktop',
    useParam: (ref: string) => source.param?.(parseRef(ref).id) ?? null,
    useMeter: (ref: string) => source.meter?.(parseRef(ref).id) ?? null,
    useStrip: (ref: string) => {
      const { kind, id } = parseRef(ref);
      if (kind === 'strip') return source.strip?.(id) ?? null;
      return source.strip?.(ref) ?? null;
    },
  };
}

/**
 * Fixture-backed source (wave 0-2). Callers pass fixture getters; defaults
 * read nothing (null) so the context is safe before the store exists.
 */
export function fixtureSource(over: Partial<ContextSource> = {}): ContextSource {
  return {
    role: 'engineer',
    device: 'desktop',
    caps: [],
    settings: {},
    ...over,
  };
}

/** Wave-3 engine source: same interface, backed by backend+caps. NOT implemented. */
export interface EngineSourceOptions {
  backend: unknown;
  role?: Role;
  device?: DeviceClass;
}

export function engineSource(_opts: EngineSourceOptions): ContextSource {
  throw new Error('engineSource is wave 3 (not implemented in L0)');
}
