# Modular UI: one great default, fully customisable (binding for all UI packages)

Goal: the design in this folder ships as the DEFAULT configuration of a modular system. A user can rearrange, hide, add,
resize and restyle anything, build their own pages/banks/strips, and share the result. The default MUST remain pixel-exact
(PLAN decision 3): the default layout is data that the generic renderer turns into exactly the design's boxes.

## 1. Three layers
1. MODULES (code): every visual unit is a registered module = presentational Svelte component + manifest. Examples: `fader`,
   `meter`, `pan-bar`, `eq-mini`, `dyn-mini`, `mute-button`, `db-readout`, `name-bar`, `eq-graph`, `band-card`, `knob`,
   `knob-panel`, `comp-graph`, `send-row`, `fx-slot-card`, `scene-grid`, `device-list`, `patch-grid`, `setting-row`, `bank-rail`,
   `header-bar`, `scene-dropdown`, `master-strip`, `text`, `spacer`, `group` (container).
2. LAYOUTS (data): a screen/page/strip is a JSON tree of module instances (`ui/layouts/*.default.json`, schema in `ui/modules/schema.ts`).
   The design's screens (Mixer, Channel tabs, FX, Routing, Connection, Settings, Scenes) are default layouts, rendered by the same
   `LayoutRenderer` a user's custom layout uses. No screen composition may be hard-coded in Svelte.
3. PROFILES (user data): overrides on top of the shipped defaults (JSON patch style, versioned), per user/role/device class/venue:
   layouts, themes, banks, strip anatomy, settings. Stored locally now (`openmix.profile.v2.*`), exportable/importable as `.json`,
   later served by the engine so a venue's profile follows the desk. Shipped defaults can improve without wiping user changes
   because profiles store only overrides; "Reset to default" removes them.

## 2. Module manifest (TypeScript, `ui/modules/types.ts`)
```ts
interface ModuleManifest<P = unknown> {
  id: string;                       // 'fader'
  title: string;                    // shown in the module palette
  category: 'strip' | 'channel' | 'screen' | 'control' | 'layout' | 'display';
  component: Component<ModuleProps<P>>;
  props: PropSchema;                // typed, validated, drives the inspector UI (number/enum/bool/color/text/ref)
  defaults: P;
  size: { minW?; minH?; maxW?; maxH?; fixedW?; fixedH?; flex?: boolean };   // honoured by the renderer and editor
  binds?: Binding[];                // data it needs, by catalogue id/stream: ['param:fader','param:mute','meter:post',...]
  requires?: string[];              // caps ids; if the desk lacks them the module renders its `fallback` or is removed
  fallback?: 'remove' | 'collapse' | 'disabled';   // honest UI: never fake values (SPEC principle 2)
  roles?: Role[];                   // roles allowed to place/see it (musician profiles lock the layout)
  events?: Array<{ name: string; args?: string[] }>;  // L1: events emitted via onEvent (dispatched as actions)
}
```
Modules are presentational: props in, callbacks out. Data comes through a `ModuleContext` (bindings resolved by the renderer from the
store: `useParam(ref)`, `useMeter(ref)`, `useStrip(ref)`), so the same module works on fixtures, the sim and a real desk.

## 3. Layout schema (`ui/modules/schema.ts`)
Node = `{ id, module, props?, bind?, on?, children?, slot?, when? }`. Containers: `row`, `col`, `grid`, `stack`, `scroll`, `tabs`, `repeat`.
`repeat` instantiates a child template per strip/bank item (`over: 'bank.visible'`), which is how the strip row is a layout, and
the strip's anatomy (pan-bar, eq-mini, dyn-mini, mute, readout, fader, meter, name-bar) is its own editable template.
`when` = conditions (device class, role, caps, setting) or an L1 expression
string. `repeat` exposes `item`/`index` to `bind`, `when` and `{{ }}`. Sizes in px/fr/%, identical to the design's CSS so default boxes are exact.
Responsive: a layout may carry variants per device class (phone/tablet/desktop); the renderer picks by container width.
Pages (`ui/layouts/pages.default.json`): list of pages with id/title/icon/layout and navigation placement (header button, rail, tab),
so users can add pages (e.g. a drums-only mixer, a monitors page) and reorder/hide built-in ones.

## 4. Customisation surface (what "fully" means)
- Layout editing in place (Edit mode): move/resize/hide/duplicate modules, module palette, props inspector, undo/redo, reset page.
- Strip anatomy editor: which modules a strip has and in what order; strip width; strips per screen (the design's 8/12/16 is one prop).
- Banks: user-defined strip sets (any strips from any family, any order, custom names/colours), bank rail editor, layers; default = design banks.
- Pages and navigation: add/remove/reorder pages and header buttons; custom channel-detail tabs from channel modules.
- Themes: all visuals come from CSS tokens; themes = token override sets (colours, radii, density, font scale, font family choice),
  presets (the design's dark = default, daylight, night/red) + full editor + per-channel colour overrides; optional scoped custom CSS
  (advanced escape hatch, stored in the profile, applied inside the app root only).
- Behaviour settings: everything in the design's Settings screen (strips per screen, peak hold, fine, sends-on-faders, mute enable,
  screen lock, scene recall mode, ...) are profile fields; nothing is hard-coded.
- Roles: a profile can lock layout editing (musician/operator profiles delivered by the engine); locked profiles show no Edit mode.
- Extension API: `registerModule(manifest)` (documented, typed) so developers can ship new modules; a module only touches the world
  via its props and ModuleContext.
- Import/export/share: versioned JSON with validation (never throws on bad input), migration functions, "diff vs default" view.

## 5. Constraints that protect fidelity and performance
- The renderer adds NO wrapper boxes that change layout: containers map 1:1 to the design's flex/grid (verified by the box check in the diff harness).
- Default layouts live in `ui/layouts/*.default.json`; if the default needs a tweak to match the design, fix the JSON/module CSS, never special-case the renderer.
- Hot-path modules (fader, meter, knob drags, graphs) stay local-first and imperative (SPEC 2a); the layout renderer never re-renders them per frame.
- Every module has a gallery entry (`?ui=4&gallery`) and a fixture; every layout node type has a schema test; profile apply/merge/migrate/validate are pure and node-tested.
- Unsupported-by-desk modules follow their `fallback` rule; the editor shows them as "not available on this mixer" instead of silently dropping them from the user's saved layout (so a profile built for another desk still loads).

## 6. Actions and bindings (L1)

Screens are layout JSON + actions: no screen composition may be hard-coded in
Svelte. A node may carry `on: { <event>: <ActionRef | ActionRef[]> }` where
`ActionRef = { do: '<actionId>', args?: { <name>: <literal | { $expr }> } }`;
arg values are literals or binding refs (`{ $expr: '<expr>' }` evaluated in
the node's scope, which includes `payload`, `item`, `index`). Sequences run
in order; an unknown action id is a validation *warning* (the node keeps
rendering, the event is skipped with a `console.warn`); a throwing handler
warns and stops the sequence.

Nodes also take `bind: { <prop>: <expr> }`, `text` supports `{{ expr }}`
interpolation, and `when` accepts a string expression — all in one tiny SAFE
expression language (hand-written tokenizer + parser + budgeted interpreter;
no `eval`/`Function`/`new Function` anywhere):

- paths into the scope (`store.mode`, `item.id`, `index`, `param.fader`,
  `setting.stripsPerScreen`, `caps.has('eq_on')`), literals
  (`1`, `'str'`, `true`, `null`), `== != < > <= >= && || !`, ternary,
  `+ - * / %`, parentheses;
- whitelist of pure helpers: `fmtDb fmtHz fmtPan upper lower pad concat
  coalesce count join clamp round`;
- rejected at parse time (never evaluated): `__proto__` / `constructor` /
  `prototype`, non-whitelisted calls, brackets, semicolons, over-long input.

Each bound prop updates alone (Svelte 5 runes; the tree never re-renders);
a failing expression keeps the prop default and warns once.
`describeBinding(expr)` reports the scope paths + helpers an expression
reads (editor inspector); `listActions()` lists the registry.

Action namespaces: `ui.*` (every store action the screens call, run through
the store so lock/muteEnable rules apply), `nav.*` (page/back/tab/bank
aliases), `profile.*` (applyOp/undo/redo/reset on the bound profile store),
`engine.*` (documented placeholder wave 3 implements: warns + no-ops).
Manifests declare `events: [{ name, args }]`; modules emit via the
`onEvent(name, payload?)` prop the renderer supplies (hot-path modules keep
local-first logic and emit only commit/change events at the edges).
Profile ops can `set` `on.<event>` and `bind.<prop>` paths, and export/import
round-trips them.
