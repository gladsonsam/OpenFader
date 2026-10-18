<!-- L1 module `route-matrix`: Dante-style patchbay grid. Rows are the
     banked channels, columns the sources of the selected group; click a
     crosspoint to patch, click a lit one to unpatch (`press`
     {rowId, col} out). Readability follows Dante Controller: sticky
     headers, zebra rows, dividers every 8 columns, a hover/focus
     crosshair tracing the cell back to its row + column, native tooltips
     (`Ch 5 → Lcl 07`), and a status bar pinned to the scrollport bottom.
     Sizes come in as props so layouts scale per device (big dials on
     desktop, compact on phones); fonts derive from the cell size. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  type MatrixRow =
    | { kind: 'row'; id: string; name: string; src: string }
    | { kind: 'gap'; label: string };
  type MatrixCellRow = Extract<MatrixRow, { kind: 'row' }>;

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      rows?: MatrixRow[];
      cols?: string[];
      used?: Record<string, string>;
      pending?: { kind: 'oneToOne' | 'clear'; group: string } | null;
      /** Body cell px (fonts derive from it). Layouts widen per device. */
      cell?: number;
      /** Row header width px. */
      headW?: number;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const rows = $derived(props.rows ?? []);
  const cols = $derived(props.cols ?? []);
  const used = $derived(props.used ?? {});
  const pending = $derived(props.pending ?? null);
  const cell = $derived(props.cell ?? 68);
  const headW = $derived(props.headW ?? 220);
  const fCol = $derived(Math.max(9, Math.round(cell * 0.2)));
  const fName = $derived(Math.round(cell * 0.26));
  const fSrc = $derived(Math.round(cell * 0.22));

  type CellState = 'current' | 'conflict' | 'free' | 'preview';
  function stateOf(row: MatrixCellRow, col: string): CellState {
    if (isPreview(row, col)) return 'preview';
    if (row.src === col) return 'current';
    return used[col] ? 'conflict' : 'free';
  }
  /** Staged-preset preview: the cells Apply would change. 1:1 previews
   * inputs 1..32 at their same-numbered sockets; clear previews every
   * currently patched cell. */
  function isPreview(row: MatrixCellRow, col: string): boolean {
    if (!pending) return false;
    if (pending.kind === 'clear') return row.src !== '—' && row.src === col;
    const m = /^in(\d+)$/.exec(row.id);
    const n = m ? parseInt(m[1], 10) : NaN;
    if (!Number.isFinite(n) || n < 1 || n > 32) return false;
    const label = pending.group + ' ' + String(n).padStart(2, '0');
    return col === label && row.src !== label;
  }
  function ariaOf(row: MatrixCellRow, col: string, st: CellState): string {
    if (st === 'preview') return row.name + ' staged to ' + col + ', activate Apply to confirm';
    if (st === 'current') return row.name + ' patched to ' + col + ', activate to unpatch';
    return 'Patch ' + row.name + ' to ' + col + (st === 'conflict' ? ', now on ' + used[col] : '');
  }
  function tipOf(row: MatrixCellRow, col: string, st: CellState): string {
    const base = row.name + ' → ' + col;
    if (st === 'preview') return base + ' (staged — Apply to confirm)';
    if (st === 'current') return base + ' (patched — click to unpatch)';
    if (st === 'conflict') return base + ' (now on ' + used[col] + ' — click to steal)';
    return base;
  }

  let hover = $state({ row: '', col: '' });
  let readout = $state('');
  const rowCount = $derived(rows.filter((r) => r.kind === 'row').length);
  const patchedCount = $derived(rows.filter((r) => r.kind === 'row' && r.src !== '—').length);
  const defaultLine = $derived(
    pending
      ? pending.kind === 'clear'
        ? 'Staged: unpatch ' + patchedCount + ' inputs — Apply to confirm'
        : 'Staged: 1:1 ' + pending.group + ' — Apply to confirm'
      : patchedCount + ' patched · ' + (rowCount - patchedCount) + ' open',
  );
  function enter(row: MatrixCellRow, col: string): void {
    hover = { row: row.id, col };
    const st = stateOf(row, col);
    readout = tipOf(row, col, st);
  }
  function leave(): void {
    hover = { row: '', col: '' };
  }
  function press(row: MatrixCellRow, col: string): void {
    readout = tipOf(row, col, stateOf(row, col));
    onEvent?.('press', { rowId: row.id, col });
  }
</script>

<div style="display:flex;flex-direction:column;min-width:0">
  <div
    style="display:grid;grid-template-columns:{headW}px repeat({cols.length}, {cell}px);width:max-content"
  >
    <div
      aria-hidden="true"
      style="box-sizing:border-box;width:{headW}px;height:{cell}px;display:flex;align-items:flex-end;padding:4px 10px;background:#141c29;color:#5b6678;font-size:{fCol}px;font-weight:700;position:sticky;top:0;left:0;z-index:4"
    >
      CH \ SRC
    </div>
    {#each cols as col, j (col)}
      <div
        role="columnheader"
        style="box-sizing:border-box;width:{cell}px;height:{cell}px;display:flex;align-items:flex-end;justify-content:center;padding:4px 2px;background:{hover.col ===
        col
          ? '#22314b'
          : '#141c29'};color:{hover.col === col ? '#ffffff' : '#8f9bb0'};font-size:{fCol}px;font-weight:700;white-space:nowrap;position:sticky;top:0;z-index:2;{j %
        8 ===
        0
          ? 'border-left:2px solid #33415a;'
          : ''}"
      >
        {col}
      </div>
    {/each}
    {#each rows as row, ri (ri)}
      {#if row.kind === 'gap'}
        <div
          aria-hidden="true"
          style="grid-column:1 / -1;box-sizing:border-box;display:flex;align-items:center;padding:6px 10px 2px;background:transparent;color:#5b6678;font-size:11px;font-weight:700;letter-spacing:1px"
        >
          {row.label}
        </div>
      {:else}
      <div
        role="rowheader"
        onmouseenter={() => {
          hover = { row: row.id, col: '' };
        }}
        onmouseleave={leave}
        style="box-sizing:border-box;width:{headW}px;height:{cell}px;display:flex;flex-direction:column;justify-content:center;gap:1px;padding:2px 10px;background:{hover.row ===
        row.id
          ? '#22314b'
          : '#141c29'};position:sticky;left:0;z-index:1"
      >
        <span
          style="font-size:{fName}px;font-weight:600;color:{hover.row === row.id
            ? '#ffffff'
            : '#eef2f7'};white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{row.name}</span
        >
        <span
          style="font-size:{fSrc}px;color:#5fe3f0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
          >{row.src}</span
        >
      </div>
      {#each cols as col, j (col)}
        {@const st = stateOf(row, col)}
        {@const hot = hover.row === row.id || hover.col === col}
        <button
          onclick={() => press(row, col)}
          onmouseenter={() => enter(row, col)}
          onmouseleave={leave}
          onfocus={() => enter(row, col)}
          onblur={leave}
          aria-label={ariaOf(row, col, st)}
          aria-pressed={st === 'current' ? 'true' : 'false'}
          title={tipOf(row, col, st)}
          style="box-sizing:border-box;width:{cell}px;height:{cell}px;display:flex;align-items:center;justify-content:center;padding:0;border:0;border-radius:0;background:{st ===
          'current'
            ? '#0e2018'
            : st === 'preview'
              ? '#141d33'
              : hot
                ? '#182435'
                : ri % 2 === 0
                  ? '#0c1320'
                  : '#0a0f17'};cursor:pointer;{j % 8 === 0 ? 'border-left:2px solid #33415a;' : ''}"
        >
          <span
            aria-hidden="true"
            style="width:{st === 'free' ? cell * 0.18 : cell * 0.34}px;height:{st === 'free'
              ? cell * 0.18
              : cell * 0.34}px;border-radius:50%;background:{st === 'current'
              ? '#5cdb5c'
              : st === 'conflict'
                ? '#f0b429'
                : 'transparent'};border:2px solid {st === 'current'
              ? '#5cdb5c'
              : st === 'conflict'
                ? '#f0b429'
                : st === 'preview'
                  ? '#ffffff'
                  : '#2a3443'}"
          ></span>
        </button>
      {/each}
      {/if}
    {/each}
    <div
      role="status"
      style="grid-column:1 / -1;position:sticky;bottom:0;z-index:3;background:#101825;border-top:1px solid #2a3443;padding:7px 10px;font-size:13px;color:#c9d1dc;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
    >
      {readout || defaultLine}
    </div>
  </div>
</div>

<style>
  button:focus-visible {
    outline: 2px solid #7cc4ff;
    outline-offset: -2px;
  }
  button:hover {
    filter: brightness(1.35);
  }
</style>
