<!-- L1 RenderNode: renders ONE layout node with NO wrapper boxes.
     Container nodes map to ONE element (the container component's root);
     leaf modules render their own root element directly. Control-flow blocks
     ({#if}/{#each}) and snippets emit no DOM, so `data-l0` roots in the output
     are exactly the modules' own roots. Unknown modules were normalised to
     `missing-module` placeholders by LayoutRenderer (never silently dropped).
     Hot-path rule: this renderer subscribes to nothing per frame; bindings
     are fine-grained reads via ModuleContext (live canvas meters stay inside
     leaf modules, wave 3).
     L1: `bind` values are safe-language expressions over an expression scope
     ({...screen scope, item/index from repeats}); string `when` clauses use
     the same language; string props with `{{ }}` holes are interpolated;
     `on[event]` actions dispatch through the action registry (lock rules run
     in the store). Each bound prop updates alone via Svelte 5 runes; a
     failing expression keeps the prop default and warns once. -->
<script lang="ts">
  import type { Component, Snippet } from 'svelte';
  import { getModule } from './registry.ts';
  import { dispatchAction, type ActionTarget } from './actions.ts';
  import { evalExpr, hasInterpolation, isLegacyBinding, renderText, type Scope } from './expr.ts';
  import { applyVariant, evalWhen, expandRepeat } from './schema.ts';
  import type { LayoutNode } from './schema.ts';
  import { makeContext, type ContextSource } from './context.ts';
  import type { DeviceClass, Role } from './types.ts';
  import type { RepeatScope } from './schema.ts';
  import RenderNode from './RenderNode.svelte';

  let {
    node,
    source,
    scope,
    device = 'desktop',
    role = 'engineer',
    caps = [],
    settings = {},
  }: {
    node: LayoutNode;
    source: ContextSource;
    scope: RepeatScope;
    device?: DeviceClass;
    role?: Role;
    caps?: string[];
    settings?: Record<string, unknown>;
  } = $props();

  const warnedExprs = new Set<string>();

  function warnExpr(key: string, msg: string): void {
    if (warnedExprs.has(key) || warnedExprs.size > 500) return;
    warnedExprs.add(key);
    console.warn('[ui-bind]', msg);
  }

  const withVariant = $derived(applyVariant(node, device));
  // Expression scope: screen scope plus the repeat instance item/index.
  // `payload` is added per event at dispatch time (see fireEvent).
  const exprScope = $derived<Scope>({
    ...(scope as unknown as Record<string, unknown>),
    item: (withVariant.props as Record<string, unknown> | undefined)?.['item'] ?? (scope as unknown as Record<string, unknown>)['item'],
    index: (withVariant.props as Record<string, unknown> | undefined)?.['index'] ?? (scope as unknown as Record<string, unknown>)['index'],
    caps,
    setting: { ...settings, ...((scope as unknown as Record<string, unknown>)['setting'] as Record<string, unknown> | undefined ?? {}) },
    device,
    role,
  });
  const visible = $derived(
    !(withVariant.hidden ?? false) && evalWhenClause(withVariant.when, exprScope, { device, role, caps, settings: { ...settings, ...(scope.settings ?? {}) } }),
  );
  const manifest = $derived(getModule(withVariant.module));
  const supported = $derived(
    !manifest?.requires || manifest.requires.every((c) => caps.includes(c)),
  );
  const roleOk = $derived(!manifest?.roles || manifest.roles.includes(role));
  // 'remove' (default), 'collapse' and role/caps failures render nothing;
  // 'disabled' renders the module with disabled:true so the desk gap is honest.
  const fallbackGone = $derived(!supported && manifest?.fallback !== 'disabled');
  const mergedProps = $derived({
    ...((manifest?.defaults as Record<string, unknown> | undefined) ?? {}),
    ...((withVariant.props as Record<string, unknown> | undefined) ?? {}),
    ...((!supported && manifest?.fallback === 'disabled' ? { disabled: true } : {}) as Record<string, unknown>),
  });
  // L1: resolve each bound prop alone. Legacy catalogue strings go through
  // the ModuleContext; expressions evaluate in exprScope; failures keep the
  // prop default and warn once.
  const boundProps = $derived(resolveBind(withVariant.bind, exprScope));
  const finalProps = $derived(interpolateProps({ ...mergedProps, ...boundProps }, exprScope));
  const bindings = $derived([
    ...((manifest?.binds as string[] | undefined) ?? []),
    ...Object.values((withVariant.bind as Record<string, string> | undefined) ?? {}),
  ]);
  const ctx = $derived(makeContext(bindings, source));
  const Comp = $derived(manifest?.component as Component<Record<string, unknown>> | undefined);
  const expanded = $derived(withVariant.module === 'repeat' ? expandRepeat(withVariant, scope) : null);

  function resolveBind(
    bind: Record<string, string> | undefined,
    esc: Scope,
  ): Record<string, unknown> {
    if (!bind) return {};
    const out: Record<string, unknown> = {};
    for (const [key, expr] of Object.entries(bind)) {
      if (isLegacyBinding(expr)) {
        out[key] = resolveLegacy(expr, esc);
        continue;
      }
      const r = evalExpr(expr, esc);
      if (!r.ok) {
        warnExpr(`${node.id}.${key}:${expr}`, `bind "${key}" on "${node.id}" failed (${r.error}); kept default`);
        continue;
      }
      out[key] = r.value;
    }
    return out;
  }

  function resolveLegacy(expr: string, esc: Scope): unknown {
    const i = expr.indexOf(':');
    const kind = i < 0 ? expr : expr.slice(0, i);
    const id = i < 0 ? '' : expr.slice(i + 1);
    // Legacy strings name ModuleContext reads; mirror them from the scope so
    // old trees keep working: setting:<key> from settings, strip/bank/scene
    // from scope records, param:/meter: left for the module's own ctx reads.
    if (kind === 'setting') return (esc['setting'] as Record<string, unknown> | undefined)?.[id];
    if (kind === 'strip' || kind === 'bank' || kind === 'scene') {
      const rec = esc[kind] as Record<string, unknown> | undefined;
      if (rec && id in rec) return rec[id];
      return esc[kind];
    }
    return undefined;
  }

  function interpolateProps(
    props: Record<string, unknown>,
    esc: Scope,
  ): Record<string, unknown> {
    let dirty = false;
    for (const v of Object.values(props)) {
      if (typeof v === 'string' && hasInterpolation(v)) { dirty = true; break; }
    }
    if (!dirty) return props;
    const out: Record<string, unknown> = { ...props };
    for (const [k, v] of Object.entries(props)) {
      if (typeof v !== 'string' || !hasInterpolation(v)) continue;
      // renderText never throws; holes that fail render as ''.
      out[k] = renderText(v, esc);
    }
    return out;
  }

  function evalWhenClause(
    when: LayoutNode['when'],
    esc: Scope,
    legacyCtx: { device?: DeviceClass; role?: Role; caps?: string[]; settings?: Record<string, unknown> },
  ): boolean {
    if (!when) return true;
    if (typeof when === 'string') {
      if (isLegacyBinding(when)) return true;
      const r = evalExpr(when, esc);
      if (!r.ok) {
        warnExpr(`${node.id}.when:${when}`, `when on "${node.id}" failed (${r.error}); kept visible`);
        return true;
      }
      return truthy(r.value);
    }
    return evalWhen(when, legacyCtx);
  }

  function truthy(v: unknown): boolean {
    if (typeof v === 'boolean') return v;
    if (typeof v === 'number') return v !== 0 && !Number.isNaN(v);
    if (typeof v === 'string') return v.length > 0;
    return v !== null && v !== undefined;
  }

  function fireEvent(name: string, payload?: unknown): void {
    const target = withVariant.on?.[name] as ActionTarget | undefined;
    if (!target) return;
    dispatchAction(target, { ...exprScope, payload });
  }

  function onEvent(name: string, payload?: unknown): void {
    fireEvent(name, payload);
  }
</script>

{#if visible && Comp && !fallbackGone && !roleOk}
  <!-- role-gated: render nothing -->
{:else if visible && Comp && !fallbackGone}
  {#if expanded !== null}
    {#each expanded as inst (inst.id)}
      <RenderNode
        node={inst}
        {source}
        {scope}
        {device}
        {role}
        {caps}
        {settings}
      />
    {/each}
  {:else if withVariant.module === 'tabs'}
    {#snippet bodySnippet(n: LayoutNode)}
      <RenderNode node={n} {source} {scope} {device} {role} {caps} {settings} />
    {/snippet}
    <Comp
      props={finalProps}
      {ctx}
      nodeId={withVariant.id}
      nodes={withVariant.children ?? []}
      body={bodySnippet as Snippet}
      {onEvent}
    />
  {:else}
    {#snippet kidsSnippet()}
      {#each withVariant.children ?? [] as child (child.id)}
        <RenderNode node={child} {source} {scope} {device} {role} {caps} {settings} />
      {/each}
    {/snippet}
    <Comp props={finalProps} {ctx} nodeId={withVariant.id} children={kidsSnippet as Snippet} {onEvent} />
  {/if}
{/if}
