<!-- Snapshots panel (item 8): desk snapshot library backed by the engine
     numbered scenes (the desk has no native snapshots — the panel says so).
     Numbered list + Save (named) / Load / Delete / Clear, plus the scoped
     recall matrix: channel banks x parameter groups x globals, with All
     toggles. All-on recalls the full plane (historical behavior); any
     narrower selection rides `scene_recall.scope` and the engine filters
     honestly. Presentational: entries/banks flow in, actions out. -->
<script lang="ts">
  export interface SnapshotEntry {
    number: number;
    name: string;
    current: boolean;
  }

  export interface SnapshotBank {
    key: string;
    label: string;
    strips: number[];
  }

  export interface SnapshotScope {
    strips?: number[];
    groups?: string[];
    globals?: string[];
  }

  const GROUPS: Array<{ id: string; label: string }> = [
    { id: 'source', label: 'Source' },
    { id: 'input', label: 'Input' },
    { id: 'config', label: 'Config' },
    { id: 'eq', label: 'EQ' },
    { id: 'dyn', label: 'Dyn' },
    { id: 'fader_pan', label: 'Fdr-Pan' },
    { id: 'mute', label: 'Mute' },
    { id: 'sends_bus', label: 'Sends-Bus' },
    { id: 'sends_fx', label: 'Sends-FX' },
  ];
  const GLOBALS: Array<{ id: string; label: string }> = [
    { id: 'main', label: 'Main LR' },
    { id: 'fx', label: 'FX slots' },
    { id: 'routing', label: 'Routing' },
  ];

  let {
    entries = [],
    banks = [],
    deskName = 'desk',
    onSave,
    onLoad,
    onDelete,
    onClear,
  }: {
    entries: SnapshotEntry[];
    banks: SnapshotBank[];
    deskName: string;
    onSave?: (name: string) => void;
    onLoad?: (n: number, scope: SnapshotScope | undefined) => void;
    onDelete?: (n: number) => void;
    onClear?: () => void;
  } = $props();

  let name = $state('');
  let bankOn: Record<string, boolean> = $state({});
  let groupOn: Record<string, boolean> = $state({});
  let globalOn: Record<string, boolean> = $state({});

  const bankKeys = $derived(banks.map(b => b.key));
  const allBanks = $derived(bankKeys.length > 0 && bankKeys.every(k => bankOn[k] !== false));
  const allGroups = $derived(GROUPS.every(g => groupOn[g.id] !== false));
  const allGlobals = $derived(GLOBALS.every(g => globalOn[g.id] !== false));
  const isFull = $derived(allBanks && allGroups && allGlobals);

  function setAll(v: boolean) {
    for (const k of bankKeys) bankOn[k] = v;
    for (const g of GROUPS) groupOn[g.id] = v;
    for (const g of GLOBALS) globalOn[g.id] = v;
  }

  function scopeForLoad(): SnapshotScope | undefined {
    if (isFull) return undefined;
    const strips = banks.filter(b => bankOn[b.key] !== false).flatMap(b => b.strips);
    const groups = GROUPS.filter(g => groupOn[g.id] !== false).map(g => g.id);
    const globals = GLOBALS.filter(g => globalOn[g.id] !== false).map(g => g.id);
    return { strips, groups, globals };
  }

  function nextFree(): number {
    const used = new Set(entries.map(e => e.number));
    for (let n = 1; n <= 999; n++) if (!used.has(n)) return n;
    return 999;
  }

  function save() {
    const n = nextFree();
    onSave?.(name.trim() || `Snapshot ${String(n).padStart(3, '0')}`);
    name = '';
  }

  const cellBtn =
    'box-sizing:border-box;min-width:0;height:44px;padding:0 10px;border-radius:3px;font-size:12px;font-weight:600;cursor:pointer;border:1px solid var(--u4-line-ctl);';
  function tog(on: boolean): string {
    return on
      ? 'background:var(--u4-chip-on-bg);color:var(--u4-chip-on-fg);'
      : 'background:var(--u4-panel-bg);color:var(--u4-text-9);';
  }
</script>

<div
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:8px;padding:12px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg-2)"
>
  <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
    <span style="font-size:15px;font-weight:700;color:var(--u4-text-2);margin-right:auto">Snapshots · engine library</span>
    <span style="font-size:11px;color:var(--u4-text-7)">{deskName} has no desk-native snapshots</span>
  </div>
  <div style="display:flex;gap:6px;flex-wrap:wrap">
    <input
      bind:value={name}
      placeholder="Snapshot name"
      aria-label="Snapshot name"
      maxlength="24"
      style="box-sizing:border-box;flex:1 1 140px;min-width:0;height:44px;padding:0 10px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-panel-bg);color:var(--u4-text-1);font-size:13px"
    />
    <button onclick={save} aria-label="Save snapshot" style="{cellBtn}background:var(--u4-green);color:var(--u4-green-ink);border-color:var(--u4-green)">Save</button>
    <button onclick={() => onClear?.()} aria-label="Delete all snapshots" style="{cellBtn}background:var(--u4-btn-bg);color:var(--u4-text-1);">Clear</button>
    <button onclick={() => setAll(true)} aria-label="Select all scope" style="{cellBtn}{tog(isFull)}">All</button>
  </div>
  {#if entries.length === 0}
    <div style="font-size:12px;color:var(--u4-text-9)">No snapshots saved yet — name one and press Save.</div>
  {:else}
    <div style="display:flex;flex-direction:column;gap:4px;max-height:220px;overflow-y:auto">
      {#each entries as e (e.number)}
        <div
          style="box-sizing:border-box;display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:3px;border:1px solid {e.current
            ? 'var(--u4-green-hi)'
            : 'var(--u4-line-faint)'};background:var(--u4-panel-bg)"
        >
          <span style="font-size:13px;font-weight:700;color:var(--u4-text-2);min-width:36px">{String(e.number).padStart(3, '0')}</span>
          <span style="font-size:13px;color:var(--u4-text-4);flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{e.name}</span>
          {#if e.current}<span style="font-size:10px;font-weight:700;color:var(--u4-green-hi)">CURRENT</span>{/if}
          <button onclick={() => onLoad?.(e.number, scopeForLoad())} aria-label={'Load snapshot ' + e.number} style="{cellBtn}background:var(--u4-green);color:var(--u4-green-ink);border-color:var(--u4-green)">Load</button>
          <button onclick={() => onDelete?.(e.number)} aria-label={'Delete snapshot ' + e.number} style="{cellBtn}background:var(--u4-btn-bg);color:var(--u4-amber);">Del</button>
        </div>
      {/each}
    </div>
  {/if}
  <div style="font-size:12px;font-weight:700;color:var(--u4-text-9)">Recall scope {isFull ? '(full plane)' : '(partial)'}</div>
  <div style="display:flex;gap:4px;flex-wrap:wrap" role="group" aria-label="Channel banks">
    {#each banks as b (b.key)}
      <button
        onclick={() => (bankOn[b.key] = !(bankOn[b.key] !== false))}
        aria-pressed={(bankOn[b.key] !== false).toString()}
        aria-label={'Scope bank ' + b.label}
        title={b.label}
        style="{cellBtn}{tog(bankOn[b.key] !== false)}"
        >{b.label}</button
      >
    {/each}
  </div>
  <div style="display:flex;gap:4px;flex-wrap:wrap" role="group" aria-label="Parameter groups">
    {#each GROUPS as g (g.id)}
      <button
        onclick={() => (groupOn[g.id] = !(groupOn[g.id] !== false))}
        aria-pressed={(groupOn[g.id] !== false).toString()}
        aria-label={'Scope group ' + g.label}
        style="{cellBtn}{tog(groupOn[g.id] !== false)}"
        >{g.label}</button
      >
    {/each}
  </div>
  <div style="display:flex;gap:4px;flex-wrap:wrap" role="group" aria-label="Globals">
    {#each GLOBALS as g (g.id)}
      <button
        onclick={() => (globalOn[g.id] = !(globalOn[g.id] !== false))}
        aria-pressed={(globalOn[g.id] !== false).toString()}
        aria-label={'Scope global ' + g.label}
        style="{cellBtn}{tog(globalOn[g.id] !== false)}"
        >{g.label}</button
      >
    {/each}
  </div>
</div>
