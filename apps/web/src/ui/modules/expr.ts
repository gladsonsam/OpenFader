// L1 Actions + bindings: tiny SAFE expression language for layout `bind`
// values, `when` strings, `{{ }}` text interpolation and `$expr` action args.
//
// Grammar (whitelist only; anything else is a parse error, never evaluated):
//   expr    := ternary
//   ternary := or ('?' expr ':' expr)?
//   or      := and ('||' and)*
//   and     := equality ('&&' equality)*
//   equality:= rel (('==' | '!=') rel)*
//   rel     := add (('<=' | '>=' | '<' | '>') add)*
//   add     := mul (('+' | '-') mul)*
//   mul     := unary (('*' | '/' | '%') unary)*
//   unary   := ('!' | '-') unary | postfix
//   postfix := primary ('.' name)*
//   primary := number | string | true | false | null
//            | path (dotted identifiers into the scope)
//            | helperCall | methodCall | '(' expr ')'
//   helperCall := whitelistName '(' args? ')'   (fmtDb fmtHz fmtPan upper
//            lower pad concat coalesce count join clamp round)
//   methodCall := pathExpr '.has(' expr ')'     (only on arrays, e.g.
//            caps.has('eq_on'); every other method is rejected)
//
// Paths resolve into a plain scope object supplied by the renderer
// ({ store, item, index, payload, param, setting, caps, ... }). There is NO
// eval/Function/new Function: a hand-written tokenizer + recursive-descent
// parser builds an AST, and a step-budgeted interpreter walks it.
//
// Safety: identifiers `__proto__`, `constructor`, `prototype` are rejected
// at parse time (no prototype-chain access); only whitelisted helpers can be
// called; input is capped (length / tokens / depth / eval steps). Every entry
// returns { ok, ... } and never throws for bad input.
//
// Run: node --experimental-strip-types apps/web/src/ui/modules/expr.test.ts

export type Scope = Record<string, unknown>;

export interface EvalOk {
  ok: true;
  value: unknown;
}

export interface EvalErr {
  ok: false;
  error: string;
}

export type EvalResult = EvalOk | EvalErr;

export const EXPR_MAX_LEN = 500;
export const EXPR_MAX_TOKENS = 200;
export const EXPR_MAX_DEPTH = 20;
const EVAL_MAX_STEPS = 1000;

const BLOCKED_NAMES = new Set(['__proto__', 'constructor', 'prototype']);

// ---------- helpers (pure; mirror designData formatting conventions) ----------

function num(v: unknown): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string' && v.trim() !== '') {
    const n = Number(v);
    return Number.isFinite(n) ? n : NaN;
  }
  return NaN;
}

function str(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' && !Number.isFinite(v)) return '∞';
  return String(v);
}

function isEmpty(v: unknown): boolean {
  return v === null || v === undefined || v === '';
}

/** dB readout of a dB value: `-4.3`, `+2.0`, `-∞` below -89. */
function fmtDb(db: unknown): string {
  const d = num(db);
  if (!Number.isFinite(d) || d < -89) return '-∞';
  return (d > 0.04 ? '+' : '') + d.toFixed(1);
}

/** Frequency readout: `440 Hz`, `2.50 kHz`. */
function fmtHz(f: unknown): string {
  const n = num(f);
  if (!Number.isFinite(n)) return '';
  return n < 1000 ? Math.round(n) + ' Hz' : (n / 1000).toFixed(n < 10000 ? 2 : 1) + ' kHz';
}

/** Pan readout: `C`, `L28`, `R45`. */
function fmtPan(pan: unknown): string {
  if (pan === null || pan === undefined) return '';
  const p = num(pan);
  if (!Number.isFinite(p)) return '';
  return p === 0 ? 'C' : (p < 0 ? 'L' : 'R') + Math.round(Math.abs(p) * 100);
}

type Helper = (args: unknown[]) => unknown;

const HELPERS: Record<string, { fn: Helper; min: number; max: number }> = {
  fmtDb: { fn: a => fmtDb(a[0]), min: 1, max: 1 },
  fmtHz: { fn: a => fmtHz(a[0]), min: 1, max: 1 },
  fmtPan: { fn: a => fmtPan(a[0]), min: 1, max: 1 },
  upper: { fn: a => str(a[0]).toUpperCase(), min: 1, max: 1 },
  lower: { fn: a => str(a[0]).toLowerCase(), min: 1, max: 1 },
  pad: {
    fn: a => {
      const n = Math.trunc(num(a[1]));
      if (!Number.isFinite(n) || n < 0 || n > 64) return str(a[0]);
      const ch = a.length > 2 ? str(a[2]).slice(0, 1) || '0' : '0';
      return str(a[0]).padStart(n, ch);
    },
    min: 2,
    max: 3,
  },
  concat: { fn: a => a.map(str).join(''), min: 0, max: 8 },
  coalesce: { fn: a => { for (const x of a) if (!isEmpty(x)) return x; return ''; }, min: 1, max: 8 },
  count: {
    fn: a => {
      const x = a[0];
      if (Array.isArray(x)) return x.length;
      if (typeof x === 'string') return x.length;
      if (typeof x === 'object' && x !== null) return Object.keys(x).length;
      return 0;
    },
    min: 1,
    max: 1,
  },
  join: { fn: a => (Array.isArray(a[0]) ? (a[0] as unknown[]).map(str).join(str(a[1] ?? ',')) : str(a[0])), min: 1, max: 2 },
  clamp: {
    fn: a => {
      const v = num(a[0]), lo = num(a[1]), hi = num(a[2]);
      if (!Number.isFinite(v)) return 0;
      if (!Number.isFinite(lo) || !Number.isFinite(hi)) return v;
      return Math.max(lo, Math.min(hi, v));
    },
    min: 3,
    max: 3,
  },
  round: {
    fn: a => {
      const v = num(a[0]);
      const d = a.length > 1 ? Math.trunc(num(a[1])) : 0;
      if (!Number.isFinite(v)) return 0;
      if (!Number.isFinite(d) || d < 0 || d > 10) return Math.round(v);
      const f = 10 ** d;
      return Math.round(v * f) / f;
    },
    min: 1,
    max: 2,
  },
};

export function helperNames(): string[] {
  return Object.keys(HELPERS);
}

// ---------- tokenizer ----------

interface Tok {
  kind: 'num' | 'str' | 'name' | 'op' | 'lparen' | 'rparen' | 'comma' | 'q' | 'colon' | 'dot';
  text: string;
  value?: unknown;
}

const OPERATORS = ['==', '!=', '<=', '>=', '&&', '||', '+', '-', '*', '/', '%', '<', '>', '!'];

function tokenize(src: string): Tok[] | { error: string } {
  const toks: Tok[] = [];
  let i = 0;
  const err = (m: string): { error: string } => ({ error: m });
  while (i < src.length) {
    const c = src[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue; }
    if (c >= '0' && c <= '9' || (c === '.' && i + 1 < src.length && src[i + 1] >= '0' && src[i + 1] <= '9')) {
      const m = /^[0-9]*\.?[0-9]+(?:[eE][+-]?[0-9]+)?/.exec(src.slice(i));
      if (!m) return err(`bad number at ${i}`);
      toks.push({ kind: 'num', text: m[0], value: Number(m[0]) });
      i += m[0].length;
      continue;
    }
    if (c === "'" || c === '"') {
      let j = i + 1, out = '';
      let closed = false;
      while (j < src.length) {
        const d = src[j];
        if (d === '\\' && j + 1 < src.length) {
          const e = src[j + 1];
          out += e === 'n' ? '\n' : e === 't' ? '\t' : e;
          j += 2;
          continue;
        }
        if (d === c) { closed = true; break; }
        out += d;
        j++;
      }
      if (!closed) return err('unterminated string');
      toks.push({ kind: 'str', text: src.slice(i, j + 1), value: out });
      i = j + 1;
      continue;
    }
    if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c === '_' || c === '$') {
      const m = /^[A-Za-z_$][A-Za-z0-9_$]*/.exec(src.slice(i));
      const word = m![0];
      toks.push({ kind: 'name', text: word });
      i += word.length;
      continue;
    }
    if (c === '(') { toks.push({ kind: 'lparen', text: c }); i++; continue; }
    if (c === ')') { toks.push({ kind: 'rparen', text: c }); i++; continue; }
    if (c === ',') { toks.push({ kind: 'comma', text: c }); i++; continue; }
    if (c === '?') { toks.push({ kind: 'q', text: c }); i++; continue; }
    if (c === ':') { toks.push({ kind: 'colon', text: c }); i++; continue; }
    if (c === '.') { toks.push({ kind: 'dot', text: c }); i++; continue; }
    const two = src.slice(i, i + 2);
    if (OPERATORS.includes(two)) { toks.push({ kind: 'op', text: two }); i += 2; continue; }
    if (OPERATORS.includes(c)) { toks.push({ kind: 'op', text: c }); i += 1; continue; }
    return err(`unexpected character ${JSON.stringify(c)} at ${i}`);
  }
  return toks;
}

// ---------- parser ----------

type Ast =
  | { t: 'lit'; value: unknown }
  | { t: 'path'; parts: string[] }
  | { t: 'un'; op: '!' | '-'; arg: Ast }
  | { t: 'bin'; op: string; left: Ast; right: Ast }
  | { t: 'tern'; cond: Ast; yes: Ast; no: Ast }
  | { t: 'call'; name: string; args: Ast[] }
  | { t: 'method'; obj: Ast; method: string; args: Ast[] };

class Parser {
  private pos = 0;
  private toks: Tok[];
  constructor(toks: Tok[]) {
    this.toks = toks;
  }

  parse(): Ast | { error: string } {
    const e = this.ternary(0);
    if (typeof (e as { error?: string }).error === 'string') return e as { error: string };
    if (this.pos < this.toks.length) return { error: `unexpected ${JSON.stringify(this.toks[this.pos].text)}` };
    return e as Ast;
  }

  private peek(): Tok | undefined {
    return this.toks[this.pos];
  }

  private eat(kind: Tok['kind'], text?: string): Tok | undefined {
    const t = this.peek();
    if (t && t.kind === kind && (text === undefined || t.text === text)) {
      this.pos++;
      return t;
    }
    return undefined;
  }

  private ternary(depth: number): Ast | { error: string } {
    if (depth > EXPR_MAX_DEPTH) return { error: 'expression too deep' };
    const c = this.or(depth + 1);
    if ('error' in c) return c;
    if (!this.eat('q')) return c as Ast;
    const yes = this.ternary(depth + 1);
    if ('error' in yes) return yes;
    if (!this.eat('colon')) return { error: "expected ':' in ternary" };
    const no = this.ternary(depth + 1);
    if ('error' in no) return no;
    return { t: 'tern', cond: c as Ast, yes: yes as Ast, no: no as Ast };
  }

  private leftOp(next: (d: number) => Ast | { error: string }, depth: number, ops: string[]): Ast | { error: string } {
    // Operator-chain levels share one depth budget; only real nesting
    // (branches, calls, parens, unary) increments it.
    let left = next(depth);
    if ('error' in left) return left;
    for (;;) {
      const t = this.peek();
      if (!t || t.kind !== 'op' || !ops.includes(t.text)) return left;
      this.pos++;
      const right = next(depth);
      if ('error' in right) return right;
      left = { t: 'bin', op: t.text, left: left as Ast, right: right as Ast };
    }
  }

  private or(d: number): Ast | { error: string } { return this.leftOp((x) => this.and(x), d, ['||']); }
  private and(d: number): Ast | { error: string } { return this.leftOp((x) => this.eq(x), d, ['&&']); }
  private eq(d: number): Ast | { error: string } { return this.leftOp((x) => this.rel(x), d, ['==', '!=']); }
  private rel(d: number): Ast | { error: string } { return this.leftOp((x) => this.add(x), d, ['<', '>', '<=', '>=']); }
  private add(d: number): Ast | { error: string } { return this.leftOp((x) => this.mul(x), d, ['+', '-']); }
  private mul(d: number): Ast | { error: string } { return this.leftOp((x) => this.unary(x), d, ['*', '/', '%']); }

  private unary(depth: number): Ast | { error: string } {
    if (depth > EXPR_MAX_DEPTH) return { error: 'expression too deep' };
    const t = this.peek();
    if (t && t.kind === 'op' && (t.text === '!' || t.text === '-')) {
      this.pos++;
      const arg = this.unary(depth + 1);
      if ('error' in arg) return arg;
      return { t: 'un', op: t.text as '!' | '-', arg: arg as Ast };
    }
    return this.postfix(depth);
  }

  private postfix(depth: number): Ast | { error: string } {
    if (depth > EXPR_MAX_DEPTH) return { error: 'expression too deep' };
    let node = this.primary(depth);
    if ('error' in node) return node;
    for (;;) {
      if (!this.eat('dot')) return node;
      const name = this.eat('name');
      if (!name) return { error: "expected name after '.'" };
      if (BLOCKED_NAMES.has(name.text)) return { error: `blocked property ${JSON.stringify(name.text)}` };
      if (this.peek()?.kind === 'lparen') {
        // method call: only .has(...) on a path value is allowed
        this.pos++;
        const args = this.argList(depth + 1);
        if ('error' in args) return args;
        node = { t: 'method', obj: node as Ast, method: name.text, args: args as Ast[] };
      } else {
        if ((node as Ast).t !== 'path') return { error: `property access only on paths` };
        (node as { t: 'path'; parts: string[] }).parts.push(name.text);
      }
    }
  }

  private argList(depth: number): Ast[] | { error: string } {
    const args: Ast[] = [];
    if (this.eat('rparen')) return args;
    for (;;) {
      const e = this.ternary(depth + 1);
      if ('error' in e) return e;
      args.push(e as Ast);
      if (this.eat('comma')) continue;
      if (this.eat('rparen')) return args;
      return { error: "expected ',' or ')'" };
    }
  }

  private primary(depth: number): Ast | { error: string } {
    if (depth > EXPR_MAX_DEPTH) return { error: 'expression too deep' };
    const t = this.peek();
    if (!t) return { error: 'unexpected end of expression' };
    if (t.kind === 'num' || t.kind === 'str') {
      this.pos++;
      return { t: 'lit', value: t.value };
    }
    if (t.kind === 'name') {
      if (t.text === 'true' || t.text === 'false' || t.text === 'null') {
        this.pos++;
        return { t: 'lit', value: t.text === 'true' ? true : t.text === 'false' ? false : null };
      }
      if (BLOCKED_NAMES.has(t.text)) return { error: `blocked name ${JSON.stringify(t.text)}` };
      this.pos++;
      // helper call?
      if (this.peek()?.kind === 'lparen' && Object.hasOwn(HELPERS, t.text)) {
        this.pos++;
        const args = this.argList(depth + 1);
        if ('error' in args) return args;
        return { t: 'call', name: t.text, args: args as Ast[] };
      }
      if (this.peek()?.kind === 'lparen') return { error: `unknown function ${JSON.stringify(t.text)}` };
      return { t: 'path', parts: [t.text] };
    }
    if (t.kind === 'lparen') {
      this.pos++;
      const e = this.ternary(depth + 1);
      if ('error' in e) return e;
      if (!this.eat('rparen')) return { error: "expected ')'" };
      return e as Ast;
    }
    return { error: `unexpected ${JSON.stringify(t.text)}` };
  }
}

// ---------- interpreter ----------

function truthy(v: unknown): boolean {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0 && !Number.isNaN(v);
  if (typeof v === 'string') return v.length > 0;
  return v !== null && v !== undefined;
}

function looseEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if ((typeof a === 'number' || typeof b === 'number') && (typeof a === 'string' || typeof b === 'string')) {
    const x = num(a), y = num(b);
    if (Number.isFinite(x) && Number.isFinite(y)) return x === y;
  }
  if (typeof a === 'boolean' || typeof b === 'boolean') return truthy(a) === truthy(b);
  // eslint-disable-next-line eqeqeq
  return a == b;
}

function lookupPath(scope: Scope, parts: string[]): unknown {
  let cur: unknown = scope;
  for (const p of parts) {
    if (typeof cur !== 'object' || cur === null) return undefined;
    if (Object.hasOwn(cur, '__proto__') && p === '__proto__') return undefined;
    cur = (cur as Record<string, unknown>)[p];
  }
  return cur;
}

class Runner {
  steps = 0;
  scope: Scope;
  constructor(scope: Scope) {
    this.scope = scope;
  }

  run(n: Ast): unknown {
    if (++this.steps > EVAL_MAX_STEPS) throw new Error('evaluation budget exceeded');
    switch (n.t) {
      case 'lit': return n.value;
      case 'path': return lookupPath(this.scope, n.parts);
      case 'un': {
        const v = this.run(n.arg);
        return n.op === '!' ? !truthy(v) : -num(v);
      }
      case 'bin': {
        const l = this.run(n.left);
        const r = this.run(n.right);
        switch (n.op) {
          case '+': {
            // string concat when either side is a string, else numeric add
            if (typeof l === 'string' || typeof r === 'string') return str(l) + str(r);
            return num(l) + num(r);
          }
          case '-': return num(l) - num(r);
          case '*': return num(l) * num(r);
          case '/': return num(l) / num(r);
          case '%': return num(l) % num(r);
          case '==': return looseEq(l, r);
          case '!=': return !looseEq(l, r);
          case '<': return num(l) < num(r);
          case '>': return num(l) > num(r);
          case '<=': return num(l) <= num(r);
          case '>=': return num(l) >= num(r);
          case '&&': return truthy(l) ? r : l;
          case '||': return truthy(l) ? l : r;
          default: throw new Error(`bad operator ${n.op}`);
        }
      }
      case 'tern': return truthy(this.run(n.cond)) ? this.run(n.yes) : this.run(n.no);
      case 'call': {
        const h = HELPERS[n.name];
        if (!h) throw new Error(`unknown function ${n.name}`);
        if (n.args.length < h.min || n.args.length > h.max) {
          throw new Error(`${n.name} takes ${h.min}-${h.max} args`);
        }
        return h.fn(n.args.map(a => this.run(a)));
      }
      case 'method': {
        if (n.method !== 'has' || n.args.length !== 1) throw new Error(`unsupported method .${n.method}`);
        const obj = this.run(n.obj);
        const arg = this.run(n.args[0]);
        if (Array.isArray(obj)) return obj.includes(arg);
        if (obj instanceof Set) return obj.has(arg);
        throw new Error('.has() only works on arrays');
      }
    }
  }
}

/** Parse an expression to an AST (for validators/editors). Never throws. */
export function parseExpr(src: string): { ok: true; ast: Ast } | { ok: false; error: string } {
  try {
    if (typeof src !== 'string') return { ok: false, error: 'expression must be a string' };
    if (src.length === 0) return { ok: false, error: 'empty expression' };
    if (src.length > EXPR_MAX_LEN) return { ok: false, error: `expression too long (>${EXPR_MAX_LEN})` };
    const toks = tokenize(src);
    if ('error' in toks) return { ok: false, error: toks.error };
    if (toks.length === 0) return { ok: false, error: 'empty expression' };
    if (toks.length > EXPR_MAX_TOKENS) return { ok: false, error: `too many tokens (>${EXPR_MAX_TOKENS})` };
    const ast = new Parser(toks).parse();
    if ('error' in ast) return { ok: false, error: ast.error };
    return { ok: true, ast };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : 'parse failed' };
  }
}

/** Evaluate an expression against a scope. Never throws. */
export function evalExpr(src: string, scope: Scope): EvalResult {
  const p = parseExpr(src);
  if (!p.ok) return p;
  try {
    return { ok: true, value: new Runner(scope).run(p.ast) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : 'eval failed' };
  }
}

export interface BindingInfo {
  ok: boolean;
  /** Root scope paths read, e.g. `store.mode`, `item.id`. */
  refs: string[];
  /** Whitelisted helpers called. */
  calls: string[];
  error?: string;
}

function walkAst(n: Ast, refs: Set<string>, calls: Set<string>, prefix: string[]): void {
  switch (n.t) {
    case 'lit': return;
    case 'path': refs.add([...prefix, ...n.parts].join('.')); return;
    case 'un': walkAst(n.arg, refs, calls, prefix); return;
    case 'bin': walkAst(n.left, refs, calls, prefix); walkAst(n.right, refs, calls, prefix); return;
    case 'tern':
      walkAst(n.cond, refs, calls, prefix);
      walkAst(n.yes, refs, calls, prefix);
      walkAst(n.no, refs, calls, prefix);
      return;
    case 'call':
      calls.add(n.name);
      for (const a of n.args) walkAst(a, refs, calls, prefix);
      return;
    case 'method':
      walkAst(n.obj, refs, calls, prefix);
      for (const a of n.args) walkAst(a, refs, calls, prefix);
      return;
  }
}

/** Describe an expression for the editor: which scope paths it reads and
 *  which helpers it calls. Never throws. */
export function describeBinding(src: string): BindingInfo {
  const p = parseExpr(src);
  if (!p.ok) return { ok: false, refs: [], calls: [], error: p.error };
  const refs = new Set<string>();
  const calls = new Set<string>();
  walkAst(p.ast, refs, calls, []);
  return { ok: true, refs: [...refs], calls: [...calls] };
}

/** Legacy catalogue binding strings (`param:<id>`, `meter:<tap>`, bare
 *  `strip`/`bank`/`scene`, `setting:<key>`): accepted wherever an expr is
 *  accepted and resolved through the ModuleContext instead of the scope. */
const LEGACY_RE = /^(param|meter|strip|bank|scene|setting):[^:]+$/;

export function isLegacyBinding(s: string): boolean {
  return s === 'strip' || s === 'bank' || s === 'scene' || LEGACY_RE.test(s);
}

// ---------- {{ }} interpolation ----------

const INTERP_RE = /\{\{\s*([\s\S]*?)\s*\}\}/g;

/** Render a text template, replacing `{{ expr }}` holes with scope values.
 *  Nullish holes render as ''. Never throws; errors yield '' (the caller
 *  warns once). */
export function renderText(template: string, scope: Scope): string {
  if (!template.includes('{{')) return template;
  return template.replace(INTERP_RE, (_m, expr: string) => {
    const r = evalExpr(expr, scope);
    if (!r.ok) return '';
    const v = (r as EvalOk).value;
    if (v === null || v === undefined) return '';
    if (typeof v === 'object') return JSON.stringify(v);
    return String(v);
  });
}

/** True when a string value carries at least one `{{ }}` hole. */
export function hasInterpolation(s: string): boolean {
  return s.includes('{{') && s.includes('}}');
}
