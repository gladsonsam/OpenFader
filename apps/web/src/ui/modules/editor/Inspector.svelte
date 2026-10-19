<!-- Visual-editor props inspector: renders one control per PropSchema entry
     plus bind/on/when/hidden editors. Every change commits a ProfileOp
     (`set` with a `props.<key>` path, or `hide`); delete/move/reset emit
     through callbacks so the parent owns the op plumbing. Read-only `object`
     fields render as JSON (view-model rows bound from scope). -->
<script lang="ts">
  import { getModule } from '../registry.ts';
  import type { LayoutNode } from '../schema.ts';
  import type { PropSchema, PropSchemaField } from '../types.ts';

  let {
    node,
    canDelete = true,
    onSetProp,
    onSetRaw,
    onToggleHidden,
    onRemove,
    onMoveUp,
    onMoveDown,
    onReset,
  }: {
    node: LayoutNode | undefined;
    canDelete?: boolean;
    onSetProp: (path: string, value: unknown) => void;
    onSetRaw: (key: 'bind' | 'on' | 'when', raw: string) => void;
    onToggleHidden: () => void;
    onRemove: () => void;
    onMoveUp: () => void;
    onMoveDown: () => void;
    onReset: () => void;
  } = $props();

  const manifest = $derived(node ? getModule(node.module) : undefined);
  const schema = $derived<PropSchema>(manifest?.props ?? {});
  const cur = $derived<Record<string, unknown>>((node?.props ?? {}) as Record<string, unknown>);

  // Raw-text drafts for bind/on/when (JSON or expression string).
  let bindText = $state('');
  let onText = $state('');
  let whenText = $state('');
  let lastId = $state('');

  $effect(() => {
    if (!node || node.id === lastId) return;
    lastId = node.id;
    bindText = node.bind ? JSON.stringify(node.bind, null, 1) : '';
    onText = node.on ? JSON.stringify(node.on, null, 1) : '';
    whenText =
      node.when === undefined ? '' : typeof node.when === 'string' ? node.when : JSON.stringify(node.when, null, 1);
  });

  function fieldValue(key: string, f: PropSchemaField): unknown {
    const v = cur[key];
    if (v !== undefined) return v;
    return 'default' in f ? f.default : undefined;
  }

  function commit(key: string, v: unknown): void {
    onSetProp(`props.${key}`, v);
  }
</script>

<div aria-label="Props inspector" style="display:flex;flex-direction:column;gap:6px;min-width:210px;max-width:270px">
  {#if !node}
    <div style="font-size:12px;color:var(--u4-text-5)">Select a node on the canvas or in the outline.</div>
  {:else}
    <div style="font-size:12px;color:var(--u4-text-4)">
      <strong style="color:var(--u4-text-1)">{node.id}</strong> · {node.module}
      {#if manifest?.requires?.length}· needs {manifest.requires.join(', ')}{/if}
    </div>
    {#if manifest === undefined}
      <div role="note" style="font-size:12px;color:#f0b44a">
        Not available on this build (missing-module placeholder). The profile still loads; pick another module or reset.
      </div>
    {/if}
    {#each Object.entries(schema) as [key, field] (key)}
      {@const f = field as PropSchemaField}
      {@const val = fieldValue(key, f)}
      <label style="display:flex;flex-direction:column;gap:2px;font-size:11px;color:var(--u4-text-4)">
        <span>{key}{f.type === 'number' && f.unit ? ` (${f.unit})` : ''}</span>
        {#if f.type === 'number'}
          <input
            type="number"
            aria-label={key}
            value={typeof val === 'number' ? val : (f.default ?? 0)}
            min={f.min}
            max={f.max}
            step={f.step ?? 'any'}
            onchange={(e) => commit(key, (e.currentTarget as HTMLInputElement).valueAsNumber)}
            style="padding:4px 6px;font-size:12px"
          />
        {:else if f.type === 'bool'}
          <input
            type="checkbox"
            aria-label={key}
            checked={val === true}
            onchange={(e) => commit(key, (e.currentTarget as HTMLInputElement).checked)}
          />
        {:else if f.type === 'enum'}
          <select
            aria-label={key}
            value={String(val ?? f.default ?? '')}
            onchange={(e) => commit(key, (e.currentTarget as HTMLSelectElement).value)}
            style="padding:4px 6px;font-size:12px"
          >
            {#each f.options as o (o)}
              <option value={o}>{o}</option>
            {/each}
          </select>
        {:else if f.type === 'color'}
          <span style="display:flex;gap:4px;align-items:center">
            <input
              type="color"
              aria-label={`${key} picker`}
              value={typeof val === 'string' && /^#[0-9a-fA-F]{6}$/.test(val) ? val : '#808080'}
              onchange={(e) => commit(key, (e.currentTarget as HTMLInputElement).value)}
            />
            <input
              type="text"
              aria-label={key}
              value={String(val ?? f.default ?? '')}
              onchange={(e) => commit(key, (e.currentTarget as HTMLInputElement).value)}
              style="flex:1;padding:4px 6px;font-size:12px"
            />
          </span>
        {:else if f.type === 'object'}
          <code style="font-size:10px;white-space:pre-wrap;word-break:break-word">{JSON.stringify(val ?? null)}</code>
        {:else if f.type === 'list'}
          <textarea
            aria-label={key}
            rows="2"
            value={JSON.stringify(val ?? f.default ?? [])}
            onchange={(e) => {
              try {
                commit(key, JSON.parse((e.currentTarget as HTMLTextAreaElement).value));
              } catch {
                /* keep old value on bad JSON */
              }
            }}
            style="padding:4px 6px;font-size:11px;font-family:monospace"
          ></textarea>
        {:else}
          <input
            type="text"
            aria-label={key}
            value={String(val ?? f.default ?? '')}
            onchange={(e) => commit(key, (e.currentTarget as HTMLInputElement).value)}
            style="padding:4px 6px;font-size:12px"
          />
        {/if}
      </label>
    {/each}
    {#if Object.keys(schema).length === 0}
      <div style="font-size:12px;color:var(--u4-text-5)">No editable props on this module.</div>
    {/if}
    <label style="display:flex;flex-direction:column;gap:2px;font-size:11px;color:var(--u4-text-4)">
      <span>bind (JSON: prop → expression)</span>
      <textarea
        aria-label="bind JSON"
        rows="2"
        bind:value={bindText}
        onchange={() => onSetRaw('bind', bindText)}
        style="padding:4px 6px;font-size:11px;font-family:monospace"
      ></textarea>
    </label>
    <label style="display:flex;flex-direction:column;gap:2px;font-size:11px;color:var(--u4-text-4)">
      <span>on (JSON: event → action)</span>
      <textarea
        aria-label="on JSON"
        rows="2"
        bind:value={onText}
        onchange={() => onSetRaw('on', onText)}
        style="padding:4px 6px;font-size:11px;font-family:monospace"
      ></textarea>
    </label>
    <label style="display:flex;flex-direction:column;gap:2px;font-size:11px;color:var(--u4-text-4)">
      <span>when (JSON object or expression)</span>
      <textarea
        aria-label="when"
        rows="2"
        bind:value={whenText}
        onchange={() => onSetRaw('when', whenText)}
        style="padding:4px 6px;font-size:11px;font-family:monospace"
      ></textarea>
    </label>
    <div style="display:flex;flex-wrap:wrap;gap:4px">
      <button onclick={onToggleHidden} style="font-size:12px">{node.hidden ? 'Unhide' : 'Hide'}</button>
      <button onclick={onMoveUp} style="font-size:12px" aria-label="Move up">↑</button>
      <button onclick={onMoveDown} style="font-size:12px" aria-label="Move down">↓</button>
      <button onclick={onReset} style="font-size:12px">Reset node</button>
      {#if canDelete}
        <button onclick={onRemove} style="font-size:12px;color:#e0605f">Delete</button>
      {/if}
    </div>
  {/if}
</div>
