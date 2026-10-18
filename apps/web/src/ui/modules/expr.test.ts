// L1 tests: safe expression language (precedence, ternary, whitelist,
// rejections, limits) + {{ }} interpolation + describeBinding.
// Run: node --experimental-strip-types apps/web/src/ui/modules/expr.test.ts
import { strict as assert } from 'node:assert';
import {
  describeBinding,
  evalExpr,
  hasInterpolation,
  helperNames,
  isLegacyBinding,
  renderText,
} from './expr.ts';

function v(expr: string, scope: Record<string, unknown> = {}): unknown {
  const r = evalExpr(expr, scope);
  assert.equal(r.ok, true, `${expr}: ${r.ok ? '' : (r as { error: string }).error}`);
  return (r as { ok: true; value: unknown }).value;
}

function bad(expr: string, scope: Record<string, unknown> = {}): string {
  const r = evalExpr(expr, scope);
  assert.equal(r.ok, false, `${expr} should fail`);
  return (r as { ok: false; error: string }).error;
}

const scope = {
  store: { mode: 'routing', routeBank: 1, patchSel: 'in3' },
  item: { id: 'in3', label: 'Lcl 03', pressed: true },
  index: 2,
  param: { fader: 0.75 },
  setting: { stripsPerScreen: 12 },
  caps: ['eq_on', 'connect'],
};

// --- literals + paths ---
{
  assert.equal(v('1', scope), 1);
  assert.equal(v("'hi'", scope), 'hi');
  assert.equal(v('"hi"', scope), 'hi');
  assert.equal(v('true', scope), true);
  assert.equal(v('null', scope), null);
  assert.equal(v('store.mode', scope), 'routing');
  assert.equal(v('item.id', scope), 'in3');
  assert.equal(v('index', scope), 2);
  assert.equal(v('param.fader', scope), 0.75);
  assert.equal(v('setting.stripsPerScreen', scope), 12);
  assert.equal(v('missing.deep.path', scope), undefined);
}

// --- operators: precedence, ternary, + - * / %, comparisons, logic ---
{
  assert.equal(v('1 + 2 * 3', scope), 7);
  assert.equal(v('(1 + 2) * 3', scope), 9);
  assert.equal(v('10 - 4 - 3', scope), 3);
  assert.equal(v('7 % 3', scope), 1);
  assert.equal(v('7 / 2', scope), 3.5);
  assert.equal(v('-index', scope), -2);
  assert.equal(v('!item.pressed', scope), false);
  assert.equal(v('index == 2', scope), true);
  assert.equal(v('index != 2', scope), false);
  assert.equal(v('index > 1 && item.pressed', scope), true);
  assert.equal(v('index > 5 || item.pressed', scope), true);
  assert.equal(v('index >= 2 ? "a" : "b"', scope), 'a');
  assert.equal(v('store.mode == "routing" ? item.id : "none"', scope), 'in3');
  assert.equal(v('1 < 2 == true', scope), true);
  assert.equal(v("'a' + 'b'", scope), 'ab');
  assert.equal(v("'n' + 1", scope), 'n1');
  assert.equal(v('store.routeBank + 1', scope), 2);
  // && / || return operand values (needed for fallbacks)
  assert.equal(v('store.mode && item.id', scope), 'in3');
  assert.equal(v('missing || "dflt"', scope), 'dflt');
}

// --- helper whitelist ---
{
  assert.equal(v('upper(store.mode)', scope), 'ROUTING');
  assert.equal(v('lower("AbC")', scope), 'abc');
  assert.equal(v('pad(3, 2)', scope), '03');
  assert.equal(v('concat("a", 1, null)', scope), 'a1');
  assert.equal(v('coalesce(missing, null, "x")', scope), 'x');
  assert.equal(v('count(caps)', scope), 2);
  assert.equal(v('join(caps, ",")', scope), 'eq_on,connect');
  assert.equal(v('clamp(99, 0, 1)', scope), 1);
  assert.equal(v('round(2.345, 2)', scope), 2.35);
  assert.equal(v('round(2.5)', scope), 3);
  assert.equal(v('fmtDb(-4.32)', scope), '-4.3');
  assert.equal(v('fmtDb(-100)', scope), '-∞');
  assert.equal(v('fmtHz(2500)', scope), '2.50 kHz');
  assert.equal(v('fmtPan(-0.28)', scope), 'L28');
  assert.equal(v('caps.has("connect")', scope), true);
  assert.equal(v('caps.has("nope")', scope), false);
  assert.deepEqual(helperNames().sort(), ['clamp', 'coalesce', 'concat', 'count', 'fmtDb', 'fmtHz', 'fmtPan', 'join', 'lower', 'pad', 'round', 'upper']);
}

// --- rejections: everything outside the grammar fails, never throws ---
{
  bad('constructor');
  bad('__proto__');
  bad('a.__proto__.b');
  bad('a.constructor');
  bad('a.prototype');
  bad('foo(1)', scope); // not whitelisted
  bad('eval("1")', scope);
  bad('upper("a", "b", "c", "d")', scope); // arity
  bad('item.id; drop', scope);
  bad('a = 1', scope);
  bad('a[0]', scope);
  bad('`tpl`', scope);
  bad('(1 + 2', scope);
  bad('1 ? 2', scope);
  bad('', scope);
  bad('caps.has("a", "b")', scope);
  bad('store.mode.length()', scope); // methods other than .has rejected
  bad('caps.join()', scope);
  bad('true ? "unclosed', scope);
  bad('x'.repeat(600), scope); // length DoS limit
  bad(Array(120).fill('1').join('+'), scope); // token DoS limit
  // deep nesting DoS limit
  let deep = '1';
  for (let i = 0; i < 40; i++) deep = `(${deep})`;
  bad(deep, scope);
}

// --- interpolation ---
{
  assert.equal(renderText('plain', scope), 'plain');
  assert.equal(renderText('Patch {{ item.label }} to', scope), 'Patch Lcl 03 to');
  assert.equal(renderText('{{ index + 1 }} / {{ count(caps) }}', scope), '3 / 2');
  assert.equal(renderText('a {{ missing }} b', scope), 'a  b');
  assert.equal(renderText('a {{ bogus( }} b', scope), 'a  b');
  assert.equal(hasInterpolation('x {{ y }}'), true);
  assert.equal(hasInterpolation('{item}'), false);
  assert.equal(hasInterpolation('no holes'), false);
}

// --- describeBinding ---
{
  const d = describeBinding('item.pressed ? upper(item.id) : "x"');
  assert.equal(d.ok, true);
  assert.deepEqual(d.refs.sort(), ['item.id', 'item.pressed']);
  assert.deepEqual(d.calls, ['upper']);
  const e = describeBinding('nope(');
  assert.equal(e.ok, false);
  assert.ok(e.error && e.error.length > 0);
}

// --- legacy bindings ---
{
  assert.equal(isLegacyBinding('param:fader'), true);
  assert.equal(isLegacyBinding('meter:post'), true);
  assert.equal(isLegacyBinding('setting:sof'), true);
  assert.equal(isLegacyBinding('strip'), true);
  assert.equal(isLegacyBinding('bank'), true);
  assert.equal(isLegacyBinding('scene'), true);
  assert.equal(isLegacyBinding('item.id'), false);
  assert.equal(isLegacyBinding('store.mode'), false);
}

console.log('expr.test.ts: ok');
