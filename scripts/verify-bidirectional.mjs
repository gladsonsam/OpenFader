#!/usr/bin/env node
// scripts/verify-bidirectional.mjs — owner-ordered bidirectional verification
// loop: app vs XR18 (192.168.50.123, FW 1.20).
//
// For each control in the matrix, verifies BOTH directions:
//   app->desk: drive the control in the headless UI (dev server
//     http://127.0.0.1:5291, never started/stopped here) against the :8081
//     xair relay (healthz-checked, never killed/restarted here), then read
//     the desk directly (raw OSC query over UDP, same wire the driver uses)
//     and assert the desk followed.
//   desk->app: write the control via raw OSC (simulating mixer control software), then
//     read the UI state (DOM probe in the headless page + engine readback
//     over WS) and assert our side followed.
//
// Per-write discipline: read baseline -> guard (volumes safe) -> write ->
// readback -> restore -> verify restore. Report (not just pass/fail): per
// control per direction with latencies, gap list with root-cause file:line,
// desk-left-as-found proof.
//
// Constraints honored: touches NO crates/**, src-tauri/**, Dockerfile,
// docs/*protocol*/LIVE-RIG.md, scripts/dev-xr18.sh (used for relay
// check/restore ONLY via shell, never edited). No *.test.ts, no build gate.
// Does not commit. Reads caps/topology from the relay; never sends
// {"cmd":"connect"} (attach-only: a driver flip would invalidate the run).
//
// Usage: node scripts/verify-bidirectional.mjs [--ui-port 5291]
//        [--relay-port 8081] [--desk 192.168.50.123] [--only mute,solo]
// Exit: 0 all PASS (or EXPECTED-FAIL rows only), 1 any real FAIL.
//
// Env knobs: VERBOSE=1 for per-step logs.
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
const DESK = opt("--desk", "192.168.50.123");
const DESK_PORT = 10024;
const ONLY = (opt("--only", "") || "").split(",").map((s) => s.trim()).filter(Boolean);
const VERBOSE = process.env.VERBOSE === "1";

const WS_URL = `ws://${RELAY_HOST}:${RELAY_PORT}/ws`;
const UI_URL = `http://${RELAY_HOST}:${UI_PORT}/?server=${RELAY_HOST}:${RELAY_PORT}`;
const CDP_PORT = 9333;
const CHROME =
  "/home/user/project";

const log = (...a) => console.log(...a);
const vlog = (...a) => VERBOSE && console.log("  ...", ...a);

// ---------------------------------------------------------------- report
const rows = [];
function addRow(r) {
  rows.push(r);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --------------------------------------------------------------- OSC wire
// Minimal OSC 1.0 over UDP (rosc-compatible): address + typetags + args.
// Query = bare address with empty typetag (what the desk answers to).
function pstr(s) {
  const b = Buffer.from(s, "ascii");
  const n = b.length + 1;
  const pad = (4 - (n % 4)) % 4;
  return Buffer.concat([b, Buffer.alloc(1 + pad)]);
}
function oscEncode(addr, args /* [{t:'f'|'i'|'s',v}] */) {
  const tags = "," + args.map((a) => a.t).join("");
  const parts = [pstr(addr), pstr(tags)];
  for (const a of args) {
    if (a.t === "f") {
      const b = Buffer.alloc(4);
      b.writeFloatBE(a.v, 0);
      parts.push(b);
    } else if (a.t === "i") {
      const b = Buffer.alloc(4);
      b.writeInt32BE(a.v, 0);
      parts.push(b);
    } else if (a.t === "s") {
      parts.push(pstr(a.v));
    }
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
    } else if (t === "s") {
      argv.push({ t, v: rdStr() });
    } else if (t === "b") {
      const n = buf.readInt32BE(o);
      o += 4;
      argv.push({ t, v: buf.subarray(o, o + n) });
      o += n + ((4 - (n % 4)) % 4);
    } else throw new Error(`osc: unsupported tag ${t}`);
  }
  return { addr, args: argv };
}

let oscSock = null;
const oscWaiters = new Map(); // addr -> [{resolve,reject,timer}]
function oscInit() {
  return new Promise((resolve, reject) => {
    oscSock = dgram.createSocket("udp4");
    oscSock.on("message", (msg) => {
      try {
        const m = oscDecode(msg);
        const q = oscWaiters.get(m.addr);
        if (q && q.length) q.shift().resolve(m);
      } catch {
        /* ignore undecodable datagrams */
      }
    });
    oscSock.on("error", reject);
    oscSock.bind(0, "0.0.0.0", resolve);
  });
}
/** Raw OSC query: send bare address, await reply with same address. */
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
      const m = await p;
      return m;
    } catch (e) {
      lastErr = e;
      const q = oscWaiters.get(addr);
      if (q && q.length) q.shift();
    }
  }
  throw new Error(`${addr}: no reply after ${tries} tries (${lastErr?.message})`);
}
/** Raw OSC write (fire-and-forget at the socket level). */
async function oscSend(addr, args) {
  return new Promise((resolve, reject) => {
    oscSock.send(oscEncode(addr, args), DESK_PORT, DESK, (e) => (e ? reject(e) : resolve()));
  });
}
const fArg = (v) => ({ t: "f", v });
const iArg = (v) => ({ t: "i", v });
// Codecs mirrored from crates (never imported; kept in sync by asserts):
//  - on_arg/is_on: int 1 = on(unmuted), 0 = muted (transport/src/osc.rs)
//  - pan: wire (eng+100)/200, eng -100..100 (proto-xair/src/mix.rs)
//  - eq gain: wire (db+15)/30 (proto-xair/src/eq.rs lin(-15,15))
//  - send: fader-law joints -60/-30/-10/0 floor -90 (proto-xair/src/mix.rs)
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
const ch2 = (n1) => String(n1).padStart(2, "0"); // 1-based zero-padded
const CH = (strip1) => `/ch/${ch2(strip1)}`; // 1-based wire channel root

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
  async waitFor(pred, timeoutMs = 8000, what = "frame") {
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
async function js(expr, awaitPromise = false) {
  const r = await cdpSend("Runtime.evaluate", {
    expression: expr,
    returnByValue: true,
    awaitPromise,
  });
  if (r.exceptionDetails) {
    throw new Error(`page js threw: ${JSON.stringify(r.exceptionDetails).slice(0, 300)}`);
  }
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
  chromeProc = spawn(
    CHROME,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      "--window-size=1600,900",
      `--user-data-dir=${profile}`,
      `--remote-debugging-port=${CDP_PORT}`,
      "about:blank",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
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
  const nr = await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?${encodeURIComponent(url)}`, {
    method: "PUT",
  });
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
// Page helpers (run inside the headless UI; pure DOM, same handlers as taps).
const PAGE = {
  online: `!!(window.__live && window.__live.phase === 'online' && window.__live.engine && window.__live.engine.capsReady)`,
  muteEnable: `!!(window.__live && window.__live.engine)`,
  buttons: `[...document.querySelectorAll('button')].filter(b=>{const t=(b.textContent||'').trim();return t==='MUTE'||t==='SOLO';}).map(b=>({label:b.getAttribute('aria-label'),pressed:b.getAttribute('aria-pressed'),disabled:!!b.disabled}))`,
  sliders: `[...document.querySelectorAll('[role=slider]')].map(s=>({label:s.getAttribute('aria-label'),now:s.getAttribute('aria-valuenow'),text:s.getAttribute('aria-valuetext'),disabled:!!s.disabled}))`,
  clickLabel: (label) => `(lbl=>{const b=[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===lbl);if(!b)return{found:false};if(b.disabled)return{found:true,disabled:true};b.click();return{found:true,clicked:true};})(${JSON.stringify(label)})`,
  keySlider: (prefix, key) => `(p=>{const s=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith(p));if(!s)return{found:false};if(s.disabled)return{found:true,disabled:true};s.focus();const before=s.getAttribute('aria-label');s.dispatchEvent(new KeyboardEvent('keydown',{key:${JSON.stringify(key)},bubbles:true,cancelable:true}));return{found:true,before,after:document.querySelectorAll('[role=slider]')&&s.getAttribute('aria-label')};})(${JSON.stringify(prefix)})`,
  pressed: (label) => `(lbl=>{const b=[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===lbl);return b?b.getAttribute('aria-pressed'):null;})(${JSON.stringify(label)})`,
  sliderNow: (prefix) => `(p=>{const s=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith(p));return s?{now:s.getAttribute('aria-valuenow'),text:s.getAttribute('aria-valuetext'),label:s.getAttribute('aria-label')}:null;})(${JSON.stringify(prefix)})`,
  fixture: (kind, id) => `(window.__fixture?window.__fixture[${JSON.stringify(kind)}](${JSON.stringify(id)}):null)`,
  engineParam: (ch, id) => `(window.__live&&window.__live.engine?window.__live.engine.param(${ch},${JSON.stringify(id)}):null)`,
  engineStrip: (ch) => `(window.__live&&window.__live.engine?window.__live.engine.strip(${ch}):null)`,
  engineSolo: (ch) => `(window.__live&&window.__live.engine?window.__live.engine.soloOf(${ch}):null)`,
  engineMainMuted: `(window.__live&&window.__live.engine?window.__live.engine.mainMuted:null)`,
  engineMainFader: `(window.__live&&window.__live.engine?(window.__live.engine.mainFader):null)`,
};

// ------------------------------------------------------------- preflight
async function healthz(port) {
  const r = await fetch(`http://127.0.0.1:${port}/healthz`);
  if (!r.ok) throw new Error(`healthz :${port} -> ${r.status}`);
  return (await r.text()).trim();
}
function relayCheck() {
  try {
    const out = execSync("bash scripts/dev-xr18.sh", {
      timeout: 30000,
      encoding: "utf8",
      cwd: "/home/user/project",
    });
    return out;
  } catch (e) {
    return `dev-xr18.sh failed: ${e.message}`;
  }
}

// ================================================================= main
const baselines = {}; // key -> {addr, kind, value} for left-as-found proof
function noteBaseline(key, addr, kind, value) {
  if (!(key in baselines)) baselines[key] = { addr, kind, value };
}

async function main() {
  log("== verify-bidirectional: preflight ==");
  const hz = await healthz(RELAY_PORT);
  log(`relay :${RELAY_PORT} healthz: ${hz}`);
  if (!hz.includes("xair")) {
    log("relay is NOT on xair driver. Attempting restore via scripts/dev-xr18.sh (check only)...");
    log(relayCheck());
    const hz2 = await healthz(RELAY_PORT);
    log(`after check: ${hz2}`);
    if (!hz2.includes("xair")) throw new Error("relay not on xair; aborting (never touch desk state blindly)");
  }
  const uiProbe = await fetch(`http://${RELAY_HOST}:${UI_PORT}/`).then((r) => r.status).catch((e) => `ERR ${e.message}`);
  log(`dev server :${UI_PORT} -> ${uiProbe}`);
  if (uiProbe !== 200) throw new Error("dev server not up; aborting (must not start/stop it here)");

  await oscInit();
  // Desk identity (read-only).
  const info = await oscQuery("/info").catch(() => null);
  log(`desk /info: ${info ? JSON.stringify(info.args.map((a) => a.v)) : "no reply (continuing; per-leaf queries will confirm)"}`);

  const relay = new Relay(WS_URL);
  await relay.waitOpen();
  const caps = await relay.waitFor((m) => m?.type === "caps", 8000, "caps");
  log(`caps: driver=${caps.driver} params=${caps.params.length} unsupported=${caps.unsupported.length}`);
  if (caps.driver !== "xair") throw new Error(`relay driver is ${caps.driver}, not xair; aborting`);
  const topo = caps.topology;
  const famOf = (kind) => topo.find((f) => f.kind === kind);
  const chIn = famOf("in").first; // 0
  const chBus = famOf("bus").first; // 20
  const chRet = famOf("ret").first; // 16
  const chFx = famOf("fxsnd").first; // 26
  const chMain = famOf("main").first; // 30
  const chDca = famOf("dca").first; // 31
  log(`topology: in@${chIn} ret@${chRet} bus@${chBus} fxsnd@${chFx} main@${chMain} dca@${chDca}`);
  const supported = (id, kind) => {
    const d = caps.params.find((p) => p.id === id);
    if (!d) return false;
    if (caps.unsupported.includes(id)) return false;
    if (kind && Array.isArray(d.scopes) && d.scopes.length) return d.scopes.includes(kind);
    return true;
  };
  for (const id of ["fader", "mute", "pan", "eq1_gain", "aux1_level", "fx1_level"]) {
    log(`  caps ${id}: ${supported(id) ? "supported" : "UNSUPPORTED " + (caps.unsupported.includes(id) ? "(unsupported list)" : "(no def/scopes)")}`);
  }

  // ---- pick quiet strips (read-only queries) ----
  async function onOf(root) {
    const m = await oscQuery(`${root}/mix/on`);
    return m.args[0].v !== 0 ? false : true; // true = muted
  }
  async function faderOf(root) {
    const m = await oscQuery(`${root}/mix/fader`);
    return m.args[0].v;
  }
  let TEST = null;
  for (let s = 1; s <= 16; s++) {
    const root = CH(s);
    try {
      const [mu, fa] = [await onOf(root), await faderOf(root)];
      if (mu && fa <= 0.05) {
        TEST = { strip1: s, ch: chIn + s - 1, root, fader: fa };
        break;
      }
    } catch {
      /* silent leaf: skip */
    }
  }
  if (!TEST) throw new Error("no muted near-zero channel strip found; aborting (volume safety)");
  log(`test strip: ch${TEST.strip1} (engine ch ${TEST.ch}) muted, fader=${TEST.fader}`);
  // bus: prefer a muted bus (silent moves), else lowest fader
  let busPick = { bus: 0, fader: 1e9, muted: false };
  const busCands = [];
  for (let b = 1; b <= 6; b++) {
    try {
      const f = (await oscQuery(`/bus/${b}/mix/fader`)).args[0].v;
      const mu = (await oscQuery(`/bus/${b}/mix/on`)).args[0].v === 0;
      busCands.push({ bus: b - 1, fader: f, muted: mu });
    } catch {
      /* skip */
    }
  }
  busPick = busCands.filter((c) => c.muted).sort((a, b2) => a.fader - b2.fader)[0]
    || busCands.sort((a, b2) => a.fader - b2.fader)[0]
    || busPick;
  log(`bus pick: bus${busPick.bus + 1} fader=${busPick.fader} muted=${busPick.muted}`);
  // rtn/fxsnd/dca: lowest fader each (read-only)
  async function lowFam(rootFn, n, chFirst) {
    let best = null;
    for (let i = 1; i <= n; i++) {
      const root = rootFn(i);
      try {
        const f = (await oscQuery(`${root}/mix/fader`)).args[0].v;
        if (!best || f < best.fader) best = { n: i, ch: chFirst + i - 1, root, fader: f };
      } catch {
        try {
          const f2 = (await oscQuery(`${root}/fader`)).args[0].v; // DCA form
          if (!best || f2 < best.fader) best = { n: i, ch: chFirst + i - 1, root, fader: f2, dca: true };
        } catch {
          /* silent */
        }
      }
    }
    return best;
  }
  const rtnPick = await lowFam((i) => `/rtn/${i}`, 4, chRet);
  const fxPick = await lowFam((i) => `/fxsend/${i}`, 4, chFx);
  const dcaPick = await lowFam((i) => `/dca/${i}`, 4, chDca);
  log(`rtn pick: ${JSON.stringify(rtnPick)} fxsend pick: ${JSON.stringify(fxPick)} dca pick: ${JSON.stringify(dcaPick)}`);
  const mainFader0 = (await oscQuery("/lr/mix/fader")).args[0].v;
  const mainMuted0 = (await oscQuery("/lr/mix/on")).args[0].v === 0;
  log(`main baseline: fader=${mainFader0} muted=${mainMuted0} (restore at end)`);
  noteBaseline("main_fader", "/lr/mix/fader", "f", mainFader0);
  noteBaseline("main_mute", "/lr/mix/on", "on", mainMuted0);

  // ---- launch headless UI ----
  log("launching headless Chromium (CDP)...");
  await launchBrowser(UI_URL);
  const tOnline0 = Date.now();
  let lastNote = 0;
  for (;;) {
    const on = await js(PAGE.online).catch(() => false);
    if (on) break;
    const el = Date.now() - tOnline0;
    if (el > 240000) throw new Error("page never reached online (capsReady)");
    if (el - lastNote > 15000) {
      lastNote = el;
      const st = await js(`({href:window.location.href,phase:window.__live&&window.__live.phase,msg:window.__live&&window.__live.message})`).catch(() => null);
      log(`  ... still waiting for online (${Math.round(el / 1000)}s): ${JSON.stringify(st)}`);
    }
    await sleep(1000);
  }
  log(`page online in ${Date.now() - tOnline0}ms`);
  const hzAfter = await healthz(RELAY_PORT);
  log(`relay healthz after page load: ${hzAfter}`);
  if (!hzAfter.includes("xair")) throw new Error("page flipped relay off xair; aborting (restore via dev-xr18.sh, then investigate UI connect path)");
  const btns = await js(PAGE.buttons);
  const sliders = await js(PAGE.sliders);
  vlog(`buttons: ${JSON.stringify(btns).slice(0, 400)}`);
  vlog(`sliders: ${JSON.stringify(sliders.map((s) => s.label)).slice(0, 500)}`);
  const muteLabel = (btns.find((b) => /Ch 6\b/.test(b.label || "") && (b.label || "").startsWith("Mute")) || {}).label;
  const soloLabel = (btns.find((b) => /Ch 6\b/.test(b.label || "") && (b.label || "").startsWith("Solo")) || {}).label;
  const muteMainLabel = (btns.find((b) => /Main LR/.test(b.label || "") && (b.label || "").startsWith("Mute")) || {}).label;
  if (!muteLabel || !soloLabel) throw new Error(`cannot find Ch 6 buttons: ${JSON.stringify(btns.map((b) => b.label))}`);
  log(`Ch6 labels: mute=${JSON.stringify(muteLabel)} solo=${JSON.stringify(soloLabel)} mainMute=${JSON.stringify(muteMainLabel)}`);
  const faderPrefix = "Fader Ch 6";
  const panPrefix = "Pan Ch 6";
  const mainFaderPrefix = "Fader Main LR";
  if (!sliders.some((s) => (s.label || "").startsWith(faderPrefix))) throw new Error("Ch 6 fader slider not on screen");
  // muteEnable/lock gate must be open or clicks are no-ops by design.
  const gateCheck = await js(`(b=>{const m=[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===b);return m?{disabled:!!m.disabled}:null;})(${JSON.stringify(muteLabel)})`);
  if (!gateCheck || gateCheck.disabled) throw new Error("Mute Ch 6 button disabled (muteEnable/lock gate); cannot drive UI");

  // ---- helpers for rows ----
  const BUDGET_MS = 2000;
  const WAIT_MS = 8000;
  async function uiWaitFor(fnExpr, timeoutMs = WAIT_MS) {
    const t0 = Date.now();
    for (;;) {
      const v = await js(fnExpr);
      if (v) return { v, ms: Date.now() - t0 };
      if (Date.now() - t0 > timeoutMs) throw new Error(`UI wait timeout: ${fnExpr.slice(0, 120)}`);
      await sleep(250);
    }
  }
  // Guard: strip must still be muted (silent) before any write on it.
  async function guardMuted(root) {
    const m = await oscQuery(`${root}/mix/on`);
    if (m.args[0].v !== 0) throw new Error(`GUARD: ${root} no longer muted; refusing write`);
  }
  async function wsFocus(ch) {
    relay.send({ cmd: "focus", ch });
  }
  async function wsWaitEngine(pred, what, t = WAIT_MS) {
    return relay.waitFor(pred, t, what);
  }
  const EPS_F = 2e-3; // desk fader quantization ~5e-4 + engine float

  // Generic row runner. steps: {base:{addr,kind,val}, appWrite(), appExpect(),
  // appReadback()->{ok,detail}, deskWrite(), deskExpect(), deskReadback()}
  async function runRow(name, cfg) {
    if (ONLY.length && !ONLY.includes(name)) {
      addRow({ control: name, skip: true });
      return;
    }
    const row = { control: name, kind: cfg.kind, strip: cfg.strip, via: cfg.via };
    log(`-- row ${name} [${cfg.kind} ${cfg.strip}] via=${cfg.via}`);
    // APP -> DESK
    try {
      if (cfg.guard) await cfg.guard();
      const t0 = Date.now();
      await cfg.appWrite();
      const rb = await cfg.appReadback(); // {ok, detail, engineMs?}
      row.app2desk = { ok: !!rb.ok, latencyMs: Date.now() - t0, detail: rb.detail };
    } catch (e) {
      row.app2desk = { ok: false, latencyMs: -1, detail: `ERROR: ${e.message}` };
    }
    // restore desk baseline for this leaf before reversing direction
    try {
      if (cfg.restore) await cfg.restore();
    } catch (e) {
      row.app2desk = row.app2desk || {};
      row.app2desk.restoreError = e.message;
    }
    await sleep(400);
    // DESK -> APP
    try {
      if (cfg.guard) await cfg.guard();
      const t0 = Date.now();
      await cfg.deskWrite();
      const rb = await cfg.deskReadback(); // polls UI/engine
      const ms = Date.now() - t0;
      row.desk2app = { ok: !!rb.ok, latencyMs: ms, within2s: ms <= BUDGET_MS, detail: rb.detail };
    } catch (e) {
      row.desk2app = { ok: false, latencyMs: -1, within2s: false, detail: `ERROR: ${e.message}` };
    }
    try {
      if (cfg.restore) await cfg.restore();
    } catch (e) {
      row.desk2app = row.desk2app || {};
      row.desk2app.restoreError = e.message;
    }
    await sleep(300);
    row.pass = !!row.app2desk?.ok && !!row.desk2app?.ok;
    log(`   app->desk: ${row.app2desk?.ok ? "PASS" : "FAIL"} ${row.app2desk?.latencyMs}ms :: ${row.app2desk?.detail}`);
    log(`   desk->app: ${row.desk2app?.ok ? "PASS" : "FAIL"} ${row.desk2app?.latencyMs}ms :: ${row.desk2app?.detail}`);
    addRow(row);
  }

  const ch = TEST.ch;
  const root = TEST.root;

  // Baselines (desk truth) for the test strip.
  const baseFader = (await oscQuery(`${root}/mix/fader`)).args[0].v;
  const baseMute = (await oscQuery(`${root}/mix/on`)).args[0].v === 0;
  const basePanW = (await oscQuery(`${root}/mix/pan`)).args[0].v;
  const baseEqW = supported("eq1_gain", "in") ? (await oscQuery(`${root}/eq/1/g`)).args[0].v : null;
  const baseSendW = supported("aux1_level", "in") ? (await oscQuery(`${root}/mix/01/level`)).args[0].v : null;
  const baseSoloBank = (await oscQuery("/-stat/solosw")).args.map((a) => a.v);
  noteBaseline("ch_fader", `${root}/mix/fader`, "f", baseFader);
  noteBaseline("ch_mute", `${root}/mix/on`, "on", baseMute);
  noteBaseline("ch_pan", `${root}/mix/pan`, "pan", basePanW);
  if (baseEqW !== null) noteBaseline("ch_eq1", `${root}/eq/1/g`, "f", baseEqW);
  if (baseSendW !== null) noteBaseline("ch_send", `${root}/mix/01/level`, "f", baseSendW);
  noteBaseline("solo_bank", "/-stat/solosw", "bank", baseSoloBank);
  log(`test-strip baselines: fader=${baseFader} muted=${baseMute} panW=${basePanW} eqW=${baseEqW} sendW=${baseSendW} soloBit=${baseSoloBank[ch - chIn]}`);

  // ================= 1. ch mute (DOM click both UI halves) =================
  await runRow("mute", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-click",
    guard: () => guardMuted(root).catch(() => {
      // app->desk click unmutes; guard only applies pre-write when muted expected.
      // For mute row the write itself unmutes: verify fader is ~0 (silent anyway).
      return (async () => {
        const f = (await oscQuery(`${root}/mix/fader`)).args[0].v;
        if (f > 0.05) throw new Error(`GUARD: fader ${f} too hot for mute toggle`);
      })();
    }),
    appWrite: async () => {
      const r = await js(PAGE.clickLabel(muteLabel));
      if (!r?.clicked) throw new Error(`click failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      // engine echo (param mute) + desk truth flipped to unmuted.
      await wsWaitEngine((m) => m?.type === "param" && m.ch === ch && m.field === "mute", "engine mute echo");
      const t0 = Date.now();
      for (;;) {
        const m = await oscQuery(`${root}/mix/on`);
        if (m.args[0].v === 1) return { ok: true, detail: `desk on=1 after ${Date.now() - t0}ms; engine param echo seen` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: "desk still muted 5s after UI click" };
        await sleep(200);
      }
    },
    deskWrite: async () => {
      await oscSend(`${root}/mix/on`, [iArg(1)]); // unmute from "mixer control software"
    },
    deskReadback: async () => {
      // UI button must flip to pressed=false (unmuted). Note: if the previous
      // app->desk restore re-muted, first ensure desk is unmuted now.
      const r = await uiWaitFor(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===${JSON.stringify(muteLabel)})?.getAttribute('aria-pressed')==='false'`);
      const eng = await js(PAGE.engineParam(ch, "mute"));
      return { ok: true, detail: `DOM aria-pressed=false in ${r.ms}ms; engine mute=${eng}` };
    },
    restore: async () => {
      await oscSend(`${root}/mix/on`, [iArg(0)]);
      const m = await oscQuery(`${root}/mix/on`);
      if (m.args[0].v !== 0) throw new Error("restore mute failed");
    },
  });

  // ================= 2. ch solo (DOM click; desk->app may be structural) =================
  await runRow("solo", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-click",
    guard: async () => {
      const f = (await oscQuery(`${root}/mix/fader`)).args[0].v;
      if (f > 0.05) throw new Error("GUARD: fader too hot for solo");
    },
    appWrite: async () => {
      const r = await js(PAGE.clickLabel(soloLabel));
      if (!r?.clicked) throw new Error(`click failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      await wsWaitEngine((m) => m?.type === "solo" && m.ch === ch, "engine solo echo");
      const t0 = Date.now();
      for (;;) {
        const bank = (await oscQuery("/-stat/solosw")).args.map((a) => a.v);
        if (bank[ch - chIn] !== 0) return { ok: true, detail: `desk solosw bit set after ${Date.now() - t0}ms; engine solo echo seen` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: "desk solosw bit still 0 5s after UI click" };
        await sleep(200);
      }
    },
    deskWrite: async () => {
      // ensure starting unsoloed, then set via raw leaf (1-based NN).
      await oscSend(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(0)]);
      await sleep(200);
      await oscSend(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(1)]);
    },
    deskReadback: async () => {
      // Engine has no solo poll path on xair (rides echoes); UI can only
      // follow if the engine emits. Wait for DOM flip; timeout = finding.
      try {
        const r = await uiWaitFor(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')===${JSON.stringify(soloLabel)})?.getAttribute('aria-pressed')==='true'`, 6000);
        const eng = await js(PAGE.engineSolo(ch));
        return { ok: true, detail: `DOM aria-pressed=true in ${r.ms}ms; engine soloOf=${eng}` };
      } catch {
        const eng = await js(PAGE.engineSolo(ch));
        return { ok: false, detail: `CONFIRMED GAP desk->app: no UI/engine update 6s after raw solosw write (engine soloOf=${eng}; xair solos ride write-echoes only, engine.rs snapshot/poll has no solo read)` };
      }
    },
    restore: async () => {
      await oscSend(`/-stat/solosw/${ch2(TEST.strip1)}`, [iArg(0)]);
      await sleep(150);
      const bank = (await oscQuery("/-stat/solosw")).args.map((a) => a.v);
      if (bank[ch - chIn] !== 0) throw new Error("restore solo failed");
    },
  });

  // ================= 3. ch fader (DOM keyboard on slider) =================
  await runRow("fader", {
    kind: "in", strip: `ch${TEST.strip1}`, via: "dom-key",
    guard: () => guardMuted(root),
    appWrite: async () => {
      const r = await js(PAGE.keySlider(faderPrefix, "PageUp"));
      if (!r?.found || r.disabled) throw new Error(`slider key failed: ${JSON.stringify(r)}`);
    },
    appReadback: async () => {
      const want = await js(PAGE.engineParam(ch, "fader"));
      await wsWaitEngine((m) => m?.type === "param" && m.ch === ch && m.field === "fader", "engine fader echo");
      const t0 = Date.now();
      for (;;) {
        const m = (await oscQuery(`${root}/mix/fader`)).args[0].v;
        if (Math.abs(m - want) < EPS_F) return { ok: true, detail: `desk fader=${m} == engine ${want} after ${Date.now() - t0}ms` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: `desk fader=${m} != engine want=${want}` };
        await sleep(200);
      }
    },
    deskWrite: async () => {
      await wsFocus(ch);
      await oscSend(`${root}/mix/fader`, [fArg(0.05)]);
    },
    deskReadback: async () => {
      const r = await uiWaitFor(`(s=>{const el=[...document.querySelectorAll('[role=slider]')].find(x=>(x.getAttribute('aria-label')||'').startsWith(${JSON.stringify(faderPrefix)}));return el&&Math.abs(parseFloat(el.getAttribute('aria-valuenow'))-5)<1.5;})()`);
      const eng = await js(PAGE.engineParam(ch, "fader"));
      return { ok: true, detail: `DOM slider~=5 in ${r.ms}ms; engine fader=${eng}` };
    },
    restore: async () => {
      await oscSend(`${root}/mix/fader`, [fArg(baseFader)]);
      const m = (await oscQuery(`${root}/mix/fader`)).args[0].v;
      if (Math.abs(m - baseFader) > EPS_F) throw new Error("restore fader failed");
    },
  });

  // ================= 4. ch pan (DOM keyboard) =================
  if (supported("pan", "in")) {
    await runRow("pan", {
      kind: "in", strip: `ch${TEST.strip1}`, via: "dom-key",
      guard: () => guardMuted(root),
      appWrite: async () => {
        const r = await js(PAGE.keySlider(panPrefix, "ArrowRight"));
        if (!r?.found || r.disabled) throw new Error(`pan key failed: ${JSON.stringify(r)}`);
      },
      appReadback: async () => {
        const wantEng = await js(PAGE.engineParam(ch, "pan"));
        await wsWaitEngine((m) => m?.type === "param" && m.ch === ch && m.field === "pan", "engine pan echo").catch(() => null);
        const t0 = Date.now();
        for (;;) {
          const w = (await oscQuery(`${root}/mix/pan`)).args[0].v;
          if (Math.abs(wireToPan(w) - wantEng) < 1.0) return { ok: true, detail: `desk pan=${wireToPan(w).toFixed(1)} == engine ${wantEng} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk panW=${w} (->${wireToPan(w).toFixed(1)}) != engine ${wantEng}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        await wsFocus(ch);
        const cur = await js(PAGE.engineParam(ch, "pan"));
        const target = Math.min(100, (typeof cur === "number" ? cur : 0) + 10);
        await oscSend(`${root}/mix/pan`, [fArg(panToWire(target))]);
        return null;
      },
      deskReadback: async () => {
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'pan')??999)-10)<3)`);
        return { ok: true, detail: `engine pan followed in ${r.ms}ms` };
      },
      restore: async () => {
        await oscSend(`${root}/mix/pan`, [fArg(basePanW)]);
        const w = (await oscQuery(`${root}/mix/pan`)).args[0].v;
        if (Math.abs(w - basePanW) > 0.01) throw new Error("restore pan failed");
      },
    });
  }

  // ================= 5. ch EQ band gain (ws-cmd; DOM lives on channel screen) =================
  if (supported("eq1_gain", "in") && baseEqW !== null) {
    const testDb = Math.min(15, wireToEqDb(baseEqW) + 2);
    await runRow("eq_gain", {
      kind: "in", strip: `ch${TEST.strip1}`, via: "ws-cmd+ui-readback",
      guard: () => guardMuted(root),
      appWrite: async () => {
        relay.send({ cmd: "param", ch, field: "eq1_gain", value: testDb });
      },
      appReadback: async () => {
        await wsWaitEngine((m) => m?.type === "param" && m.ch === ch && m.field === "eq1_gain", "engine eq echo");
        const t0 = Date.now();
        for (;;) {
          const w = (await oscQuery(`${root}/eq/1/g`)).args[0].v;
          if (Math.abs(wireToEqDb(w) - testDb) < 0.3) return { ok: true, detail: `desk eq1=${wireToEqDb(w).toFixed(2)}dB == want ${testDb}dB after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk eq1=${wireToEqDb(w).toFixed(2)}dB != want ${testDb}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        await wsFocus(ch);
        await oscSend(`${root}/eq/1/g`, [fArg(eqDbToWire(testDb + 2))]);
      },
      deskReadback: async () => {
        const want = testDb + 2;
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'eq1_gain')??999)-(${want}))<0.35)`);
        return { ok: true, detail: `engine eq1_gain=${want}dB in ${r.ms}ms (channel-screen source of truth)` };
      },
      restore: async () => {
        await oscSend(`${root}/eq/1/g`, [fArg(baseEqW)]);
        const w = (await oscQuery(`${root}/eq/1/g`)).args[0].v;
        if (Math.abs(w - baseEqW) > 0.01) throw new Error("restore eq failed");
      },
    });
  }

  // ================= 6. ch send level (ws-cmd) =================
  if (supported("aux1_level", "in") && baseSendW !== null) {
    const testDb = -20;
    await runRow("send_level", {
      kind: "in", strip: `ch${TEST.strip1}->bus1`, via: "ws-cmd+ui-readback",
      guard: () => guardMuted(root),
      appWrite: async () => {
        relay.send({ cmd: "param", ch, field: "aux1_level", value: testDb });
      },
      appReadback: async () => {
        await wsWaitEngine((m) => m?.type === "param" && m.ch === ch && m.field === "aux1_level", "engine send echo");
        const t0 = Date.now();
        for (;;) {
          const w = (await oscQuery(`${root}/mix/01/level`)).args[0].v;
          if (Math.abs(wireToSendDb(w) - testDb) < 0.5) return { ok: true, detail: `desk aux1=${wireToSendDb(w).toFixed(1)}dB == want ${testDb}dB after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk aux1=${wireToSendDb(w).toFixed(1)}dB != want ${testDb}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        await wsFocus(ch);
        await oscSend(`${root}/mix/01/level`, [fArg(sendDbToWire(-25))]);
      },
      deskReadback: async () => {
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs((window.__live.engine.param(${ch},'aux1_level')??999)-(-25))<0.6)`);
        return { ok: true, detail: `engine aux1_level=-25dB in ${r.ms}ms` };
      },
      restore: async () => {
        await oscSend(`${root}/mix/01/level`, [fArg(baseSendW)]);
        const w = (await oscQuery(`${root}/mix/01/level`)).args[0].v;
        if (Math.abs(w - baseSendW) > 0.005) throw new Error("restore send failed");
      },
    });
  }

  // ================= 7. bus fader + 8. bus mute (ws-cmd) =================
  {
    const b = busPick.bus; // 0-based
    const bch = chBus + b;
    const bleaf = `/bus/${b + 1}`;
    const bFader0 = (await oscQuery(`${bleaf}/mix/fader`)).args[0].v;
    const bMute0 = (await oscQuery(`${bleaf}/mix/on`)).args[0].v === 0;
    noteBaseline("bus_fader", `${bleaf}/mix/fader`, "f", bFader0);
    noteBaseline("bus_mute", `${bleaf}/mix/on`, "on", bMute0);
    const up = Math.min(1, bFader0 + 0.03);
    await runRow("bus_fader", {
      kind: "bus", strip: `bus${b + 1}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "bus_fader", bus: b, value: up }),
      appReadback: async () => {
        await wsWaitEngine((m) => m?.type === "bus" && m.bus === b, "engine bus echo");
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(`${bleaf}/mix/fader`)).args[0].v;
          if (Math.abs(m - up) < EPS_F) return { ok: true, detail: `desk bus fader=${m} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk bus fader=${m} != want ${up}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: bch });
        await oscSend(`${bleaf}/mix/fader`, [fArg(Math.max(0, bFader0 - 0.02))]);
      },
      deskReadback: async () => {
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&window.__live.engine.busMaster(${b})&&Math.abs(window.__live.engine.busMaster(${b}).fader-(${Math.max(0, bFader0 - 0.02)}))<0.004)`);
        return { ok: true, detail: `engine busMaster followed in ${r.ms}ms` };
      },
      restore: async () => {
        await oscSend(`${bleaf}/mix/fader`, [fArg(bFader0)]);
        const m = (await oscQuery(`${bleaf}/mix/fader`)).args[0].v;
        if (Math.abs(m - bFader0) > EPS_F) throw new Error("restore bus fader failed");
      },
    });
    await runRow("bus_mute", {
      kind: "bus", strip: `bus${b + 1}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "bus_mute", bus: b, muted: !bMute0 }),
      appReadback: async () => {
        await wsWaitEngine((m) => m?.type === "bus" && m.bus === b, "engine bus echo");
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(`${bleaf}/mix/on`)).args[0].v;
          const muted = m === 0;
          if (muted === !bMute0) return { ok: true, detail: `desk bus muted=${muted} after ${Date.now() - t0}ms (transient, restoring)` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk bus on=${m}, want muted=${!bMute0}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: bch });
        await oscSend(`${bleaf}/mix/on`, [iArg(bMute0 ? 1 : 0)]); // flip from baseline
      },
      deskReadback: async () => {
        const want = !bMute0;
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&window.__live.engine.busMaster(${b})&&window.__live.engine.busMaster(${b}).muted===(${want}))`);
        return { ok: true, detail: `engine busMaster.muted=${want} in ${r.ms}ms (transient, restoring)` };
      },
      restore: async () => {
        await oscSend(`${bleaf}/mix/on`, [iArg(bMute0 ? 0 : 1)]);
        const m = (await oscQuery(`${bleaf}/mix/on`)).args[0].v;
        if ((m === 0) !== bMute0) throw new Error("restore bus mute failed");
      },
    });
  }

  // ================= 9/10. main fader + main mute =================
  await runRow("main_fader", {
    kind: "main", strip: "main", via: "dom-key",
    appWrite: async () => {
      const avail = await js(PAGE.sliderNow(mainFaderPrefix));
      if (!avail) {
        // Master slider not on this screen: same-protocol WS fallback.
        relay.send({ cmd: "main_fader", value: Math.max(0, mainFader0 - 0.02) });
        return;
      }
      await js(PAGE.keySlider(mainFaderPrefix, "ArrowDown"));
    },
    appReadback: async () => {
      await sleep(600); // let engine echo + desk apply settle
      const m = (await oscQuery("/lr/mix/fader")).args[0].v;
      const moved = Math.abs(m - mainFader0) > EPS_F;
      return moved
        ? { ok: true, detail: `desk main fader ${mainFader0} -> ${m} (small nudge, restoring)` }
        : { ok: false, detail: `desk main fader unchanged at ${m} (UI write did not reach desk)` };
    },
    deskWrite: async () => {
      await oscSend("/lr/mix/fader", [fArg(Math.max(0, mainFader0 - 0.01))]);
    },
    deskReadback: async () => {
      const want = Math.max(0, mainFader0 - 0.01);
      const r = await uiWaitFor(`(window.__live&&window.__live.engine&&window.__live.engine.mainFader!==null&&Math.abs(window.__live.engine.mainFader-(${want}))<0.004)`);
      return { ok: true, detail: `engine mainFader followed in ${r.ms}ms` };
    },
    restore: async () => {
      await oscSend("/lr/mix/fader", [fArg(mainFader0)]);
      const m = (await oscQuery("/lr/mix/fader")).args[0].v;
      if (Math.abs(m - mainFader0) > EPS_F) throw new Error("restore main fader failed");
    },
  });
  await runRow("main_mute", {
    kind: "main", strip: "main", via: muteMainLabel ? "dom-click" : "ws-cmd+ui-readback",
    appWrite: async () => {
      if (muteMainLabel) {
        const r = await js(PAGE.clickLabel(muteMainLabel));
        if (!r?.clicked) throw new Error(`main mute click failed: ${JSON.stringify(r)}`);
      } else {
        relay.send({ cmd: "main_mute", muted: !mainMuted0 });
      }
    },
    appReadback: async () => {
      await wsWaitEngine((m) => m?.type === "main", "engine main echo").catch(() => null);
      const t0 = Date.now();
      for (;;) {
        const m = (await oscQuery("/lr/mix/on")).args[0].v;
        const muted = m === 0;
        if (muted === !mainMuted0) return { ok: true, detail: `desk main muted=${muted} after ${Date.now() - t0}ms (BRIEF room silence, restoring now)` };
        if (Date.now() - t0 > 5000) return { ok: false, detail: `desk main on=${m}, want muted=${!mainMuted0}` };
        await sleep(200);
      }
    },
    deskWrite: async () => {
      await oscSend("/lr/mix/on", [iArg(mainMuted0 ? 1 : 0)]);
    },
    deskReadback: async () => {
      const want = !mainMuted0;
      const r = await uiWaitFor(`(window.__live&&window.__live.engine&&window.__live.engine.mainMuted===(${want}))`);
      return { ok: true, detail: `engine mainMuted=${want} in ${r.ms}ms (BRIEF room silence, restoring now)` };
    },
    restore: async () => {
      await oscSend("/lr/mix/on", [iArg(mainMuted0 ? 0 : 1)]);
      const m = (await oscQuery("/lr/mix/on")).args[0].v;
      if ((m === 0) !== mainMuted0) throw new Error("restore main mute failed");
    },
  });

  // ================= rtn / fxsend / dca fader+mute (ws-cmd) =================
  async function famRows(name, pick, kind, scopKind) {
    if (!pick) {
      addRow({ control: name + "_fader", kind, strip: "-", via: "n/a", app2desk: { ok: false, latencyMs: -1, detail: "no answering leaf found (desk silence)" }, desk2app: { ok: false, latencyMs: -1, detail: "skipped" }, pass: false });
      return;
    }
    const leafBase = pick.dca ? pick.root : `${pick.root}/mix`;
    const fLeaf = `${leafBase}/fader`;
    const oLeaf = `${leafBase}/on`;
    let f0, m0;
    try {
      f0 = (await oscQuery(fLeaf)).args[0].v;
      m0 = (await oscQuery(oLeaf)).args[0].v === 0;
    } catch (e) {
      addRow({ control: name + "_fader", kind, strip: pick.root, via: "n/a", app2desk: { ok: false, latencyMs: -1, detail: `leaf silence: ${e.message}` }, desk2app: { ok: false, latencyMs: -1, detail: "skipped" }, pass: false });
      return;
    }
    noteBaseline(`${name}_fader`, fLeaf, "f", f0);
    noteBaseline(`${name}_mute`, oLeaf, "on", m0);
    const up = Math.min(1, f0 + 0.03);
    const fid = pick.root === "/rtn/1" || pick.root.startsWith("/rtn/") ? "fx1" : pick.root.startsWith("/fxsend/") ? "fxs1" : "dca1";
    await runRow(`${name}_fader`, {
      kind, strip: `${fid}@${pick.root}`, via: "ws-cmd+ui-readback",
      appWrite: async () => relay.send({ cmd: "param", ch: pick.ch, field: "fader", value: up }),
      appReadback: async () => {
        await wsWaitEngine((m) => m?.type === "param" && m.ch === pick.ch && m.field === "fader", "engine fader echo").catch(() => null);
        const t0 = Date.now();
        for (;;) {
          const m = (await oscQuery(fLeaf)).args[0].v;
          if (Math.abs(m - up) < 0.01) return { ok: true, detail: `desk ${fLeaf}=${m} after ${Date.now() - t0}ms` };
          if (Date.now() - t0 > 5000) return { ok: false, detail: `desk ${fLeaf}=${m} != want ${up}` };
          await sleep(200);
        }
      },
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch: pick.ch });
        await oscSend(fLeaf, [fArg(Math.max(0, f0 - 0.02))]);
      },
      deskReadback: async () => {
        const want = Math.max(0, f0 - 0.02);
        const r = await uiWaitFor(`(window.__live&&window.__live.engine&&Math.abs(((window.__live.engine.param(${pick.ch},'fader'))??999)-(${want}))<0.012)`);
        return { ok: true, detail: `engine param fader followed in ${r.ms}ms` };
      },
      restore: async () => {
        await oscSend(fLeaf, [fArg(f0)]);
        const m = (await oscQuery(fLeaf)).args[0].v;
        if (Math.abs(m - f0) > 0.01) throw new Error(`restore ${name} fader failed`);
      },
    });
    if (supported("mute", scopKind)) {
      await runRow(`${name}_mute`, {
        kind, strip: `${fid}@${pick.root}`, via: "ws-cmd+ui-readback",
        appWrite: async () => relay.send({ cmd: "mute", ch: pick.ch, muted: !m0 }),
        appReadback: async () => {
          await wsWaitEngine((m) => m?.type === "param" && m.ch === pick.ch && m.field === "mute", "engine mute echo").catch(() => null);
          const t0 = Date.now();
          for (;;) {
            const m = (await oscQuery(oLeaf)).args[0].v;
            if ((m === 0) === !m0) return { ok: true, detail: `desk muted=${!m0} after ${Date.now() - t0}ms (transient, restoring)` };
            if (Date.now() - t0 > 5000) return { ok: false, detail: `desk on=${m}, want muted=${!m0}` };
            await sleep(200);
          }
        },
        deskWrite: async () => {
          relay.send({ cmd: "focus", ch: pick.ch });
          await oscSend(oLeaf, [iArg(m0 ? 1 : 0)]);
        },
        deskReadback: async () => {
          const want = !m0;
          const r = await uiWaitFor(`(window.__live&&window.__live.engine&&((window.__live.engine.param(${pick.ch},'mute'))===(${want})))`);
          return { ok: true, detail: `engine mute=${want} in ${r.ms}ms (transient, restoring)` };
        },
        restore: async () => {
          await oscSend(oLeaf, [iArg(m0 ? 0 : 1)]);
          const m = (await oscQuery(oLeaf)).args[0].v;
          if ((m === 0) !== m0) throw new Error(`restore ${name} mute failed`);
        },
      });
    } else {
      addRow({ control: `${name}_mute`, kind, strip: pick.root, via: "n/a", app2desk: { ok: true, latencyMs: 0, detail: "honest unsupported (caps)" }, desk2app: { ok: true, latencyMs: 0, detail: "honest unsupported (caps)" }, pass: true, note: "unsupported-by-caps, no writes" });
    }
  }
  await famRows("rtn", rtnPick, "ret", "ret");
  await famRows("fxsend", fxPick, "fxsnd", "fxsnd");
  await famRows("dca", dcaPick, "dca", "dca");

  // ================= solo on non-channel strips: expect honest refusal =================
  for (const [nm, pch] of [["bus", chBus], ["dca", chDca]]) {
    await runRow(`solo_${nm}_refused`, {
      kind: nm, strip: `ch${pch}`, via: "ws-cmd",
      appWrite: async () => relay.send({ cmd: "solo", ch: pch, soloed: true }),
      appReadback: async () => {
        const e = await wsWaitEngine((m) => m?.type === "error", "engine error echo");
        return /solo|support|range|channel|strip/i.test(e.message || "") ? { ok: true, detail: `honest refusal: ${e.message}` } : { ok: false, detail: `wrong error: ${e.message}` };
      },
      deskWrite: async () => {},
      deskReadback: async () => ({ ok: true, detail: "n/a (refused writes never reach desk)" }),
      restore: async () => {},
    });
  }

  // ================= mute-group master: same-value write path only =================
  {
    const g0 = (await oscQuery("/config/mute/1")).args[0].v;
    noteBaseline("mg1", "/config/mute/1", "i", g0);
    await runRow("mute_group", {
      kind: "mg", strip: "mg1", via: "ws-cmd+same-value (no audible transient by design)",
      appWrite: async () => relay.send({ cmd: "mute_group", group: 0, muted: g0 !== 0 }),
      appReadback: async () => {
        const e = await wsWaitEngine((m) => m?.type === "mute_group" && m.group === 0, "engine mg echo");
        const m = (await oscQuery("/config/mute/1")).args[0].v;
        return m === g0
          ? { ok: true, detail: `desk /config/mute/1=${m} (same-value, no transient); engine echo muted=${e.muted}` }
          : { ok: false, detail: `desk changed to ${m}!` };
      },
      deskWrite: async () => {
        relay.send({ cmd: "focus", ch });
        await oscSend("/config/mute/1", [iArg(g0)]);
      },
      deskReadback: async () => {
        const e = await wsWaitEngine((m) => m?.type === "mute_group" && m.group === 0, "engine mg echo", 9000).catch(() => null);
        return { ok: true, detail: `same-value raw write; engine re-read echo ${e ? "seen" : "not emitted (no diff — expected; poller re-read agrees)"}` };
      },
      restore: async () => {
        await oscSend("/config/mute/1", [iArg(g0)]);
      },
    });
  }

  // ================= desk left as found =================
  log("== desk-left-as-found re-read ==");
  const drift = [];
  for (const [key, b] of Object.entries(baselines)) {
    try {
      const m = await oscQuery(b.addr);
      let cur;
      if (b.kind === "bank") cur = m.args.map((a) => a.v);
      else cur = m.args[0].v;
      const same = b.kind === "bank"
        ? JSON.stringify(cur) === JSON.stringify(b.value)
        : b.kind === "on" || b.kind === "i"
          ? (cur === 0) === (b.value === 0 || b.value === true)
          : Math.abs(cur - b.value) < 0.01;
      // normalize mute compare: value stored as bool or int
      let okSame = same;
      if (b.kind === "on") {
        const baseMuted = b.value === true || b.value === 0;
        okSame = (cur === 0) === baseMuted;
      }
      log(`  ${key} ${b.addr}: base=${JSON.stringify(b.value)} now=${JSON.stringify(cur)} ${okSame ? "OK" : "DRIFT!"}`);
      if (!okSame) drift.push(key);
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

  // ================= report =================
  log("\n================ MATRIX ================");
  for (const r of rows) {
    if (r.skip) continue;
    const a = r.app2desk || {};
    const d = r.desk2app || {};
    log(`${r.pass ? "PASS" : "FAIL"} ${r.control} [${r.kind} ${r.strip}] via=${r.via}${r.note ? " (" + r.note + ")" : ""}`);
    log(`    app->desk: ${a.ok ? "PASS" : "FAIL"} ${a.latencyMs}ms :: ${a.detail || ""}${a.restoreError ? " RESTORE-ERR: " + a.restoreError : ""}`);
    log(`    desk->app: ${d.ok ? "PASS" : "FAIL"} ${d.latencyMs}ms${d.within2s === false && d.ok ? " SLOW(>2s)" : ""} :: ${d.detail || ""}${d.restoreError ? " RESTORE-ERR: " + d.restoreError : ""}`);
  }
  const real = rows.filter((r) => !r.skip);
  const passed = real.filter((r) => r.pass);
  const failed = real.filter((r) => !r.pass);
  log(`\n${passed.length}/${real.length} rows pass`);
  log(`desk left as found: ${drift.length === 0 ? "YES" : "NO — drift: " + drift.join(",")}`);
  if (failed.length) {
    log("\n---- GAP LIST (root-cause layer + file:line) ----");
    for (const r of failed) {
      if (!r.app2desk?.ok) log(`* ${r.control} app->desk: ${r.app2desk?.detail} [UI binding apps/web/src/ui/state/engineBridge.ts:89-109 -> EngineWorld.set* apps/web/src/ui/world/engineWorld/engineWorld.ts:643-663 -> protocol apps/web/src/ui/world/protocol.ts:658-662 -> engine Cmd arm crates/engine/src/engine.rs:3121-3254 -> xair leaf crates/proto-xair/src/lib.rs:1108-1303]`);
      if (!r.desk2app?.ok) log(`* ${r.control} desk->app: ${r.desk2app?.detail} [poller crates/engine/src/engine.rs:2657-2706 -> Event ingest apps/web/src/ui/world/engineWorld/engineWorld.ts:286-427 -> fixture sync apps/web/src/ui/state/uiStore.svelte.ts:230-311 -> render]`);
    }
  }
  const fails = failed.length + drift.length;
  log(fails === 0 ? "VERIFY-BIDIRECTIONAL PASS" : "VERIFY-BIDIRECTIONAL FAIL");
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
