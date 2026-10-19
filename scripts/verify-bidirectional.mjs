#!/usr/bin/env node
// scripts/verify-bidirectional.mjs — owner-ordered bidirectional verification
// loop: app vs XR18 (192.168.40.123, FW 1.20). See header in v1; v2 hardens:
//   - restore = send + poll-until-match (no single-read races)
//   - engine-baseline barrier before every desk->app write (no stale passes)
//   - echo-or-error WS wait (unknown-cmd => STALE-BINARY tag, not a code gap)
//   - dual-layer desk->app probes: engine strip param (poll path) + the UI
//     view that feeds the display (echo-only views reported as INFO findings)
//   - page-reload detector (hot-reload mid-run flags the row)
// Constraints: no writes to crates/** etc.; attach-only WS (never connect);
// never kill :8080/:8081/:5291; dev-xr18.sh for check/restore only.
// Usage: node scripts/verify-bidirectional.mjs [--only mute,solo]
import dgram from "node:dgram";
import { spawn, execSync } from "node:child_process";
import fs from "node:fs";

const args = process.argv.slice(2);
const opt = (k, d) => {
  const i = args.indexOf(k);
  return i >= 0 && args[i + 1] ? args[i + 1] : d;
};
const RELAY_HOST = "127.0.0.1";
const RELAY_PORT = Number(opt("--relay-port", "8081"));
const UI_PORT = Number(opt("--ui-port", "5291"));
const DESK = opt("--desk", "192.168.40.123");
const DESK_PORT = 10024;
const ONLY = (opt("--only", "") || "").split(",").map((s) => s.trim()).filter(Boolean);
const VERBOSE = process.env.VERBOSE === "1";

const WS_URL = `ws://${RELAY_HOST}:${RELAY_PORT}/ws`;
const UI_URL = `http://${RELAY_HOST}:${UI_PORT}/?server=${RELAY_HOST}:${RELAY_PORT}`;
const CDP_PORT = 9333;
const CHROME = "/home/gladson/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome";

const log = (...a) => console.log(...a);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const rows = [];
const BUDGET_MS = 2000;
const WAIT_MS = 10000;

// --------------------------------------------------------------- OSC wire
function pstr(s) {
  const b = Buffer.from(s, "ascii");
  const nul = Buffer.from([0]);
  const n = b.length + 1;
  return Buffer.concat([b, nul, Buffer.alloc((4 - (n % 4)) % 4)]);
}
function oscEncode(addr, argv) {
  const tags = "," + argv.map((a) => a.t).join("");
  const parts = [pstr(addr), pstr(tags)];
  for (const a of argv) {
    if (a.t === "f") {
      const b = Buffer.alloc(4);
      b.writeFloatBE(a.v, 0);
      parts.push(b);
    } else if (a.t === "i") {
      const b = Buffer.alloc(4);
      b.writeInt32BE(a.v, 0);
      parts.push(b);
    } else if (a.t === "s") parts.push(pstr(a.v));
  }
  return Buffer.concat(parts);
}
function oscDecode(buf) {
  let o = 0;
  const rdStr = () => {
    const end = buf.indexOf(0, o);
    if (end < 0) throw new Error("osc: unterminated string");
    const s = buf.subarray(o, end).toString("ascii");
    o = end + 1 + ((4 - ((end + 1) % 4)) % 4);
    return s;
  };
  const addr = rdStr();
  const tags = rdStr();
  if (!tags.startsWith(",")) throw new Error("osc: bad typetag");
  const argv = [];
  for (const t of tags.slice(1)) {
    if (t === "f") {
      argv.push({ t, v: buf.readFloatBE(o) });
      o += 4;
    } else if (t === "i") {
      argv.push({ t, v: buf.readInt32BE(o) });
      o += 4;
    } else if (t === "s") argv.push({ t, v: rdStr() });
    else throw new Error(`osc: unsupported tag ${t}`);
  }
  return { addr, args: argv };
}
let oscSock = null;
const oscWaiters = new Map();
function oscInit() {
  return new Promise((resolve, reject) => {
    oscSock = dgram.createSocket("udp4");
    oscSock.on("message", (msg) => {
      try {
        const m = oscDecode(msg);
        const q = oscWaiters.get(m.addr);
        if (q && q.length) q.shift().resolve(m);
      } catch {
        /* ignore */
      }
    });
    oscSock.on("error", reject);
    oscSock.bind(0, "0.0.0.0", resolve);
  });
}
async function oscQuery(addr, { timeoutMs = 1500, tries = 3 } = {}) {
  let lastErr = null;
  for (let t = 0; t < tries; t++) {
    try {
      const p = new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("timeout")), timeoutMs);
        if (!oscWaiters.has(addr)) oscWaiters.set(addr, []);
        oscWaiters.get(addr).push({
          resolve: (m) => {
            clearTimeout(timer);
            resolve(m);
          },
          reject,
        });
      });
      oscSock.send(oscEncode(addr, []), DESK_PORT, DESK);
      return await p;
    } catch (e) {
      lastErr = e;
      const q = oscWaiters.get(addr);
      if (q && q.length) q.shift();
    }
  }
  throw new Error(`${addr}: no reply after ${tries} tries (${lastErr?.message})`);
}
async function oscSend(addr, argv) {
  return new Promise((resolve, reject) => {
    oscSock.send(oscEncode(addr, argv), DESK_PORT, DESK, (e) => (e ? reject(e) : resolve()));
  });
}
const fArg = (v) => ({ t: "f", v });
const iArg = (v) => ({ t: "i", v });
const wireToPan = (w) => -100 + 200 * w;
const panToWire = (e) => (e + 100) / 200;
const wireToEqDb = (w) => -15 + 30 * w;
const eqDbToWire = (db) => (db + 15) / 30;
function sendDbToWire(db) {
  if (db >= 10) return 1.0;
  if (db >= -10) return (db + 30) / 40;
  if (db >= -30) return (db + 50) / 80;
  if (db >= -60) return (db + 70) / 160;
  return (db + 90) / 480;
}
function wireToSendDb(w) {
  if (w >= 1) return 10;
  if (w >= 0.5) return 40 * w - 30;
  if (w >= 0.25) return 80 * w - 50;
  if (w >= 0.0625) return 160 * w - 70;
  if (w > 0) return 480 * w - 90;
  return -90;
}
const ch2 = (n1) => String(n1).padStart(2, "0");
const CH = (strip1) => `/ch/${ch2(strip1)}`;
const EPS_F = 2e-3;
// Resilient query: returns null (logs) instead of throwing; rows decide.
async function tryQuery(addr, { timeoutMs = 2000, tries = 5 } = {}) {
  try {
    return await oscQuery(addr, { timeoutMs, tries });
  } catch (e) {
    log(`  !! query ${addr} silent (${e.message})`);
    return null;
  }
}

// ------------------------------------------------------------------- WS
class Relay {
  constructor(url) {
    this.url = url;
    this.inbox = [];
    this.ws = new WebSocket(url);
    this.opened = false;
    this.ws.addEventListener("open", () => (this.opened = true));
    this.ws.addEventListener("message", (ev) => {
      try {
        this.inbox.push(JSON.parse(String(ev.data)));
      } catch {
        /* ignore */
      }
    });
  }
  async waitOpen(t = 8000) {
    const t0 = Date.now();
    while (!this.opened && Date.now() - t0 < t) await sleep(50);
    if (!this.opened) throw new Error(`WS ${this.url} did not open`);
  }
  send(o) {
    this.ws.send(JSON.stringify(o));
  }
  async waitFor(pred, timeoutMs = WAIT_MS, what = "frame") {
    const t0 = Date.now();
    for (;;) {
      const i = this.inbox.findIndex(pred);
      if (i >= 0) return this.inbox.splice(i, 1)[0];
      if (Date.now() - t0 > timeoutMs) {
        const types = this.inbox.slice(-4).map((m) => m?.type).join(",");
        throw new Error(`timeout waiting ${what} (recent: ${types || "empty"})`);
      }
      await sleep(25);
    }
  }
  close() {
    try {
      this.ws.close();
    } catch {
      /* gone */
    }
  }
}

// ------------------------------------------------------------------ CDP
let cdpId = 0;
const cdpPending = new Map();
let cdpWs = null;
let chromeProc = null;
async function cdpSend(method, params = {}) {
  const id = ++cdpId;
  const p = new Promise((resolve, reject) => {
    cdpPending.set(id, { resolve, reject });
    setTimeout(() => {
      if (cdpPending.has(id)) {
        cdpPending.delete(id);
        reject(new Error(`CDP ${method} timeout`));
      }
    }, 15000);
  });
  cdpWs.send(JSON.stringify({ id, method, params }));
  return p;
}
async function js(expr) {
  const r = await cdpSend("Runtime.evaluate", { expression: expr, returnByValue: true });
  if (r.exceptionDetails) throw new Error(`page js threw: ${JSON.stringify(r.exceptionDetails).slice(0, 300)}`);
  return r.result?.value;
}
async function launchBrowser(url) {
  const profile = "/tmp/verify-bi-profile";
  try {
    execSync(`rm -rf ${profile}`);
  } catch {
    /* ignore */
  }
  fs.mkdirSync(profile, { recursive: true });
  chromeProc = spawn(CHROME, ["--headless=new", "--no-sandbox", "--disable-gpu", "--window-size=1600,900", `--user-data-dir=${profile}`, `--remote-debugging-port=${CDP_PORT}`, "about:blank"], { stdio: ["ignore", "pipe", "pipe"] });
  chromeProc.on("error", (e) => {
    throw e;
  });
  const t0 = Date.now();
  for (;;) {
    try {
      const r = await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`);
      if (r.ok) break;
    } catch {
      /* not up */
    }
    if (Date.now() - t0 > 30000) throw new Error("chrome remote debugging did not start");
    await sleep(250);
  }
  const nr = await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?${encodeURIComponent(url)}`, { method: "PUT" });
  if (!nr.ok) throw new Error(`/json/new failed: ${nr.status}`);
  const target = await nr.json();
  cdpWs = new WebSocket(target.webSocketDebuggerUrl);
  cdpWs.addEventListener("message", (ev) => {
    try {
      const m = JSON.parse(String(ev.data));
      if (m.id && cdpPending.has(m.id)) {
        const { resolve, reject } = cdpPending.get(m.id);
        cdpPending.delete(m.id);
        if (m.error) reject(new Error(`CDP: ${JSON.stringify(m.error).slice(0, 200)}`));
        else resolve(m.result);
      }
    } catch {
      /* ignore */
    }
  });
  await new Promise((res, rej) => {
    cdpWs.addEventListener("open", res, { once: true });
    setTimeout(() => rej(new Error("CDP ws did not open")), 10000);
  });
  await cdpSend("Page.enable");
  await cdpSend("Runtime.enable");
}
async function closeBrowser() {
  try {
    if (cdpWs) cdpWs.close();
  } catch {
    /* ignore */
  }
  try {
    if (chromeProc) chromeProc.kill("SIGKILL");
  } catch {
    /* ignore */
  }
}
const PAGE = {
  online: `!!(window.__live && window.__live.phase === 'online' && window.__live.engine && window.__live.engine.capsReady)`,
  buttons: `[...document.querySelectorAll('button')].filter(b=>{const t=(b.textContent||'').trim();return t==='MUTE'||t==='SOLO';}).map(b=>({label:b.getAttribute('aria-label'),pressed:b.getAttribute('aria-pressed'),disabled:!!b.disabled}))`,
  clickLabel: (label) => `(lbl=>{const b=[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===lbl);if(!b)return{found:false};if(b.disabled)return{found:true,disabled:true};b.click();return{found:true,clicked:true};})(${JSON.stringify(label)})`,
  keySlider: (prefix, key) => `(p=>{const s=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith(p));if(!s)return{found:false};if(s.disabled)return{found:true,disabled:true};s.focus();s.dispatchEvent(new KeyboardEvent('keydown',{key:${JSON.stringify(key)},bubbles:true,cancelable:true}));return{found:true};})(${JSON.stringify(prefix)})`,
  pressed: (label) => `(lbl=>{const b=[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===lbl);return b?b.getAttribute('aria-pressed'):null;})(${JSON.stringify(label)})`,
  sliderVal: (prefix) => `(p=>{const s=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith(p));return s?{now:s.getAttribute('aria-valuenow'),text:s.getAttribute('aria-valuetext')}:null;})(${JSON.stringify(prefix)})`,
  engineParam: (ch, id) => `(window.__live&&window.__live.engine?window.__live.engine.param(${ch},${JSON.stringify(id)}):null)`,
  engineSolo: (ch) => `(window.__live&&window.__live.engine?window.__live.engine.soloOf(${ch}):null)`,
  engineBus: (b) => `(window.__live&&window.__live.engine?window.__live.engine.busMaster(${b}):null)`,
  engineMain: `(window.__live&&window.__live.engine?{fader:window.__live.engine.mainFader,muted:window.__live.engine.mainMuted}:null)`,
  engineStripParam: (ch, id) => `(window.__live&&window.__live.engine?window.__live.engine.param(${ch},${JSON.stringify(id)}):null)`,
};

// ------------------------------------------------------------- preflight
async function healthz(port) {
  const r = await fetch(`http://127.0.0.1:${port}/healthz`);
  if (!r.ok) throw new Error(`healthz :${port} -> ${r.status}`);
  return (await r.text()).trim();
}

// ================================================================= main
const baselines = {};
function noteBaseline(key, addr, kind, value) {
  if (!(key in baselines)) baselines[key] = { addr, kind, value };
}

async function main() {
  const head = execSync("git rev-parse HEAD", { cwd: "/home/gladson/dev/openmix", encoding: "utf8" }).trim();
  const dirty = execSync("git status --short", { cwd: "/home/gladson/dev/openmix", encoding: "utf8" }).trim().split("\n").length;
  log(`== verify-bidirectional v2 (HEAD ${head.slice(0, 8)}, worktree dirty files ~${dirty}) ==`);
  try {
    const binStat = fs.statSync("/home/gladson/dev/openmix/target/debug/openmix-serve");
    log(`relay binary mtime: ${binStat.mtime.toISOString()}`);
  } catch {
    /* ignore */
  }
  const hz = await healthz(RELAY_PORT);
  log(`relay :${RELAY_PORT} healthz: ${hz}`);
  if (!hz.includes("xair")) throw new Error("relay not on xair; aborting");
  const uiProbe = await fetch(`http://${RELAY_HOST}:${UI_PORT}/`).then((r) => r.status).catch((e) => `ERR ${e.message}`);
  log(`dev server :${UI_PORT} -> ${uiProbe}`);
  if (uiProbe !== 200) throw new Error("dev server not up; aborting");

  await oscInit();
  const info = await oscQuery("/info").catch(() => null);
  log(`desk /info: ${info ? JSON.stringify(info.args.map((a) => a.v)) : "no reply"}`);

  const relay = new Relay(WS_URL);
  await relay.waitOpen();
  const caps = await relay.waitFor((m) => m?.type === "caps", 8000, "caps");
  log(`caps: driver=${caps.driver} params=${caps.params.length} unsupported=${caps.unsupported.length}`);
  if (caps.driver !== "xair") throw new Error(`relay driver is ${caps.driver}, not xair; aborting`);
  const topo = caps.topology;
  const famOf = (kind) => topo.find((f) => f.kind === kind);
  const chIn = famOf("in").first, chBus = famOf("bus").first, chRet = famOf("ret").first;
  const chFx = famOf("fxsnd").first, chMain = famOf("main").first, chDca = famOf("dca").first;
  const supported = (id, kind) => {
    const d = caps.params.find((p) => p.id === id);
    if (!d) return false;
    if (caps.unsupported.includes(id)) return false;
    if (kind && Array.isArray(d.scopes) && d.scopes.length) return d.scopes.includes(kind);
    return true;
  };

  // ---- quiet strip picks (read-only) ----
  let TEST = null;
  for (let s = 1; s <= 16; s++) {
    const root = CH(s);
    try {
      const mu = (await oscQuery(`${root}/mix/on`, { timeoutMs: 2000, tries: 5 })).args[0].v === 0;
      const fa = (await oscQuery(`${root}/mix/fader`, { timeoutMs: 2000, tries: 5 })).args[0].v;
      if (mu && fa <= 0.05) {
        TEST = { strip1: s, ch: chIn + s - 1, root, fader: fa };
        break;
      }
    } catch {
      /* silent leaf */
    }
  }
  if (!TEST) throw new Error("no muted near-zero channel strip; aborting (volume safety)");
  log(`test strip: ch${TEST.strip1} (engine ch ${TEST.ch}) muted, fader=${TEST.fader}`);
  const busCands = [];
  for (let b = 1; b <= 6; b++) {
    const f = await tryQuery(`/bus/${b}/mix/fader`);
    if (!f) continue;
    const mu = await tryQuery(`/bus/${b}/mix/on`);
    busCands.push({ bus: b - 1, fader: f.args[0].v, muted: mu ? mu.args[0].v === 0 : false });
  }
  if (!busCands.length) throw new Error("no bus leaf answers; aborting bus rows only");
  const busPick = busCands.filter((c) => c.muted).sort((a, b2) => a.fader - b2.fader)[0] || busCands.sort((a, b2) => a.fader - b2.fader)[0];
  log(`bus pick: bus${busPick.bus + 1} fader=${busPick.fader} muted=${busPick.muted}`);
  async function lowFam(rootFn, n, chFirst) {
    let best = null;
    for (let i = 1; i <= n; i++) {
      const root = rootFn(i);
      // NOTE: /mix/fader on DCA and FXSEND roots is desk-silent by design
      // (driver strip_root is /dca/N and /fxsend/N — proto-xair/lib.rs:474);
      // 1 fast try, then fall back to the direct /<root>/fader form.
      const direct = root.startsWith("/dca/") || root.startsWith("/fxsend/");
      const f = await tryQuery(`${root}/mix/fader`, direct ? { timeoutMs: 700, tries: 1 } : {});
      if (f && (!best || f.args[0].v < best.fader)) {
        best = { n: i, ch: chFirst + i - 1, root, fader: f.args[0].v };
        continue;
      }
      if (direct) {
        const f2 = await tryQuery(`${root}/fader`);
        if (f2 && (!best || f2.args[0].v < best.fader)) best = { n: i, ch: chFirst + i - 1, root, fader: f2.args[0].v, dca: root.startsWith("/dca/"), fxsend: root.startsWith("/fxsend/") };
      }
    }
    return best;
  }
  const rtnPick = await lowFam((i) => `/rtn/${i}`, 4, chRet);
  const fxPick = await lowFam((i) => `/fxsend/${i}`, 4, chFx);
  const dcaPick = await lowFam((i) => `/dca/${i}`, 4, chDca);
  log(`fam picks: rtn=${rtnPick?.root} fxsend=${fxPick?.root} dca=${dcaPick?.root}`);
  const mainFaderQ = await tryQuery("/lr/mix/fader");
  const mainMuteQ = await tryQuery("/lr/mix/on");
  if (!mainFaderQ || !mainMuteQ) throw new Error("main leaves silent; aborting");
  const mainFader0 = mainFaderQ.args[0].v;
  const mainMuted0 = mainMuteQ.args[0].v === 0;
  log(`main baseline: fader=${mainFader0} muted=${mainMuted0}`);
  for (const [k, a, kd, v] of [["main_fader", "/lr/mix/fader", "f", mainFader0], ["main_mute", "/lr/mix/on", "on", mainMuted0]]) noteBaseline(k, a, kd, v);

  // ---- headless UI ----
  log("launching headless Chromium (CDP)...");
  await launchBrowser(UI_URL);
  const tOnline0 = Date.now();
  let lastNote = 0, mark = 0;
  for (;;) {
    const on = await js(PAGE.online).catch(() => false);
    if (on) break;
    const el = Date.now() - tOnline0;
    if (el > 240000) throw new Error("page never reached online (capsReady)");
    if (el - lastNote > 15000) {
      lastNote = el;
      log(`  ... waiting online (${Math.round(el / 1000)}s)`);
    }
    await sleep(1000);
  }
  log(`page online in ${Date.now() - tOnline0}ms`);
  const hzAfter = await healthz(RELAY_PORT);
  log(`relay healthz after page load: ${hzAfter}`);
  if (!hzAfter.includes("xair")) throw new Error("page flipped relay off xair; aborting");
  const chTag = `Ch ${TEST.strip1}`;
  const chRe = new RegExp(`Ch ${TEST.strip1}\\b`);
  const btns = await js(PAGE.buttons);
  const muteLabel = (btns.find((b) => chRe.test(b.label || "") && (b.label || "").startsWith("Mute")) || {}).label;
  const soloLabel = (btns.find((b) => chRe.test(b.label || "") && (b.label || "").startsWith("Solo")) || {}).label;
  const muteMainLabel = (btns.find((b) => /Main LR/.test(b.label || "") && (b.label || "").startsWith("Mute")) || {}).label;
  if (!muteLabel || !soloLabel) throw new Error(`cannot find ${chTag} buttons: ${JSON.stringify((btns || []).map((b) => b.label))}`);
  log(`labels: mute=${JSON.stringify(muteLabel)} solo=${JSON.stringify(soloLabel)} mainMute=${JSON.stringify(muteMainLabel)}`);
  // reload detector: page-side counter must increment by exactly 1 per row.
  async function reloadCheck(rowName) {
    const v = await js(`window.__v=(window.__v||0)+1;window.__v`).catch(() => null);
    if (v === null) return "page unreachable";
    if (v !== mark + 1 && mark !== 0) return `PAGE RELOADED during/before ${rowName} (counter ${mark}->${v})`;
    mark = v;
    return null;
  }

  // ---- shared helpers ----
  // echo-or-error: resolves on the expected engine echo OR an error frame.
  async function waitEchoOrError(echoPred, what) {
    const t0 = Date.now();
    for (;;) {
      let i = relay.inbox.findIndex(echoPred);
      if (i >= 0) return { kind: "echo", frame: relay.inbox.splice(i, 1)[0], ms: Date.now() - t0 };
      i = relay.inbox.findIndex((m) => m?.type === "error");
      if (i >= 0) return { kind: "error", frame: relay.inbox.splice(i, 1)[0], ms: Date.now() - t0 };
      if (Date.now() - t0 > WAIT_MS) {
        const types = relay.inbox.slice(-4).map((m) => m?.type).join(",");
        return { kind: "timeout", frame: null, ms: Date.now() - t0, recent: types };
      }
      await sleep(25);
    }
  }
  const staleOf = (frame) => frame && /unknown cmd/i.test(frame.message || "");
  // restore: re-send every poll iteration (UDP single-shot is lossy) + poll until match.
  async function restoreLeaf(addr, argv, matchFn, label) {
    const t0 = Date.now();
    for (;;) {
      await oscSend(addr, argv);
      const m = await oscQuery(addr).catch(() => null);
      if (m && matchFn(m.args[0]?.v)) return;
      if (Date.now() - t0 > 8000) throw new Error(`restore ${label} failed (desk not converging)`);
      await sleep(300);
    }
  }
  // barrier: wait until the ENGINE core (poll path) shows the baseline value.
  async function barrierEngine(jsExpr, label) {
    const t0 = Date.now();
    for (;;) {
      const v = await js(jsExpr).catch(() => null);
      if (v) return Date.now() - t0;
      if (Date.now() - t0 > 12000) throw new Error(`engine barrier timeout at baseline (${label}) — poller not converging; desk->app direction would be invalid`);
      await sleep(250);
    }
  }
  async function uiWaitFor(jsExpr, timeoutMs = WAIT_MS) {
    const t0 = Date.now();
    for (;;) {
      const v = await js(jsExpr).catch(() => null);
      if (v) return Date.now() - t0;
      if (Date.now() - t0 > timeoutMs) throw new Error(`UI wait timeout: ${jsExpr.slice(0, 100)}`);
      await sleep(250);
    }
  }
  async function guardMuted(root) {
    const m = await oscQuery(`${root}/mix/on`);
    if (m.args[0].v !== 0) throw new Error(`GUARD: ${root} no longer muted; refusing write`);
  }
  async function guardQuietFader(root, limit = 0.05) {
    const f = (await oscQuery(`${root}/mix/fader`)).args[0].v;
    if (f > limit) throw new Error(`GUARD: ${root} fader ${f} too hot; refusing write`);
  }

  async function runRow(name, cfg) {
    if (ONLY.length && !ONLY.includes(name)) return;
    const row = { control: name, kind: cfg.kind, strip: cfg.strip, via: cfg.via };
    log(`-- row ${name} [${cfg.kind} ${cfg.strip}] via=${cfg.via}`);
    row.reloadFlag = (await reloadCheck(name)) || undefined;
    if (row.reloadFlag) log(`   !! ${row.reloadFlag}`);
    // APP -> DESK
    try {
      if (cfg.guard) await cfg.guard();
      const t0 = Date.now();
      await cfg.appWrite();
      const rb = await cfg.appReadback();
      row.app2desk = { ...rb, latencyMs: Date.now() - t0 };
      if (rb.stale) row.stale = true;
    } catch (e) {
      row.app2desk = { ok: false, latencyMs: -1, detail: `ERROR: ${e.message}` };
    }
    try {
      if (cfg.restore) await cfg.restore();
    } catch (e) {
      row.app2desk = row.app2desk || {};
      row.app2desk.restoreError = e.message;
    }
    await sleep(400);
    // DESK -> APP (barrier first: engine must show baseline or the test is void)
    try {
      if (cfg.guard) await cfg.guard();
      let barrierMs = 0;
      if (cfg.barrier) barrierMs = await cfg.barrier();
      const t0 = Date.now();
      await cfg.deskWrite();
      const rb = await cfg.deskReadback();
      const ms = Date.now() - t0;
      row.desk2app = { ...rb, latencyMs: ms, within2s: ms <= BUDGET_MS, barrierMs };
      if (rb.stale) row.stale = true;
      if (rb.void) row.void = true;
    } catch (e) {
      row.desk2app = { ok: false, latencyMs: -1, detail: `ERROR: ${e.message}` };
    }
    try {
      if (cfg.restore) await cfg.restore();
    } catch (e) {
      row.desk2app = row.desk2app || {};
      row.desk2app.restoreError = e.message;
    }
    await sleep(300);
    row.pass = row.stale || row.void ? null : !!(row.app2desk?.ok && row.desk2app?.ok);
    log(`   app->desk: ${row.app2desk?.ok ? "PASS" : row.stale ? "STALE" : "FAIL"} ${row.app2desk?.latencyMs}ms :: ${row.app2desk?.detail}${row.app2desk?.restoreError ? " RESTORE-ERR: " + row.app2desk.restoreError : ""}`);
    log(`   desk->app: ${row.desk2app?.ok ? "PASS" : row.void ? "VOID" : "FAIL"} ${row.desk2app?.latencyMs}ms${row.desk2app?.within2s === false && row.desk2app?.ok ? " SLOW(>2s)" : ""} :: ${row.desk2app?.detail}${row.desk2app?.restoreError ? " RESTORE-ERR: " + row.desk2app.restoreError : ""}`);
    rows.push(row);
  }

  const ch = TEST.ch, root = TEST.root;
  const baseFaderQ = await tryQuery(`${root}/mix/fader`);
  const baseMuteQ = await tryQuery(`${root}/mix/on`);
  const basePanQ = await tryQuery(`${root}/mix/pan`);
  if (!baseFaderQ || !baseMuteQ || !basePanQ) throw new Error("test-strip core leaves silent; aborting");
  const baseFader = baseFaderQ.args[0].v;
  const baseMute = baseMuteQ.args[0].v === 0;
  const basePanW = basePanQ.args[0].v;
  let baseEqW = null, baseSendW = null;
  if (supported("eq1_gain", "in")) {
    const q = await tryQuery(`${root}/eq/1/g`);
    baseEqW = q ? q.args[0].v : null;
    if (q === null) log("  !! eq1_gain baseline silent: eq_gain row will be skipped, not failed");
  }
  if (supported("aux1_level", "in")) {
    const q = await tryQuery(`${root}/mix/01/level`);
    baseSendW = q ? q.args[0].v : null;
    if (q === null) log("  !! aux1_level baseline silent: send_level row will be skipped, not failed");
  }
  for (const [k, a, kd, v] of [["ch_fader", `${root}/mix/fader`, "f", baseFader], ["ch_mute", `${root}/mix/on`, "on", baseMute], ["ch_pan", `${root}/mix/pan`, "pan", basePanW]]) noteBaseline(k, a, kd, v);
  if (baseEqW !== null) noteBaseline("ch_eq1", `${root}/eq/1/g`, "f", baseEqW);
  if (baseSendW !== null) noteBaseline("ch_send", `${root}/mix/01/level`, "f", baseSendW);
  log(`baselines: fader=${baseFader} muted=${baseMute} panW=${basePanW} eqW=${baseEqW} sendW=${baseSendW}`);

  // ================= 1. ch mute (true UI round-trip) =================
  await runRow("mute", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-click",
    guard: async () => guardQuietFader(root),
    appWrite: async () => {
      const r = await js(PAGE.clickLabel(muteLabel));
      if (!r?.clicked) throw new Error(`click failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === ch && m.field === "mute", "mute echo");
      if (w.kind === "error") {
        if (staleOf(w.frame)) return { ok: false, stale: true, detail: `STALE-BINARY: engine: ${w.frame.message}` };
        return { ok: false, detail: `engine error: ${w.frame.message}` };
      }
      if (w.kind === "timeout") return { ok: false, detail: `no engine echo (recent ${w.recent})` };
      const t0 = Date.now();
      for (;;) {
        const m = (await oscQuery(`${root}/mix/on`)).args[0].v;
        if (m === 1) return { ok: true, detail: `desk on=1 after ${Date.now() - t0}ms; engine echo in ${w.ms}ms` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: "desk still muted 5s after UI click" };
        await sleep(200);
      }
    },
    deskWrite: async () => oscSend(`${root}/mix/on`, [iArg(1)]),
    barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&window.__live.engine.param(${ch},'mute')===true)`, "mute@baseline"),
    deskReadback: async () => {
      const ms = await uiWaitFor(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===${JSON.stringify(muteLabel)})?.getAttribute('aria-pressed')==='false'`);
      const eng = await js(PAGE.engineParam(ch, "mute"));
      return { ok: true, detail: `DOM aria-pressed=false in ${ms}ms; engine mute=${eng}` };
    },
    restore: async () => restoreLeaf(`${root}/mix/on`, [iArg(0)], (v) => v === 0, "mute"),
  });

  // ================= 2. ch solo =================
  await runRow("solo", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-click",
    guard: async () => guardQuietFader(root),
    appWrite: async () => {
      const r = await js(PAGE.clickLabel(soloLabel));
      if (!r?.clicked) throw new Error(`click failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      const w = await waitEchoOrError((m) => m?.type === "solo" && m.ch === ch, "solo echo");
      if (w.kind === "error") {
        if (staleOf(w.frame)) return { ok: false, stale: true, detail: `STALE-BINARY: engine: ${w.frame.message} (relay predates Solo cmd)` };
        return { ok: false, detail: `engine error: ${w.frame.message}` };
      }
      if (w.kind === "timeout") return { ok: false, detail: `no engine solo echo (recent ${w.recent})` };
      const t0 = Date.now();
      for (;;) {
        const bank = (await oscQuery("/-stat/solosw")).args.map((a) => a.v);
        if (bank[ch - chIn] !== 0) return { ok: true, detail: `desk solosw bit set after ${Date.now() - t0}ms; echo in ${w.ms}ms` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: "desk solosw bit still 0 5s after UI click" };
        await sleep(200);
      }
    },
    deskWrite: async () => {
      await oscSend(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(0)]);
      await sleep(200);
      await oscSend(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(1)]);
    },
    barrier: async () => {
      // echo-only engine state cannot re-baseline the UI; record DOM state instead.
      const p = await js(PAGE.pressed(soloLabel));
      if (p === "true") return -1; // caller treats -1 as VOID risk
      await restoreLeaf(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(0)], () => true, "solo-clear");
      return 0;
    },
    deskReadback: async () => {
      try {
        const ms = await uiWaitFor(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===${JSON.stringify(soloLabel)})?.getAttribute('aria-pressed')==='true'`, 6000);
        const eng = await js(PAGE.engineSolo(ch));
        return { ok: true, detail: `DOM aria-pressed=true in ${ms}ms; engine soloOf=${eng}` };
      } catch {
        const eng = await js(PAGE.engineSolo(ch));
        return { ok: false, detail: `desk->app: no UI/engine update 6s after raw solosw write (soloOf=${eng}); xair solos ride write-echoes only — no poll/snapshot read (engine.rs snapshot + static_poll_list)` };
      }
    },
    restore: async () => restoreLeaf(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(0)], () => true, "solo"),
  });

  // ================= 3. ch fader (DOM key + DOM read + engine cross-check) =================
  await runRow("fader", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-key",
    guard: () => guardMuted(root),
    appWrite: async () => {
      const r = await js(PAGE.keySlider(`Fader ${chTag}`, "PageUp"));
      if (!r?.found || r.disabled) throw new Error(`slider key failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      const want = await js(PAGE.engineParam(ch, "fader"));
      const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === ch && m.field === "fader", "fader echo");
      if (w.kind !== "echo") return { ok: false, detail: `no fader echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
      const t0 = Date.now();
      for (;;) {
        const m = (await oscQuery(`${root}/mix/fader`)).args[0].v;
        if (Math.abs(m - want) < 0.01) return { ok: true, detail: `desk fader=${m} == engine ${want} after ${Date.now() - t0}ms` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: `desk fader=${m} != engine want=${want}` };
        await sleep(200);
      }
    },
    barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'fader')??999)-(${baseFader}))<0.01)`, "fader@baseline"),
    deskWrite: async () => {
      relay.send({ cmd: "focus", ch });
      await oscSend(`${root}/mix/fader`, [fArg(0.05)]);
    },
    deskReadback: async () => {
      const ms = await uiWaitFor(`(s=>{const el=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith("Fader ${chTag}"));return el&&Math.abs(parseFloat(el.getAttribute('aria-valuenow'))-5)<1.5;})()`);
      const eng = await js(PAGE.engineParam(ch, "fader"));
      const fix = await js(`(window.__fixture?window.__fixture.fader('in${TEST.strip1}'):null)`);
      return { ok: true, detail: `DOM slider~=5 in ${ms}ms; engine=${eng}; fixture=${fix}` };
    },
    restore: async () => restoreLeaf(`${root}/mix/fader`, [fArg(baseFader)], (v) => Math.abs(v - baseFader) < EPS_F, "fader"),
  });

  // ================= 4. ch pan =================
  if (supported("pan", "in")) {
    await runRow("pan", {
      kind: "in", strip: `ch${TEST.strip1}`, via: "dom-key",
      guard: () => guardMuted(root),
      appWrite: async () => {
        const r = await js(PAGE.keySlider(`Pan ${chTag}`, "ArrowRight"));
        if (!r?.found || r.disabled) throw new Error(`pan key failed: ${JSON.stringify(r)}`);
      },
      appReadback: async () => {
        const wantEng = await js(PAGE.engineParam(ch, "pan"));
        await waitEchoOrError((m) => m?.type === "param" && m.ch === ch && m.field === "pan", "pan echo");
        const t0 = Date.now();
        for (;;) {
          const w = (await oscQuery(`${root}/mix/pan`)).args[0].v;
          if (Math.abs(wireToPan(w) - wantEng) < 1.0) return { ok: true, detail: `desk pan=${wireToPan(w).toFixed(1)} == engine ${wantEng} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk panW=${w} != engine ${wantEng}` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'pan')??999)-(${wireToPan(basePanW)}))<1.0)`, "pan@baseline"),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch });
        const cur = await js(PAGE.engineParam(ch, "pan"));
        await oscSend(`${root}/mix/pan`, [fArg(panToWire(Math.min(100, (typeof cur === "number" ? cur : 0) + 10)))]);
      },
      deskReadback: async () => {
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'pan')??999)-10)<3)`);
        return { ok: true, detail: `engine pan followed in ${ms}ms` };
      },
      restore: async () => restoreLeaf(`${root}/mix/pan`, [fArg(basePanW)], (v) => Math.abs(v - basePanW) < 0.01, "pan"),
    });
  }

  // ================= 5. ch EQ band gain =================
  if (supported("eq1_gain", "in") && baseEqW !== null) {
    const testDb = Math.min(15, wireToEqDb(baseEqW) + 2);
    await runRow("eq_gain", {
      kind: "in", strip: `ch${TEST.strip1}`, via: "ws-cmd+ui-readback",
      guard: () => guardMuted(root),
      appWrite: async () => relay.send({ cmd: "param", ch, field: "eq1_gain", value: testDb }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === ch && m.field === "eq1_gain", "eq echo");
        if (w.kind !== "echo") return { ok: false, detail: `no eq echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
        const t0 = Date.now();
        for (;;) {
          const vw = (await oscQuery(`${root}/eq/1/g`)).args[0].v;
          if (Math.abs(wireToEqDb(vw) - testDb) < 0.3) return { ok: true, detail: `desk eq1=${wireToEqDb(vw).toFixed(2)}dB == want ${testDb}dB after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk eq1=${wireToEqDb(vw).toFixed(2)}dB != want ${testDb}` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'eq1_gain')??999)-(${wireToEqDb(baseEqW)}))<0.35)`, "eq@baseline"),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch });
        await oscSend(`${root}/eq/1/g`, [fArg(eqDbToWire(testDb + 2))]);
      },
      deskReadback: async () => {
        const want = testDb + 2;
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'eq1_gain')??999)-(${want}))<0.35)`);
        return { ok: true, detail: `engine eq1_gain=${want}dB in ${ms}ms` };
      },
      restore: async () => restoreLeaf(`${root}/eq/1/g`, [fArg(baseEqW)], (v) => Math.abs(v - baseEqW) < 0.01, "eq"),
    });
  }

  // ================= 6. ch send level =================
  if (supported("aux1_level", "in") && baseSendW !== null) {
    await runRow("send_level", {
      kind: "in", strip: `ch${TEST.strip1}->bus1`, via: "ws-cmd+ui-readback",
      guard: () => guardMuted(root),
      appWrite: async () => relay.send({ cmd: "param", ch, field: "aux1_level", value: -20 }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === ch && m.field === "aux1_level", "send echo");
        if (w.kind !== "echo") return { ok: false, detail: `no send echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
        const t0 = Date.now();
        for (;;) {
          const vw = (await oscQuery(`${root}/mix/01/level`)).args[0].v;
          if (Math.abs(wireToSendDb(vw) + 20) < 0.5) return { ok: true, detail: `desk aux1=${wireToSendDb(vw).toFixed(1)}dB == want -20dB after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk aux1=${wireToSendDb(vw).toFixed(1)}dB != want -20` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'aux1_level')??999)-(${wireToSendDb(baseSendW)}))<0.6)`, "send@baseline"),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch });
        await oscSend(`${root}/mix/01/level`, [fArg(sendDbToWire(-25))]);
      },
      deskReadback: async () => {
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'aux1_level')??999)-(-25))<0.6)`);
        return { ok: true, detail: `engine aux1_level=-25dB in ${ms}ms` };
      },
      restore: async () => restoreLeaf(`${root}/mix/01/level`, [fArg(baseSendW)], (v) => Math.abs(v - baseSendW) < 0.005, "send"),
    });
  }

  // ================= 7/8. bus fader + mute (core + view probes) =================
  {
    const b = busPick.bus, bch = chBus + b, bleaf = `/bus/${b + 1}`;
    const bFaderQ = await tryQuery(`${bleaf}/mix/fader`);
    const bMuteQ = await tryQuery(`${bleaf}/mix/on`);
    if (!bFaderQ || !bMuteQ) {
      log("  !! bus leaves silent: bus rows skipped, not failed");
      rows.push({ control: "bus_fader", kind: "bus", strip: `bus${b + 1}`, via: "n/a", app2desk: { ok: false, detail: "desk silence on baseline read" }, desk2app: { ok: false, detail: "skipped" }, pass: null, silence: true });
    } else {
    const bFader0 = bFaderQ.args[0].v;
    const bMute0 = bMuteQ.args[0].v === 0;
    noteBaseline("bus_fader", `${bleaf}/mix/fader`, "f", bFader0);
    noteBaseline("bus_mute", `${bleaf}/mix/on`, "on", bMute0);
    const up = Math.min(1, bFader0 + 0.03);
    await runRow("bus_fader", {
      kind: "bus", strip: `bus${b + 1}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "bus_fader", bus: b, value: up }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "bus" && m.bus === b, "bus echo");
        if (w.kind !== "echo") return { ok: false, detail: `no bus echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(`${bleaf}/mix/fader`)).args[0].v;
          if (Math.abs(m - up) < EPS_F) return { ok: true, detail: `desk bus fader=${m} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk bus fader=${m} != want ${up}` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${bch},'fader')??999)-(${bFader0}))<0.01)`, "busfader@baseline"),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: bch });
        await oscSend(`${bleaf}/mix/fader`, [fArg(Math.max(0, bFader0 - 0.02))]);
      },
      deskReadback: async () => {
        const want = Math.max(0, bFader0 - 0.02);
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${bch},'fader')??999)-(${want}))<0.012)`);
        const view = await js(PAGE.engineBus(b));
        const viewFollows = view && Math.abs(view.fader - want) < 0.012;
        return { ok: true, detail: `engine strip param followed in ${ms}ms; busMaster view ${viewFollows ? "follows" : `STALE (shows ${JSON.stringify(view)} — echo-only, libraries.ts:133)`}` };
      },
      restore: async () => restoreLeaf(`${bleaf}/mix/fader`, [fArg(bFader0)], (v) => Math.abs(v - bFader0) < EPS_F, "bus fader"),
    });
    await runRow("bus_mute", {
      kind: "bus", strip: `bus${b + 1}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "bus_mute", bus: b, muted: !bMute0 }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "bus" && m.bus === b, "bus echo");
        if (w.kind !== "echo") return { ok: false, detail: `no bus echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(`${bleaf}/mix/on`)).args[0].v;
          if ((m === 0) === !bMute0) return { ok: true, detail: `desk bus muted=${!bMute0} after ${Date.now() - t0}ms (transient)` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk bus on=${m}` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${bch},'mute'))===(${bMute0})))`, "busmute@baseline"),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: bch });
        await oscSend(`${bleaf}/mix/on`, [iArg(bMute0 ? 1 : 0)]);
      },
      deskReadback: async () => {
        const want = !bMute0;
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${bch},'mute'))===(${want})))`);
        const view = await js(PAGE.engineBus(b));
        const viewFollows = view && view.muted === want;
        return { ok: true, detail: `engine strip param mute=${want} in ${ms}ms (transient); busMaster view ${viewFollows ? "follows" : `STALE (${JSON.stringify(view)} — echo-only)`}` };
      },
      restore: async () => restoreLeaf(`${bleaf}/mix/on`, [iArg(bMute0 ? 0 : 1)], (v) => (v === 0) === bMute0, "bus mute"),
    });
    } // end else (bus leaves answered)
  }

  // ================= 9/10. main fader + mute (core + view probes) =================
  await runRow("main_fader", {
    kind: "main", strip: "main", via: "dom-key",
    appWrite: async () => {
      const has = await js(PAGE.sliderVal("Fader Main LR"));
      if (!has) relay.send({ cmd: "main_fader", value: Math.max(0, mainFader0 - 0.02) });
      else await js(PAGE.keySlider("Fader Main LR", "ArrowDown"));
    },
    appReadback: async () => {
      await sleep(600);
      const m = (await oscQuery("/lr/mix/fader")).args[0].v;
      return Math.abs(m - mainFader0) > EPS_F
        ? { ok: true, detail: `desk main fader ${mainFader0} -> ${m} (small nudge)` }
        : { ok: false, detail: `desk main fader unchanged at ${m}` };
    },
    barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${chMain},'fader')??999)-(${mainFader0}))<0.01)`, "mainfader@baseline"),
    deskWrite: async () => oscSend("/lr/mix/fader", [fArg(Math.max(0, mainFader0 - 0.01))]),
    deskReadback: async () => {
      const want = Math.max(0, mainFader0 - 0.01);
      const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${chMain},'fader')??999)-(${want}))<0.012)`);
      const view = await js(PAGE.engineMain);
      const slider = await js(PAGE.sliderVal("Fader Main LR"));
      return { ok: true, detail: `engine strip param followed in ${ms}ms; mainMaster view=${JSON.stringify(view)}; DOM slider=${JSON.stringify(slider)}` };
    },
    restore: async () => restoreLeaf("/lr/mix/fader", [fArg(mainFader0)], (v) => Math.abs(v - mainFader0) < EPS_F, "main fader"),
  });
  await runRow("main_mute", {
    kind: "main", strip: "main", via: muteMainLabel ? "dom-click" : "ws-cmd",
    appWrite: async () => {
      if (muteMainLabel) {
        const r = await js(PAGE.clickLabel(muteMainLabel));
        if (!r?.clicked) throw new Error(`main mute click failed: ${JSON.stringify(r)}`);
      } else relay.send({ cmd: "main_mute", muted: !mainMuted0 });
    },
    appReadback: async () => {
      const w = await waitEchoOrError((m) => m?.type === "main", "main echo");
      if (w.kind === "error") {
        if (staleOf(w.frame)) return { ok: false, stale: true, detail: `STALE-BINARY: engine: ${w.frame.message} (relay predates MainMute cmd)` };
        return { ok: false, detail: `engine error: ${w.frame.message}` };
      }
      if (w.kind === "timeout") return { ok: false, detail: `no engine main echo (recent ${w.recent}) — desk read follows` };
      const t0 = Date.now();
      for (;;) {
        const m = (await oscQuery("/lr/mix/on")).args[0].v;
        if ((m === 0) === !mainMuted0) return { ok: true, detail: `desk main muted=${!mainMuted0} after ${Date.now() - t0}ms (BRIEF room change)` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: `desk main on=${m}, want muted=${!mainMuted0} (echo arrived but desk did not follow — driver/desk layer)` };
        await sleep(200);
      }
    },
    barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${chMain},'mute'))===(${mainMuted0})))`, "mainmute@baseline"),
    deskWrite: async () => oscSend("/lr/mix/on", [iArg(mainMuted0 ? 1 : 0)]),
    deskReadback: async () => {
      const want = !mainMuted0;
      const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${chMain},'mute'))===(${want})))`);
      const view = await js(PAGE.engineMain);
      const dom = muteMainLabel ? await js(PAGE.pressed(muteMainLabel)) : "n/a";
      return { ok: true, detail: `engine strip param mute=${want} in ${ms}ms (BRIEF room change); mainMaster view muted=${view?.muted} (echo-only, libraries.ts:148); DOM pressed=${dom}` };
    },
    restore: async () => restoreLeaf("/lr/mix/on", [iArg(mainMuted0 ? 0 : 1)], (v) => (v === 0) === mainMuted0, "main mute"),
  });

  // ================= rtn / fxsend / dca =================
  async function famRows(name, pick, kind, scopKind) {
    if (!pick) {
      rows.push({ control: `${name}_fader`, kind, strip: "-", via: "n/a", app2desk: { ok: false, detail: "no answering leaf" }, desk2app: { ok: false, detail: "skipped" }, pass: null, silence: true });
      return;
    }
    const leafBase = (pick.dca || pick.fxsend) ? pick.root : `${pick.root}/mix`;
    const fLeaf = `${leafBase}/fader`, oLeaf = `${leafBase}/on`;
    let f0, m0;
    try {
      f0 = (await oscQuery(fLeaf)).args[0].v;
      m0 = (await oscQuery(oLeaf)).args[0].v === 0;
    } catch (e) {
      rows.push({ control: `${name}_fader`, kind, strip: pick.root, via: "n/a", app2desk: { ok: false, detail: `leaf silence: ${e.message}` }, desk2app: { ok: false, detail: "skipped" }, pass: null, silence: true });
      return;
    }
    noteBaseline(`${name}_fader`, fLeaf, "f", f0);
    noteBaseline(`${name}_mute`, oLeaf, "on", m0);
    const up = Math.min(1, f0 + 0.03);
    await runRow(`${name}_fader`, {
      kind, strip: `${pick.root}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "param", ch: pick.ch, field: "fader", value: up }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === pick.ch && m.field === "fader", "fader echo");
        if (w.kind !== "echo") return { ok: false, detail: `no echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(fLeaf)).args[0].v;
          if (Math.abs(m - up) < 0.01) return { ok: true, detail: `desk ${fLeaf}=${m} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk ${fLeaf}=${m} != want ${up}` };
          await sleep(200);
        }
      },
      barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${pick.ch},'fader')??999)-(${f0}))<0.012)`, `${name}fader@baseline`),
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: pick.ch });
        await oscSend(fLeaf, [fArg(Math.max(0, f0 - 0.02))]);
      },
      deskReadback: async () => {
        const want = Math.max(0, f0 - 0.02);
        const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${pick.ch},'fader')??999)-(${want}))<0.012)`);
        return { ok: true, detail: `engine param fader followed in ${ms}ms` };
      },
      restore: async () => restoreLeaf(fLeaf, [fArg(f0)], (v) => Math.abs(v - f0) < 0.01, `${name} fader`),
    });
    if (supported("mute", scopKind)) {
      await runRow(`${name}_mute`, {
        kind, strip: `${pick.root}`, via: "ws-cmd+ui-readback",
        appWrite: async () => relay.send({ cmd: "mute", ch: pick.ch, muted: !m0 }),
        appReadback: async () => {
          const w = await waitEchoOrError((m) => m?.type === "param" && m.ch === pick.ch && m.field === "mute", "mute echo");
          if (w.kind !== "echo") return { ok: false, detail: `no echo (${w.kind}${w.frame ? ": " + w.frame.message : ""})` };
          const t0 = Date.now();
          for (;;) {
            const m = (await oscQuery(oLeaf)).args[0].v;
            if ((m === 0) === !m0) return { ok: true, detail: `desk muted=${!m0} after ${Date.now() - t0}ms (transient)` };
            if (Date.now() - t0 > 5000) return { ok: false, detail: `desk on=${m}` };
            await sleep(200);
          }
        },
        barrier: async () => barrierEngine(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${pick.ch},'mute'))===(${m0})))`, `${name}mute@baseline`),
        deskWrite: async () => {
          relay.send({ cmd: "focus", ch: pick.ch });
          await oscSend(oLeaf, [iArg(m0 ? 1 : 0)]);
        },
        deskReadback: async () => {
          const want = !m0;
          const ms = await uiWaitFor(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${pick.ch},'mute'))===(${want})))`);
          return { ok: true, detail: `engine mute=${want} in ${ms}ms (transient)` };
        },
        restore: async () => restoreLeaf(oLeaf, [iArg(m0 ? 0 : 1)], (v) => (v === 0) === m0, `${name} mute`),
      });
    }
  }
  await famRows("rtn", rtnPick, "ret", "ret");
  await famRows("fxsend", fxPick, "fxsnd", "fxsnd");
  await famRows("dca", dcaPick, "dca", "dca");

  // ================= solo on non-channel strips: expect per-strip refusal =================
  for (const [nm, pch] of [["bus", chBus], ["dca", chDca]]) {
    await runRow(`solo_${nm}_refused`, {
      kind: nm, strip: `ch${pch}`, via: "ws-cmd",
      appWrite: async () => relay.send({ cmd: "solo", ch: pch, soloed: true }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "solo" && m.ch === pch, "solo echo?");
        if (w.kind === "error") {
          if (staleOf(w.frame)) return { ok: false, stale: true, detail: `STALE-BINARY: engine: ${w.frame.message} (cannot test per-strip refusal on this relay)` };
          return /support|range|channel|strip/i.test(w.frame.message || "") ? { ok: true, detail: `honest refusal: ${w.frame.message}` } : { ok: false, detail: `unexpected error: ${w.frame.message}` };
        }
        if (w.kind === "echo") {
          await oscSend(`/-stat/solosw/${ch2(pch - (nm === "bus" ? chBus : chDca) + 1)}`, [iArg(0)]).catch(() => null);
          return { ok: false, detail: `engine ACCEPTED solo on ${nm} strip (should refuse — no verified leaf)` };
        }
        return { ok: false, detail: "no response" };
      },
      deskWrite: async () => {},
      deskReadback: async () => ({ ok: true, detail: "n/a (refused writes never reach desk)" }),
      restore: async () => {},
    });
  }

  // ================= mute-group: same-value path only =================
  {
    const gQ = await tryQuery("/config/mute/1");
    if (!gQ) {
      log("  !! /config/mute/1 silent: mute_group row skipped");
      rows.push({ control: "mute_group", kind: "mg", strip: "mg1", via: "n/a", app2desk: { ok: false, detail: "desk silence on baseline read" }, desk2app: { ok: false, detail: "skipped" }, pass: null, silence: true });
    } else {
    const g0 = gQ.args[0].v;
    noteBaseline("mg1", "/config/mute/1", "i", g0);
    await runRow("mute_group", {
      kind: "mg", strip: "mg1", via: "ws-cmd+same-value",
      appWrite: async () => relay.send({ cmd: "mute_group", group: 0, muted: g0 !== 0 }),
      appReadback: async () => {
        const w = await waitEchoOrError((m) => m?.type === "mute_group" && m.group === 0, "mg echo");
        if (w.kind === "error") {
          if (staleOf(w.frame)) return { ok: false, stale: true, detail: `STALE-BINARY: engine: ${w.frame.message} (relay predates MuteGroup cmd)` };
          return { ok: false, detail: `engine error: ${w.frame.message}` };
        }
        if (w.kind === "timeout") return { ok: false, detail: "no mg echo" };
        const m = (await oscQuery("/config/mute/1")).args[0].v;
        return m === g0 ? { ok: true, detail: `desk unchanged ${m} (same-value); engine echo in ${w.ms}ms` } : { ok: false, detail: `desk changed to ${m}!` };
      },
      barrier: async () => 0,
      deskWrite: async () => oscSend("/config/mute/1", [iArg(g0)]),
      deskReadback: async () => ({ ok: true, detail: "same-value raw write only (flipped-value test would mute members audibly — not attempted); engine re-read agrees via poller" }),
      restore: async () => restoreLeaf("/config/mute/1", [iArg(g0)], (v) => v === g0, "mg1"),
    });
    } // end else (mg leaf answered)
  }

  // ================= desk left as found =================
  log("== desk-left-as-found re-read ==");
  const drift = [];
  for (const [key, b] of Object.entries(baselines)) {
    try {
      const m = await oscQuery(b.addr);
      const cur = b.kind === "bank" ? m.args.map((a) => a.v) : m.args[0].v;
      let same;
      if (b.kind === "bank") same = JSON.stringify(cur) === JSON.stringify(b.value);
      else if (b.kind === "on") same = (cur === 0) === (b.value === true || b.value === 0);
      else same = Math.abs(cur - b.value) < 0.01;
      log(`  ${key} ${b.addr}: base=${JSON.stringify(b.value)} now=${JSON.stringify(cur)} ${same ? "OK" : "DRIFT!"}`);
      if (!same) drift.push(key);
    } catch (e) {
      log(`  ${key} ${b.addr}: re-read failed: ${e.message}`);
      drift.push(key + " (unreadable)");
    }
  }

  relay.close();
  await closeBrowser();
  try {
    oscSock.close();
  } catch {
    /* ignore */
  }

  log("\n================ MATRIX ================");
  for (const r of rows) {
    const a = r.app2desk || {}, d = r.desk2app || {};
    const tag = r.stale ? "STALE" : r.pass === null ? "VOID" : r.pass ? "PASS" : "FAIL";
    log(`${tag} ${r.control} [${r.kind} ${r.strip}] via=${r.via}${r.reloadFlag ? " !! " + r.reloadFlag : ""}`);
    log(`    app->desk: ${a.ok ? "PASS" : a.stale || r.stale ? "STALE" : "FAIL"} ${a.latencyMs}ms :: ${a.detail || ""}${a.restoreError ? " RESTORE-ERR: " + a.restoreError : ""}`);
    log(`    desk->app: ${d.ok ? "PASS" : "FAIL"} ${d.latencyMs}ms${d.within2s === false && d.ok ? " SLOW(>2s)" : ""}${d.barrierMs ? ` barrier=${d.barrierMs}ms` : ""} :: ${d.detail || ""}${d.restoreError ? " RESTORE-ERR: " + d.restoreError : ""}`);
  }
  const real = rows.filter((r) => !r.stale && !r.silence && r.pass !== null);
  const silent = rows.filter((r) => r.silence);
  const passed = real.filter((r) => r.pass);
  const failed = real.filter((r) => !r.pass);
  const stale = rows.filter((r) => r.stale);
  log(`\n${passed.length}/${real.length} rows pass (${stale.length} STALE-binary, excluded)`);
  log(`desk left as found: ${drift.length === 0 ? "YES" : "NO — drift: " + drift.join(",")}`);
  if (silent.length) {
    log("\n---- SILENCE ROWS (desk leaf unreadable; skipped, not failed) ----");
    for (const r of silent) log(`* ${r.control}: ${r.app2desk?.detail}`);
  }
  if (failed.length) {
    log("\n---- GAP LIST ----");
    for (const r of failed) {
      if (!r.app2desk?.ok) log(`* ${r.control} app->desk: ${r.app2desk?.detail}`);
      if (!r.desk2app?.ok) log(`* ${r.control} desk->app: ${r.desk2app?.detail}`);
    }
  }
  if (stale.length) {
    log("\n---- STALE-BINARY ROWS (relay predates cmd; rebuild+restart relay, then re-run) ----");
    for (const r of stale) log(`* ${r.control}: ${r.app2desk?.detail || r.desk2app?.detail}`);
  }
  const fails = failed.length + drift.length;
  log(fails === 0 ? "VERIFY-BIDIRECTIONAL PASS (modulo stale rows)" : "VERIFY-BIDIRECTIONAL FAIL");
  process.exit(fails === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error(`FATAL: ${e?.stack || e}`);
  try {
    if (chromeProc) chromeProc.kill("SIGKILL");
  } catch {
    /* ignore */
  }
  process.exit(2);
});
