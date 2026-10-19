# Layout editor (Mixing-Station parity, workstream 5)

Visual editor over the L0 module system. Route: `?ui=4&screen=layout-editor`.
Code: `apps/web/src/ui/modules/editor/`
(`EditorShell.svelte`, `Palette.svelte`, `Inspector.svelte`, `Outline.svelte`,
`SharePanel.svelte`, `EditorPage.svelte`, `editMode.svelte.ts`).

Profiles store only overrides (ops) over shipped defaults
(`ui/layouts/*.default.json`); see `ui/modules/profile/ops.ts`.

## Edit mode + roles

Edit chrome is hidden unless `canEditProfile(locked, role)`:

| profile locked | role     | Edit mode | canvas ops              | banks/pages/share |
|----------------|----------|-----------|-------------------------|-------------------|
| yes            | any      | hidden    | none (store rejects)    | no                |
| no             | engineer | visible   | set/remove/insert/move/hide + reset | yes        |
| no             | operator | visible   | set/insert/move/hide (no Delete — hide instead) | no |
| no             | musician | hidden    | none (fixed layout)     | no                |

Role preview selector lives in the editor toolbar; modules whose manifest
`roles` exclude the current role are hidden from the palette and render
nothing on canvas (existing `RenderNode` role gate). Caps-gated modules
render their `fallback` instead of values (never fake numbers).

## Palette → canvas → inspector (ops produced)

- **Palette** (`queryPalette` + category filter): click inserts under the
  selected container (or after the selected leaf, or at root); drag-drop onto
  a canvas node inserts into that container / after that leaf. Produces
  `insert` ops with `defaults` props and a unique `${module}-${n}` id.
- **Canvas**: click selects (`[data-node]` delegation); drop of a palette
  module inserts; drop of an outline node moves (`move` op; dropping on a
  leaf means "after that leaf"). Edit outline marks the drop zone.
- **Outline**: tree list, select, per-row hide toggle, drag to re-parent /
  reorder (`move` ops; `__sibling:` drops resolve to the leaf's parent).
- **Inspector** (PropSchema-driven): number/enum/bool/color/text/ref/list
  controls → `set` ops on `props.<key>`; `object` fields read-only JSON.
  Raw `bind` (JSON prop→expression), `on` (JSON event→action), `when`
  (JSON object or expression string) editors → `set` ops. Hide toggle →
  `hide` op; ↑/↓ reorder → `move` ops; Delete → `remove` op (engineer only);
  Reset node → `resetNode` + `diffFromDefault` recommitted via `setOps`.
- **Resize** = prop edits: containers expose `width`/`height`/`flex`/`gap`/
  `padding`; leaves expose their `height`/`size` numbers (inspector).
  No separate resize handle — sizes are layout props, hence ops.

Undo/redo/reset go through the profile store history (`setOps` for reset-node).

## Runtime renderer coverage

`LayoutRenderer` normalises the tree (`normalizeLayout`: unknown modules →
visible `missing-module` placeholders, never silent drops), then `RenderNode`
handles, per node:

- `bind`: legacy catalogue strings (`param:`/`meter:`/`strip`/`bank`/`scene`,
  `setting:`) + L1 safe-language expressions over scope (`store`, `item`,
  `index`, `caps`, `setting`, `device`, `role`); failures keep the default
  and warn once.
- `on`: `{ do, args }` action refs (incl. `$expr` args over event scope)
  dispatched via the action registry; unknown ids warn, never throw.
- `when`: object form (`device`/`role`/`caps`/`setting`) + L1 expression
  strings; failures keep the node visible.
- `variants`: per-device-class overlay (`phone`/`tablet`/`desktop`), shallow
  prop/bind merge, children replace.
- `repeat`: `over: strips | bank:<k> | list:<k>` expanded per item
  (`id[item]`, `props.item`/`props.index`, `strip:<item>` binds).
- containers `row`/`col`/`grid`/`stack`/`scroll`/`tabs`/`repeat` render
  exactly one element each; `tabs` gets `nodes`+`body`, others `children`.
- `hidden` nodes are skipped. Caps failures follow `fallback`:
  `remove`/`collapse`/role-gate render nothing, `disabled` renders with
  `disabled: true`.
- text props support `{{ }}` interpolation.

## Supported modules (registry catalogue)

Containers (layout): `row`, `col`, `grid`, `stack`, `scroll`, `tabs`,
`repeat`, `text`, `spacer`, `group`, `missing-module` (placeholder).

| id | title | cat | requires → fallback |
|----|-------|-----|---------------------|
| `fader` | Fader | strip | `param:fader` → disabled |
| `meter` | Meter | display | `meter:post` → collapse |
| `knob` | Knob | control | `param:value` → disabled |
| `mute-button` | Mute button | strip | — → disabled |
| `pan-bar` | Pan bar | strip | `param:pan` → collapse |
| `name-bar` | Name bar | strip | — |
| `scale-labels` | Scale labels | display | — |
| `strip-frame` | Mixer strip frame | strip | — |
| `strip-pan` | Strip pan bar | strip | — |
| `strip-eq-mini` | Strip EQ mini pane | strip | — |
| `strip-dyn-mini` | Strip dynamics mini pane | strip | — |
| `strip-mute` | Strip mute button | strip | — |
| `strip-readout` | Strip dB readout | display | — |
| `strip-fader` | Strip fader | strip | — |
| `strip-name` | Strip name bar | strip | — |
| `strip-name-editor` | Strip name editor | strip | — |
| `strip-group-badges` | Strip desk-group badges | strip | — |
| `strip-state-icons` | Strip status icons | strip | — |
| `strip-solo` | Strip solo button | strip | — |
| `strip-empty` | Empty bank slot | strip | — |
| `strip-well-empty` | Empty proc well | strip | — |
| `channel-tab-button` | Channel tab button | channel | — |
| `channel-name-chip` | Channel name chip | channel | — |
| `channel-pan-bar` | Channel pan bar | channel | — |
| `channel-mute` | Channel mute button | channel | — |
| `channel-fader` | Channel fader + meter | channel | — |
| `channel-readout` | Channel dB readout | channel | — |
| `channel-source-label` | Channel source label | channel | — |
| `input-preamp-card` | Input preamp card | channel | — |
| `input-source-card` | Input source card | channel | — |
| `input-delay-card` | Input delay card | channel | — |
| `send-row` | Send row | channel | — |
| `channel-groups-card` | Channel desk-groups card | channel | — |
| `eq-graph` | EQ response graph | (eq) | — |
| `eq-head` | EQ head buttons | (eq) | — |
| `eq-band-card` | EQ band card | (eq) | — |
| `eq-hpf-card` | HPF card | (eq) | — |
| `eq-summary` | EQ selected-band summary | (eq) | — |
| `dyn-transfer-graph` | Dyn transfer graph | (dyn) | — |
| `dyn-knob-panel` | Dyn knob panel | (dyn) | — |
| `dyn-summary` | Dyn summary | (dyn) | — |
| `fx-slot-card` | FX slot card | (fx) | `fx` → remove |
| `fx-detail-panel` | FX detail panel | (fx) | `fx` → remove |
| `fx-type-selector` | FX type selector | (fx) | `fx` → remove |
| `fx-param-knobs` | FX param knobs | (fx) | `fx` → remove |
| `channel-presets-card` | Channel presets | (presets) | — |
| `btn` | Exact button | control | — |
| `route-tile` | Routing channel tile | (routing) | — |
| `src-tile` | Routing source tile | (routing) | — |
| `route-matrix` | Routing patchbay matrix | (routing) | — |
| `status-dot` | Connection status dot | (conn) | — |
| `device-row` | Connection device row | (conn) | — |
| `relay-card` | Relay connection card | (conn) | — |
| `manual-desk` | Manual desk add (add-by-IP) | (conn) | — |
| `header-bar` | Header bar | (shell) | — |
| `bank-rail` | Bank rail | (shell) | — |
| `scene-dropdown` | Scene dropdown | (shell) | — |
| `setting-row` | Setting row | control | — |
| `settings-panel` | Settings panel | screen | — |
| `scene-card` | Scene card | control | — |
| `scene-recall-bar` | Scene recall bar | control | — |
| `button` | Button | control | — |
| `panel` | Panel | screen | — |
| `segmented` | Segmented | control | — |

## Share: export / import / banks / pages

Share panel (behind Edit mode → Share toggle):

- **Text**: Export fills a textarea with `exportProfile` JSON; Import parses
  it with `importProfile` (migrate + validate, never throws; errors inline).
  Copy button uses the clipboard. This textarea is the QR/text path — paste
  it into any transport.
- **File**: Download saves `<id>.openmix-profile.json`; Upload reads a file
  into the textarea and imports it.
- **Banks**: list with name edit, items edit (comma-separated stable strip
  keys, e.g. `ch/01, ch/02, main`), add/delete — via `store.setBanks`.
- **Pages**: list with title/layout/nav (`header`/`rail`/`tab`/`hidden`)
  edit, add/delete — via `store.setPages`.
- Locked profiles reject import/banks/pages (store returns false; panel
  disables the inputs).
