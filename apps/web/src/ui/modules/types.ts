// L0 Module system: module manifest types (binding contract, MODULES.md §2).
// Presentational components only: props in, callbacks out. Data arrives via
// ModuleContext (see context.ts), resolved by the renderer from fixtures now
// and the engine in wave 3.

import type { Component } from 'svelte';

export type Role = 'engineer' | 'operator' | 'musician';
export type DeviceClass = 'phone' | 'tablet' | 'desktop';

/** Data a module may need, by catalogue id / stream (MODULES.md §2). */
export type Binding =
  | `param:${string}`
  | `meter:${string}`
  | 'strip'
  | 'bank'
  | 'scene'
  | `setting:${string}`;

export type RefKind = 'strip' | 'bank' | 'page' | 'param';

export type PropSchemaField =
  | { type: 'number'; min?: number; max?: number; step?: number; unit?: string; default?: number }
  | { type: 'enum'; options: string[]; default?: string }
  | { type: 'bool'; default?: boolean }
  | { type: 'color'; default?: string }
  | { type: 'text'; default?: string }
  | { type: 'ref'; kind: RefKind; default?: string }
  | { type: 'list'; item: PropSchemaField; default?: unknown[] }
  /** Opaque object bound from scope (e.g. a view-model row); read-only in the inspector. */
  | { type: 'object'; default?: unknown };

/** Typed, validated prop description; drives the props inspector. */
export type PropSchema = Record<string, PropSchemaField>;

export interface ModuleSize {
  minW?: number;
  maxW?: number;
  minH?: number;
  maxH?: number;
  fixedW?: number;
  fixedH?: number;
  flex?: boolean;
}

export type ModuleFallback = 'remove' | 'collapse' | 'disabled';

/** One event a module can emit (MODULES.md §1 + L1 actions). Declared by the
 *  manifest; the renderer supplies `onEvent` and dispatches the node's
 *  `on[event]` action(s). Hot-path modules keep local-first logic and emit
 *  only commit/change events at the edges. */
export interface ModuleEvent {
  /** Event name, e.g. 'press', 'connect'. */
  name: string;
  /** Payload field names the event carries (documentation for the editor). */
  args?: string[];
}

/** Props every module component receives (generic P = its own props). */
export interface ModuleProps<P = unknown> {
  props: P;
  /** Resolved bindings for this instance (see context.ts). */
  ctx: ModuleContext;
  /** Node id of this instance (for edit mode + profile ops). */
  nodeId: string;
  /** Emit a declared event (L1 actions). Absent when the node has no `on`. */
  onEvent?: (name: string, payload?: unknown) => void;
}

/** Live bindings for one module instance. Each binding subscribes to its own
 *  value only, so the renderer never re-renders modules per animation frame. */
export interface ModuleContext {
  /** Snapshot of raw binding strings declared by the manifest. */
  bindings: Binding[];
  /** Capabilities the desk supports (caps ids); unsupported controls follow fallback. */
  caps(): string[];
  role(): Role;
  device(): DeviceClass;
  /** Read a `param:<id>` binding (engine value or fixture equivalent). */
  useParam(ref: string): unknown;
  /** Read a `meter:<tap>` binding (level in dBFS, or null when unknown). */
  useMeter(ref: string): number | null;
  /** Read the `strip` / `bank` binding (fixture channel or bank descriptor). */
  useStrip(ref: string): unknown;
}

export interface ModuleManifest<P = unknown> {
  /** Unique id, e.g. 'fader'. */
  id: string;
  /** Shown in the module palette. */
  title: string;
  category: 'strip' | 'channel' | 'screen' | 'control' | 'layout' | 'display';
  component: Component<ModuleProps<P>>;
  /** Typed prop schema (number/enum/bool/color/text/ref/list). */
  props: PropSchema;
  defaults: P;
  /** Honoured by the renderer and editor. */
  size: ModuleSize;
  /** Data it needs, by catalogue id/stream. */
  binds?: Binding[];
  /** Caps ids; when the desk lacks them the module renders `fallback`. */
  requires?: string[];
  /** Honest UI: never fake values (SPEC principle 2). */
  fallback?: ModuleFallback;
  /** Roles allowed to place/see it (musician profiles lock the layout). */
  roles?: Role[];
  /** Events the module can emit via `onEvent` (L1 actions; optional). */
  events?: ModuleEvent[];
}
