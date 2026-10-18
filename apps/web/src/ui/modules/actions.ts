// L1 Actions + bindings: action registry, arg resolution and dispatch.
//
// A layout node may carry `on: { <event>: <ActionRef | ActionRef[]> }` where
//   ActionRef = { do: '<actionId>', args?: { <name>: <literal | { $expr }> } }
// Arg values are literals, or binding refs `{ $expr: '<expr>' }` evaluated in
// the node's expression scope (which includes `payload`, `item`, `index`).
//
// Namespaces:
//   ui.*      every store action the existing screens call (through ui.do, so
//             lock/muteEnable rules apply exactly as in the plain screens)
//   nav.*     navigation aliases (page/back/tab/bank)
//   profile.* profile ops over the bound profile store (applyOp/undo/redo/reset)
//   engine.*  documented placeholder wave 3 will implement (warns + no-ops)
//   demo.*    reserved for the L0 demo page (registered by Demo.svelte)
//
// Browser wiring: builtins are registered once at import; the live store is
// attached with bindUiActions({ do }, { profile }) by each screen before it
// renders. Node tests use the same registry with fake stores. Nothing here
// imports Svelte or the store, so this file is plain-node testable.
//
// Run: node --experimental-strip-types apps/web/src/ui/modules/actions.test.ts

import { evalExpr, type Scope } from './expr.ts';

export interface ActionRef {
  do: string;
  args?: Record<string, unknown>;
}

export type ActionTarget = ActionRef | ActionRef[];

/** Minimal ui.do surface handlers run through (the real uiStore in the
 *  browser, a fake in tests). Index signature keeps this decoupled. */
export interface UiDo {
  [name: string]: (...args: never[]) => unknown;
}

/** Minimal profile-store surface for profile.* actions. */
export interface ProfileLike {
  commit(op: unknown): boolean;
  undo(): boolean;
  redo(): boolean;
  reset(): void;
}

export interface ActionCtx {
  ui: UiDo;
  profile?: ProfileLike;
  warn: (msg: string) => void;
}

export type ActionHandler = (args: Record<string, unknown>, ctx: ActionCtx) => unknown;

export interface ActionMeta {
  /** arg name -> expected primitive ('string'|'number'|'boolean'|'any'). */
  args?: Record<string, 'string' | 'number' | 'boolean' | 'any'>;
  describe?: string;
}

interface Entry {
  handler: ActionHandler;
  meta: ActionMeta;
}

const registry = new Map<string, Entry>();

/** Live stores bound by the current screen (set by bindUiActions). */
let liveUi: UiDo | null = null;
let liveProfile: ProfileLike | null = null;
const warnedOnce = new Set<string>();
const MAX_WARNS = 500;

function warnOnce(key: string, msg: string, warn?: (m: string) => void): void {
  if (warnedOnce.has(key) || warnedOnce.size >= MAX_WARNS) return;
  warnedOnce.add(key);
  (warn ?? defaultWarn)(msg);
}

function defaultWarn(msg: string): void {
  console.warn('[ui-action]', msg);
}

export function clearActionWarnings(): void {
  warnedOnce.clear();
}

/**
 * Register an action id. Overwriting is allowed (last wins) so HMR and
 * screen switches re-binding builtins never fail. Never throws.
 */
export function registerAction(id: string, handler: ActionHandler, meta: ActionMeta = {}): boolean {
  if (typeof id !== 'string' || id.length === 0 || typeof handler !== 'function') return false;
  registry.set(id, { handler, meta });
  return true;
}

/** Look up one action (undefined when unknown). */
export function getAction(id: string): Entry | undefined {
  return registry.get(id);
}

/** All registered actions for the editor palette, in registration order. */
export function listActions(): Array<{ id: string; args?: ActionMeta['args']; describe?: string }> {
  return [...registry.entries()].map(([id, e]) => ({ id, args: e.meta.args, describe: e.meta.describe }));
}

/** Remove all registrations (tests only; call registerBuiltins() after). */
export function clearActions(): void {
  registry.clear();
}

/**
 * Attach the live stores handlers run through. Call once per screen before
 * rendering (idempotent; each screen re-binds its own profile store).
 */
export function bindUiActions(ui: UiDo, opts: { profile?: ProfileLike } = {}): void {
  liveUi = ui;
  liveProfile = opts.profile ?? null;
}

/** Resolve one action's args: `{ $expr }` refs evaluate in scope; plain
 *  values pass through; arrays / nested objects resolve recursively. */
export function resolveArgs(
  args: Record<string, unknown> | undefined,
  scope: Scope,
  onError: (msg: string) => void,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (!args) return out;
  const resolve = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(resolve);
    if (typeof v === 'object' && v !== null) {
      const rec = v as Record<string, unknown>;
      const keys = Object.keys(rec);
      if (keys.length === 1 && keys[0] === '$expr') {
        const expr = rec['$expr'];
        if (typeof expr !== 'string') {
          onError(`$expr ref must be a string, got ${typeof expr}`);
          return undefined;
        }
        const r = evalExpr(expr, scope);
        if (!r.ok) {
          onError(`arg $expr failed: ${expr}: ${r.error}`);
          return undefined;
        }
        return r.value;
      }
      const o: Record<string, unknown> = {};
      for (const [k, x] of Object.entries(rec)) {
        if (k.startsWith('$')) {
          onError(`unknown $ directive ${JSON.stringify(k)}`);
          continue;
        }
        o[k] = resolve(x);
      }
      return o;
    }
    return v;
  };
  for (const [k, v] of Object.entries(args)) out[k] = resolve(v);
  return out;
}

function checkArgTypes(id: string, meta: ActionMeta, args: Record<string, unknown>): string | null {
  if (!meta.args) return null;
  for (const [name, want] of Object.entries(meta.args)) {
    const v = args[name];
    if (v === undefined) continue; // optional until the handler decides
    if (want === 'any') continue;
    if (want === 'number' && typeof v !== 'number') return `${id}: arg ${name} must be number`;
    if (want === 'string' && typeof v !== 'string') return `${id}: arg ${name} must be string`;
    if (want === 'boolean' && typeof v !== 'boolean') return `${id}: arg ${name} must be boolean`;
  }
  return null;
}

export interface DispatchResult {
  ok: boolean;
  error?: string;
}

/**
 * Run one ActionRef or a sequence. Each step resolves `$expr` args in
 * `scope`, type-checks them against the action's arg schema, then calls the
 * handler with (args, ctx). Unknown ids warn once and are skipped; a handler
 * that throws warns and STOPS the sequence. Never throws.
 */
import { tap } from '../primitives/feel.ts';

/** Actions that must stay silent: they fire at pointermove/keystroke rates
 *  (fader/pan/knob drags, name/host typing), where a tick per event would
 *  buzz constantly. Everything else ticks on success — every tap anywhere
 *  in the app gets haptic confirmation without per-action wiring. */
const SILENT_ACTIONS = new Set([
  'ui.setFader',
  'ui.setStripFader',
  'ui.setSend',
  'ui.setPan',
  'ui.setEqParam',
  'ui.setDynParam',
  'ui.setFxParam',
  'ui.setInputKnob',
  'ui.setChannelName',
  'ui.setRelayHost',
]);

export function dispatchAction(
  target: ActionTarget | undefined,
  scope: Scope,
  ctx?: Partial<ActionCtx>,
): DispatchResult {
  if (!target) return { ok: true };
  const full: ActionCtx = {
    ui: ctx?.ui ?? liveUi ?? {},
    profile: ctx?.profile ?? liveProfile ?? undefined,
    warn: ctx?.warn ?? defaultWarn,
  };
  const steps = Array.isArray(target) ? target : [target];
  for (const step of steps) {
    if (typeof step !== 'object' || step === null || typeof (step as ActionRef).do !== 'string') {
      full.warn(`bad action ref: ${JSON.stringify(step)}`);
      return { ok: false, error: 'bad action ref' };
    }
    const ref = step as ActionRef;
    const entry = registry.get(ref.do);
    if (!entry) {
      warnOnce(`unknown:${ref.do}`, `unknown action "${ref.do}" (kept node, skipped event)`, full.warn);
      continue;
    }
    const args = resolveArgs(ref.args, scope, m => full.warn(`${ref.do}: ${m}`));
    const typeErr = checkArgTypes(ref.do, entry.meta, args);
    if (typeErr) {
      full.warn(typeErr);
      return { ok: false, error: typeErr };
    }
    try {
      entry.handler(args, full);
      if (!SILENT_ACTIONS.has(ref.do)) tap();
    } catch (e) {
      const msg = `${ref.do} threw: ${e instanceof Error ? e.message : String(e)}`;
      full.warn(msg);
      return { ok: false, error: msg };
    }
  }
  return { ok: true };
}

// ---------- builtins ----------

function uiThrough(name: string, pick: (a: Record<string, unknown>) => unknown[], meta: ActionMeta): void {
  registerAction(
    name,
    (args, ctx) => {
      const fn = ctx.ui[name.slice(3)];
      if (typeof fn !== 'function') {
        ctx.warn(`store has no "${name.slice(3)}" (ui.* action "${name}" skipped)`);
        return;
      }
      (fn as (...a: unknown[]) => unknown)(...pick(args));
    },
    meta,
  );
}

function clockNow(): string {
  const n = new Date();
  return String(n.getHours()).padStart(2, '0') + ':' + String(n.getMinutes()).padStart(2, '0');
}

/** Register all ui.* / nav.* / profile.* / engine.* builtins (idempotent). */
export function registerBuiltins(): void {
  const A = (d?: string): ActionMeta => (d ? { describe: d } : {});
  const S = (d: string): ActionMeta => ({ args: { id: 'string' } as const, describe: d });
  const N = (d: string): ActionMeta => ({ args: { value: 'any' } as const, describe: d });

  // navigation + pages (every route the shell/screens use)
  uiThrough('ui.openPage', a => [a['mode']], { args: { mode: 'string' }, describe: 'Open a page (mixer/channel/fx/routing/connect/settings/scenes)' });
  uiThrough('ui.goFx', () => [], A('Open the FX page'));
  uiThrough('ui.goRouting', () => [], A('Open the Routing page'));
  uiThrough('ui.goConnect', () => [], A('Open the Connection page'));
  uiThrough('ui.goSettings', () => [], A('Open the Settings page'));
  uiThrough('ui.goOffline', () => [], A('Work offline'));
  uiThrough('ui.goBack', () => [], A('Go back to the previous mode'));
  uiThrough('ui.openScenes', () => [], A('Open the Scenes page'));
  uiThrough('ui.goPatch', a => [a['id']], { args: { id: 'string' }, describe: 'Open routing patch for channel' });
  registerAction('nav.page', (a, c) => dispatchAction({ do: 'ui.openPage', args: { mode: a['mode'] } }, {}, c), { args: { mode: 'string' }, describe: 'nav alias for ui.openPage' });
  registerAction('nav.back', (_a, c) => dispatchAction({ do: 'ui.goBack' }, {}, c), A('nav alias for ui.goBack'));
  registerAction('nav.tab', (a, c) => dispatchAction({ do: 'ui.setTab', args: { tab: a['tab'] } }, {}, c), { args: { tab: 'string' }, describe: 'nav alias for ui.setTab' });
  registerAction('nav.bank', (a, c) => dispatchAction({ do: 'ui.setBank', args: { bank: a['bank'] } }, {}, c), { args: { bank: 'string' }, describe: 'nav alias for ui.setBank' });

  // banks / selection
  uiThrough('ui.setBank', a => [a['bank']], { args: { bank: 'string' }, describe: 'Select a bank' });
  uiThrough('ui.openBusPopup', () => [], S('Open the master bus selector popup'));
  uiThrough('ui.closeBusPopup', () => [], S('Close the master bus selector popup'));
  uiThrough('ui.selectBusMaster', a => [a['id']], { args: { id: 'string' }, describe: 'Pick a bus popup row (bank/master switch)' });
  uiThrough('ui.select', a => [a['id']], S('Select a channel'));
  uiThrough('ui.openEq', a => [a['id']], S('Open EQ for channel'));
  uiThrough('ui.openDyn', a => [a['id']], S('Open dynamics for channel'));
  uiThrough('ui.stepChannel', a => [a['delta']], { args: { delta: 'number' }, describe: 'Step selection +/-1' });
  uiThrough('ui.setTab', a => [a['tab']], { args: { tab: 'string' }, describe: 'Go to channel tab' });
  uiThrough('ui.setSelBand', a => [a['band']], { args: { band: 'number' }, describe: 'Select EQ band' });

  // fader / pan / mute (lock + muteEnable enforced in the store)
  uiThrough('ui.setFader', a => [a['id'], a['value']], { args: { id: 'string', value: 'number' }, describe: 'Set fader (lock-checked)' });
  uiThrough('ui.setSend', a => [a['id'], a['k'], a['value']], { args: { id: 'string', k: 'number', value: 'number' }, describe: 'Set send (lock-checked)' });
  uiThrough('ui.setPan', a => [a['id'], a['value']], { args: { id: 'string', value: 'number' }, describe: 'Set pan (lock-checked)' });
  uiThrough('ui.resetPan', a => [a['id']], S('Reset pan (lock-checked)'));
  uiThrough('ui.toggleMute', a => [a['id']], S('Toggle mute (muteEnable + lock checked)'));
  uiThrough('ui.toggleSolo', a => [a['id']], S('Toggle solo (muteEnable + lock checked; forwarded live)'));
  uiThrough('ui.openNameEditor', a => [a['id']], S('Open/close the name-bar editor (also selects)'));
  uiThrough('ui.closeNameEditor', () => [], S('Close the name-bar editor'));
  uiThrough('ui.setChannelName', a => [a['id'], a['value']], { args: { id: 'string', value: 'string' }, describe: 'Rename a channel (lock-checked)' });
  uiThrough('ui.setChannelColor', a => [a['id'], a['color'], a['text']], { args: { id: 'string', color: 'string', text: 'string' }, describe: 'Recolour a channel (lock-checked)' });
  uiThrough('ui.setChannelIcon', a => [a['id'], a['icon']], { args: { id: 'string', icon: 'any' }, describe: 'Set a channel strip icon (lock-checked)' });

  // EQ / dynamics / input (not lock-checked in the design)
  uiThrough('ui.setEqParam', a => [a['id'], a['band'], a['param'], a['value']], { args: { id: 'string', band: 'number', param: 'string', value: 'number' }, describe: 'Set EQ param' });
  uiThrough('ui.resetEqParam', a => [a['id'], a['band'], a['param']], { args: { id: 'string', band: 'number', param: 'string' }, describe: 'Reset EQ param' });
  uiThrough('ui.toggleEqOn', a => [a['id']], S('Toggle EQ on'));
  uiThrough('ui.toggleEqBand', a => [a['id'], a['band']], { args: { id: 'string', band: 'number' }, describe: 'Toggle EQ band on' });
  uiThrough('ui.flatEq', a => [a['id']], S('Flatten EQ gains'));
  uiThrough('ui.setHpfSlope', a => [a['id'], a['slope']], { args: { id: 'string', slope: 'number' }, describe: 'Set HPF slope' });
  uiThrough('ui.setEqBandType', a => [a['id'], a['band'], a['type']], { args: { id: 'string', band: 'number', type: 'string' }, describe: 'Set EQ band type' });
  uiThrough('ui.setDynParam', a => [a['id'], a['which'], a['prop'], a['value']], { args: { id: 'string', which: 'string', prop: 'string', value: 'number' }, describe: 'Set dynamics param' });
  uiThrough('ui.toggleDyn', a => [a['id'], a['which']], { args: { id: 'string', which: 'string' }, describe: 'Toggle gate/comp' });
  uiThrough('ui.toggleDelayOn', a => [a['id']], S('Toggle input delay on'));
  uiThrough('ui.toggleInvert', a => [a['id']], S('Toggle polarity invert'));
  uiThrough('ui.togglePhantom', a => [a['id']], S('Toggle 48V phantom'));
  uiThrough('ui.toggleSendPre', a => [a['id'], a['k']], { args: { id: 'string', k: 'number' }, describe: 'Toggle send pre/post' });

  // FX
  uiThrough('ui.setFxSel', a => [a['index']], { args: { index: 'number' }, describe: 'Select FX slot' });
  uiThrough('ui.setFxType', a => [a['index'], a['type']], { args: { index: 'number', type: 'string' }, describe: 'Set FX type' });
  uiThrough('ui.setFxParam', a => [a['index'], a['param'], a['value']], { args: { index: 'number', param: 'number', value: 'number' }, describe: 'Set FX param' });
  uiThrough('ui.resetFxParam', a => [a['index'], a['param']], { args: { index: 'number', param: 'number' }, describe: 'Reset FX param' });
  uiThrough('ui.toggleFxOn', a => [a['index']], { args: { index: 'number' }, describe: 'Toggle FX on' });
  uiThrough('ui.tapFx', a => [a['index']], { args: { index: 'number' }, describe: 'Tap FX tempo' });

  // routing (patch/unpatch lock-checked in the store)
  uiThrough('ui.setPatchSel', a => [a['id']], S('Pick routing channel'));
  uiThrough('ui.setPatchGroup', a => [a['group']], { args: { group: 'string' }, describe: 'Pick patch group' });
  uiThrough('ui.setRouteBank', a => [a['bank']], { args: { bank: 'number' }, describe: 'Pick routing bank' });
  uiThrough('ui.patchSrc', a => [a['label']], { args: { label: 'string' }, describe: 'Patch source (advances; lock-checked)' });
  uiThrough('ui.toggleRoute', a => [a['id'], a['label']], { args: { id: 'string', label: 'string' }, describe: 'Matrix cell: stage it (Apply commits; lock-checked)' });
  uiThrough('ui.patchOneToOne', a => [a['group']], { args: { group: 'string' }, describe: 'Preset: straight 1:1 patch of inputs 1..32 (lock-checked)' });
  uiThrough('ui.clearPatches', () => [], A('Preset: unpatch all inputs (lock-checked)'));
  uiThrough('ui.stagePreset', a => [a['kind'], a['group']], { args: { kind: 'string', group: 'string' }, describe: 'Stage a mass-patch preset (lock-checked)' });
  uiThrough('ui.applyPatch', () => [], A('Commit everything staged (lock-checked)'));
  uiThrough('ui.cancelPatch', () => [], A('Drop everything staged'));
  uiThrough('ui.unpatch', () => [], A('Unpatch channel (lock-checked)'));

  // desk groups (live-only; fixture model untouched, the engine holds the
  // state and command echoes re-render the scopes that read it)
  uiThrough('ui.setGroupAssign', a => [a['kind'], a['id'], a['group'], a['member']], { args: { kind: 'string', id: 'string', group: 'number', member: 'boolean' }, describe: 'Set desk group membership (mg|sg; lock-checked)' });
  uiThrough('ui.setGroupMute', a => [a['kind'], a['group'], a['muted']], { args: { kind: 'string', group: 'number', muted: 'boolean' }, describe: 'Set desk group master mute (mg|sg)' });

  // connection
  registerAction('ui.scan', (args, ctx) => {
    const fn = ctx.ui['scan'];
    if (typeof fn !== 'function') { ctx.warn('store has no "scan" (ui.scan skipped)'); return; }
    const clock = typeof args['clock'] === 'string' ? args['clock'] : clockNow();
    (fn as (...a: unknown[]) => unknown)(clock);
  }, { args: { clock: 'string' }, describe: 'Scan devices (clock defaults to HH:MM)' });
  uiThrough('ui.disconnect', () => [], A('Disconnect'));
  uiThrough('ui.connect', a => [a['key']], { args: { key: 'string' }, describe: 'Connect device' });
  // Real relay link (connection screen relay card -> live singleton).
  uiThrough('ui.connectRelay', a => [a['host'], a['driver']], { args: { host: 'string', driver: 'string' }, describe: 'Connect to the relay host (driver only when explicitly chosen)' });
  uiThrough('ui.disconnectRelay', () => [], A('Disconnect the relay link'));
  uiThrough('ui.setRelayHost', a => [a['host']], { args: { host: 'string' }, describe: 'Set the relay host (persisted)' });

  // behaviour settings
  uiThrough('ui.setSpp', a => [a['value']], N('Strips per screen'));
  uiThrough('ui.setFine', a => [a['value']], { args: { value: 'boolean' }, describe: 'Fine control' });
  uiThrough('ui.setSof', a => [a['value']], { args: { value: 'boolean' }, describe: 'Sends on faders' });
  uiThrough('ui.setMix', a => [a['value']], N('Send target mix'));
  uiThrough('ui.setPeakHold', a => [a['value']], { args: { value: 'boolean' }, describe: 'Meter peak hold' });
  uiThrough('ui.setMuteEnable', a => [a['value']], { args: { value: 'boolean' }, describe: 'Mute enable' });
  uiThrough('ui.setLocked', a => [a['value']], { args: { value: 'boolean' }, describe: 'Screen lock' });
  uiThrough('ui.unlock', () => [], A('Unlock'));
  uiThrough('ui.setSceneConfirm', a => [a['value']], { args: { value: 'boolean' }, describe: 'Scene recall mode' });

  // scenes
  uiThrough('ui.pickScene', a => [a['index']], { args: { index: 'number' }, describe: 'Pick scene' });
  uiThrough('ui.recallScene', () => [], A('Recall scene'));

  // profile ops over the bound profile store
  registerAction('profile.applyOp', (args, ctx) => {
    if (!ctx.profile) { ctx.warn('profile.applyOp: no profile store bound'); return; }
    ctx.profile.commit(args['op']);
  }, { args: { op: 'any' }, describe: 'Apply one profile op' });
  registerAction('profile.undo', (_a, ctx) => {
    if (!ctx.profile) { ctx.warn('profile.undo: no profile store bound'); return; }
    ctx.profile.undo();
  }, A('Undo profile op'));
  registerAction('profile.redo', (_a, ctx) => {
    if (!ctx.profile) { ctx.warn('profile.redo: no profile store bound'); return; }
    ctx.profile.redo();
  }, A('Redo profile op'));
  registerAction('profile.reset', (_a, ctx) => {
    if (!ctx.profile) { ctx.warn('profile.reset: no profile store bound'); return; }
    ctx.profile.reset();
  }, A('Reset profile to defaults'));

  // engine.*: documented placeholder wave 3 will implement (recall, params,
  // focus). Registered so layouts can reference them; dispatch warns + no-ops.
  for (const id of ['engine.setParam', 'engine.recallScene', 'engine.focus']) {
    registerAction(id, (_args, ctx) => {
      ctx.warn(`${id} is a wave-3 placeholder (engine binding not implemented); no-op`);
    }, { args: { id: 'any', value: 'any' }, describe: 'wave-3 placeholder (not implemented)' });
  }
}

// Builtins are registered on import so validators see known action ids.
// Tests reset with clearActions() + registerBuiltins(); screens re-bind the
// live stores with bindUiActions().
registerBuiltins();