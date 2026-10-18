#!/usr/bin/env node
// Design-diff harness: pixel + layout-box regression of our ui=4 screens
// against the frozen design. See README.md in this directory.
import { createRequire } from "node:module";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { SCREENS, SCREEN_IDS } from "./screens.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..", "..", "..");
const REFS_DIR = path.join(REPO, "docs", "design", "refs");
const DESIGN_DIR = path.join(REPO, "docs", "design");
const DESIGN_URL = "http://127.0.0.1:5301/mixing-station-mixer-eq.html";
const DEFAULT_URL_BASE = "http://127.0.0.1:5291";
const OUT_ROOT = "/tmp/ui4-diff";
const VIEWPORT = { width: 1280, height: 720 };
const DIFF_PY = path.join(HERE, "diff.py");

const require = createRequire(import.meta.url);
const log = (...a) => console.error("[design-diff]", ...a);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function parseArgs(argv) {
  const o = { screen: null, all: false, updateRef: false, selftest: false, urlBase: DEFAULT_URL_BASE, threshold: 1.0, delta: 24 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--screen") o.screen = argv[++i];
    else if (a === "--all") o.all = true;
    else if (a === "--update-ref") o.updateRef = true;
    else if (a === "--selftest") o.selftest = true;
    else if (a === "--url-base") o.urlBase = argv[++i];
    else if (a === "--threshold") o.threshold = parseFloat(argv[++i]);
    else if (a === "--delta") o.delta = parseInt(argv[++i], 10);
    else if (a === "--help" || a === "-h") o.help = true;
    else throw new Error(`unknown arg: ${a}`);
  }
  return o;
}

function loadPlaywright() {
  const cands = [];
  try {
    cands.push(path.dirname(require.resolve("playwright-core/package.json")));
  } catch {}
  cands.push("/tmp/vantyr-pw/node_modules/playwright-core");
  try {
    const g = spawnSync("npm", ["root", "-g"], { encoding: "utf8" }).stdout.trim();
    if (g) cands.push(path.join(g, "playwright-core"));
  } catch {}
  for (const c of cands) {
    try {
      return require(path.join(c, "lib", "server", "chromium", "chromium.js")) ||
        require(c);
    } catch {}
    try {
      return require(c);
    } catch {}
  }
  throw new Error("playwright-core not found (tried local, /tmp/vantyr-pw, global).");
}

function findChrome() {
  if (process.env.CHROMIUM_EXE && fs.existsSync(process.env.CHROMIUM_EXE)) return process.env.CHROMIUM_EXE;
  const base = path.join(os.homedir(), ".cache", "ms-playwright");
  const hits = [];
  if (fs.existsSync(base)) {
    for (const d of fs.readdirSync(base)) {
      for (const sub of ["chrome-linux/chrome", "chrome-linux64/chrome", "chrome-linux/headless_shell"]) {
        const p = path.join(base, d, sub);
        if (fs.existsSync(p)) hits.push(p);
      }
    }
  }
  hits.sort();
  const full = hits.filter((h) => h.endsWith("/chrome"));
  if (full.length) return full[full.length - 1];
  if (hits.length) return hits[hits.length - 1];
  throw new Error("no Chromium found under ~/.cache/ms-playwright");
}

async function ensureDesignServer() {
  try {
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), 4000);
    const r = await fetch(DESIGN_URL, { signal: ctl.signal });
    clearTimeout(t);
    if (r.ok) return false;
    throw new Error(`status ${r.status}`);
  } catch (e) {
    log(`design server not responding (${e.message}); starting python http.server on :5301`);
    const child = spawn("python3", ["-I", "-m", "http.server", "5301", "--bind", "127.0.0.1", "--directory", DESIGN_DIR], {
      detached: true,
      stdio: "ignore",
    });
    child.unref();
    for (let i = 0; i < 25; i++) {
      await sleep(400);
      try {
        const r = await fetch(DESIGN_URL);
        if (r.ok) {
          log("design server started by this tool (started=true)");
          return true;
        }
      } catch {}
    }
    throw new Error("could not start design server on :5301");
  }
}

function neutralizeMeters() {
  const orig = window.setInterval;
  window.setInterval = function (fn, ms, ...rest) {
    if (Number(ms) === 80) return 0; // design stepMeters loop: keep meters frozen
    return orig.call(window, fn, ms, ...rest);
  };
}

function findAndClick(step) {
  const norm = (s) => (s == null ? "" : String(s)).replace(/\s+/g, " ").trim();
  const acc = (el) => norm(el.getAttribute("aria-label") || el.innerText);
  const btns = [...document.querySelectorAll("button")];
  let el = null,
    desc = "";
  if (step.clickAria) {
    el = btns.find((b) => b.getAttribute("aria-label") === step.clickAria) || null;
    desc = "aria=" + step.clickAria;
  } else if (step.clickAriaPrefix) {
    el = btns.find((b) => (b.getAttribute("aria-label") || "").startsWith(step.clickAriaPrefix)) || null;
    desc = "ariaPrefix=" + step.clickAriaPrefix;
  } else if (step.clickText) {
    const c = btns.filter((b) => norm(b.innerText) === step.clickText);
    c.sort((a, b) => a.getBoundingClientRect().width * a.getBoundingClientRect().height - b.getBoundingClientRect().width * b.getBoundingClientRect().height);
    el = c[0] || null;
    desc = "text=" + step.clickText;
  } else if (step.clickTextPrefix) {
    const c = btns.filter((b) => acc(b).startsWith(step.clickTextPrefix) || norm(b.innerText).startsWith(step.clickTextPrefix));
    c.sort((a, b) => a.getBoundingClientRect().width * a.getBoundingClientRect().height - b.getBoundingClientRect().width * b.getBoundingClientRect().height);
    el = c[0] || null;
    desc = "textPrefix=" + step.clickTextPrefix;
  } else if (step.clickInGroup) {
    const cont = [...document.querySelectorAll("[aria-label]")].find(
      (e) => e.getAttribute("aria-label") === step.clickInGroup.group && e.querySelector("button")
    );
    if (cont) el = [...cont.querySelectorAll("button")].find((b) => norm(b.innerText) === step.clickInGroup.text) || null;
    desc = "group=" + step.clickInGroup.group + " text=" + step.clickInGroup.text;
  }
  if (!el) return "NOTFOUND:" + desc;
  el.click();
  return "clicked:" + desc + " -> " + acc(el).slice(0, 60);
}

function measureBoxes(specs) {
  const norm = (s) => (s == null ? "" : String(s)).replace(/\s+/g, " ").trim();
  const r = (el) => {
    const b = el.getBoundingClientRect();
    return { x: Math.round(b.x * 100) / 100, y: Math.round(b.y * 100) / 100, w: Math.round(b.width * 100) / 100, h: Math.round(b.height * 100) / 100 };
  };
  return specs.map((s) => {
    const m = s.match;
    let el = null;
    try {
      if (m.kind === "aria") el = document.querySelector('[aria-label="' + m.value + '"]');
      else if (m.kind === "ariaPrefix")
        el = [...document.querySelectorAll("[aria-label]")].find((e) => (e.getAttribute("aria-label") || "").startsWith(m.value)) || null;
      else if (m.kind === "css") el = document.querySelector(m.value);
      else if (m.kind === "text" || m.kind === "textPrefix") {
        const pool = [...document.querySelectorAll("button,section,nav,aside,main,div[role],span")];
        const c = pool
          .filter((e) => {
            const t = norm(e.innerText);
            return m.kind === "text" ? t === m.value : t.startsWith(m.value);
          })
          .filter((e) => e.getBoundingClientRect().width > 0);
        c.sort((a, b) => a.getBoundingClientRect().width * a.getBoundingClientRect().height - b.getBoundingClientRect().width * b.getBoundingClientRect().height);
        el = c[0] || null;
      }
    } catch (e) {
      return { name: s.name, found: false, error: String((e && e.message) || e) };
    }
    if (!el) return { name: s.name, found: false };
    return { name: s.name, found: true, rect: r(el) };
  });
}

async function waitFonts(page) {
  await page.evaluate(() => Promise.race([document.fonts.ready, new Promise((r) => setTimeout(r, 5000))])).catch(() => {});
}

async function navigateDesign(page, id) {
  const { steps } = SCREENS[id];
  await page.goto(DESIGN_URL, { waitUntil: "load", timeout: 30000 });
  await waitFonts(page);
  await sleep(500);
  for (const step of steps) {
    const res = await page.evaluate(findAndClick, step);
    if (String(res).startsWith("NOTFOUND")) throw new Error(`design nav failed on ${id}: ${res}`);
    log(`${id}: ${res}`);
    await sleep(450);
  }
  await waitFonts(page);
  await sleep(400);
}

function py(args) {
  const r = spawnSync("python3", ["-I", DIFF_PY, ...args], { encoding: "utf8", timeout: 120000 });
  return r;
}

async function captureDesignFrozen(ctx, id) {
  const page = await ctx.newPage();
  try {
    await navigateDesign(page, id);
    const shotOpts = { animations: "disabled" };
    const a = await page.screenshot(shotOpts);
    await sleep(600);
    const b = await page.screenshot(shotOpts);
    const boxes = await page.evaluate(measureBoxes, SCREENS[id].boxes);
    return { a, b, boxes };
  } finally {
    await page.close();
  }
}

async function captureDesignLive(ctx, id, nShots, gapMs) {
  const page = await ctx.newPage();
  try {
    await navigateDesign(page, id);
    await sleep(700);
    const shots = [];
    for (let i = 0; i < nShots; i++) {
      if (i) await sleep(gapMs);
      shots.push(await page.screenshot());
    }
    return shots;
  } finally {
    await page.close();
  }
}

async function captureOurs(ctx, id, urlBase) {
  const page = await ctx.newPage();
  let warned = false;
  try {
    const url = `${urlBase}/?ui=4&fixture=1&freeze=1&${SCREENS[id].ours}`;
    await page.goto(url, { waitUntil: "load", timeout: 30000 });
    await waitFonts(page);
    try {
      await page.waitForFunction("window.__uiReady === true", { timeout: 8000 });
    } catch {
      warned = true;
    }
    await sleep(400);
    const shot = await page.screenshot({ animations: "disabled" });
    let boxes = [];
    try {
      boxes = await page.evaluate(measureBoxes, SCREENS[id].boxes);
    } catch (e) {
      boxes = SCREENS[id].boxes.map((s) => ({ name: s.name, found: false, error: "measureFailed" }));
    }
    return { shot, boxes, warned };
  } finally {
    await page.close();
  }
}

function compareBoxes(refBoxes, oursMeasured) {
  const byName = new Map(oursMeasured.map((m) => [m.name, m]));
  const failed = [];
  let pass = 0;
  for (const rb of refBoxes) {
    const m = byName.get(rb.name);
    if (!m || !m.found) {
      failed.push({ name: rb.name, reason: "missing" });
      continue;
    }
    const d = [Math.abs(m.rect.x - rb.ref.x), Math.abs(m.rect.y - rb.ref.y), Math.abs(m.rect.w - rb.ref.w), Math.abs(m.rect.h - rb.ref.h)];
    if (d.every((v) => v <= 1.0 + 1e-9)) pass++;
    else failed.push({ name: rb.name, reason: "off", delta: d.map((v) => Math.round(v * 100) / 100), ref: rb.ref, ours: m.rect });
  }
  return { total: refBoxes.length, pass, failed };
}

function readBoxesFile(id) {
  const p = path.join(REFS_DIR, `${id}.boxes.json`);
  if (!fs.existsSync(p)) return null;
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

async function runUpdateRef(browser, ids, pw) {
  fs.mkdirSync(REFS_DIR, { recursive: true });
  const frozen = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: "reduce" });
  await frozen.addInitScript(neutralizeMeters);
  const live = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: "reduce" });
  let failed = 0;
  for (const id of ids) {
    const outDir = path.join(OUT_ROOT, id);
    fs.mkdirSync(outDir, { recursive: true });
    try {
      const { a, b, boxes } = await captureDesignFrozen(frozen, id);
      const identical = a.equals(b);
      fs.writeFileSync(path.join(REFS_DIR, `${id}.png`), a);
      // Live captures for mask generation: several shots over ~6 s so the
      // union of pairwise diffs covers the full travel of meters/GR/dots.
      const lv = await captureDesignLive(live, id, 5, 1200);
      const shotPaths = lv.map((buf, i) => {
        const p = path.join(outDir, `live-${i}.png`);
        fs.writeFileSync(p, buf);
        return p;
      });
      const maskPath = path.join(REFS_DIR, `${id}.mask.json`);
      const preview = path.join(outDir, "mask-preview.png");
      const mr = py(["mask", "--shots", ...shotPaths, "--out", maskPath, "--preview", preview]);
      if (mr.status !== 0) throw new Error(`mask failed: ${mr.stderr.slice(-500)}`);
      const maskInfo = JSON.parse(mr.stdout.trim().split("\n").pop());
      const missing = boxes.filter((x) => !x.found).map((x) => x.name);
      const boxRecs = SCREENS[id].boxes.map((s, i) => ({
        name: s.name,
        match: s.match,
        ref: boxes[i] && boxes[i].found ? boxes[i].rect : null,
      }));
      fs.writeFileSync(path.join(REFS_DIR, `${id}.boxes.json`), JSON.stringify({ version: 1, screen: id, boxes: boxRecs }, null, 2));
      const summary = {
        screen: id,
        mode: "update-ref",
        refBytes: a.length,
        stableRef: identical,
        maskRects: maskInfo.rects,
        maskedPct: maskInfo.coveragePct,
        boxesTotal: boxes.length,
        boxesMissing: missing,
        pass: identical && missing.length === 0,
      };
      if (!summary.pass) failed++;
      console.log(JSON.stringify(summary));
      if (!identical) log(`${id}: WARNING refs not stable (two shots differ)`);
      if (missing.length) log(`${id}: WARNING boxes unresolved: ${missing.join(", ")}`);
    } catch (e) {
      failed++;
      console.log(JSON.stringify({ screen: id, mode: "update-ref", pass: false, error: String(e.message || e) }));
    }
  }
  await frozen.close();
  await live.close();
  return failed;
}

async function runDiff(browser, ids, opts) {
  const ctx = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: "reduce" });
  let failed = 0;
  for (const id of ids) {
    const outDir = path.join(OUT_ROOT, id);
    fs.mkdirSync(outDir, { recursive: true });
    const t0 = Date.now();
    try {
      const refPath = path.join(REFS_DIR, `${id}.png`);
      const maskPath = path.join(REFS_DIR, `${id}.mask.json`);
      if (!fs.existsSync(refPath)) throw new Error(`no reference ${refPath} (run --update-ref first)`);
      const { shot, boxes, warned } = await captureOurs(ctx, id, opts.urlBase);
      const oursPath = path.join(outDir, "ours.png");
      fs.writeFileSync(oursPath, shot);
      const args = ["diff", "--ref", refPath, "--ours", oursPath, "--out-dir", outDir, "--threshold", String(opts.threshold), "--delta", String(opts.delta)];
      if (fs.existsSync(maskPath)) args.push("--mask", maskPath);
      const dr = py(args);
      const diffOut = JSON.parse(dr.stdout.trim().split("\n").pop());
      const refBoxes = readBoxesFile(id);
      let boxRes;
      if (!refBoxes) boxRes = { total: 0, pass: 0, failed: [], error: "noBoxesFile" };
      else boxRes = compareBoxes(refBoxes.boxes, boxes);
      const boxesPass = !boxRes.error && boxRes.failed.length === 0 && boxRes.total > 0;
      const pass = diffOut.pass && boxesPass;
      if (!pass) failed++;
      console.log(JSON.stringify({
        screen: id,
        mode: "diff",
        mismatchPct: diffOut.mismatchPct,
        threshold: opts.threshold,
        delta: opts.delta,
        maskedPct: diffOut.maskedPct,
        pass,
        diffPass: diffOut.pass,
        boxesPass,
        boxes: { total: boxRes.total, pass: boxRes.pass, failed: boxRes.failed },
        worstCells: diffOut.worstCells,
        warnedUiReady: warned,
        outDir,
        ms: Date.now() - t0,
      }));
    } catch (e) {
      failed++;
      console.log(JSON.stringify({ screen: id, mode: "diff", pass: false, error: String(e.message || e) }));
    }
  }
  await ctx.close();
  return failed;
}

async function runSelftest(browser) {
  const outDir = path.join(OUT_ROOT, "selftest");
  fs.mkdirSync(outDir, { recursive: true });
  const frozen = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: "reduce" });
  await frozen.addInitScript(neutralizeMeters);
  const results = {};
  try {
    const page = await frozen.newPage();
    await navigateDesign(page, "mixer");
    const a = await page.screenshot({ animations: "disabled" });
    await sleep(600);
    const b = await page.screenshot({ animations: "disabled" });
    const measured = await page.evaluate(measureBoxes, SCREENS.mixer.boxes);
    await page.close();
    const ra = path.join(outDir, "ref-a.png");
    const rb = path.join(outDir, "ref-b.png");
    fs.writeFileSync(ra, a);
    fs.writeFileSync(rb, b);
    const r1 = py(["diff", "--ref", ra, "--ours", rb, "--out-dir", outDir, "--threshold", "1.0", "--delta", "24"]);
    const o1 = JSON.parse(r1.stdout.trim().split("\n").pop());
    results.identity = { pass: r1.status === 0 && o1.mismatchPct === 0, mismatchPct: o1.mismatchPct };
    const shifted = path.join(outDir, "shifted.png");
    py(["shift", "--src", ra, "--dst", shifted, "--dx", "2"]);
    const sod = path.join(outDir, "shifted-diff");
    fs.mkdirSync(sod, { recursive: true });
    const r2 = py(["diff", "--ref", ra, "--ours", shifted, "--out-dir", sod, "--threshold", "1.0", "--delta", "24"]);
    const o2 = JSON.parse(r2.stdout.trim().split("\n").pop());
    results.shifted = { pass: r2.status === 1 && o2.mismatchPct > 1.0, mismatchPct: o2.mismatchPct };
    const refBoxes = readBoxesFile("mixer");
    if (!refBoxes) {
      results.boxes = { pass: false, error: "docs/design/refs/mixer.boxes.json missing (run --update-ref --all first)" };
    } else {
      const cmp = compareBoxes(refBoxes.boxes, measured);
      results.boxes = { pass: cmp.failed.length === 0 && cmp.total > 0, total: cmp.total, failed: cmp.failed.map((f) => f.name) };
    }
  } finally {
    await frozen.close();
  }
  const pass = results.identity.pass && results.shifted.pass && results.boxes.pass;
  console.log(JSON.stringify({ mode: "selftest", pass, ...results }));
  return pass ? 0 : 1;
}

function usage() {
  return `usage:
  node run.mjs --screen <id> [--url-base URL] [--threshold PCT] [--delta N]
  node run.mjs --all [--url-base URL]
  node run.mjs --screen <id> --update-ref | --update-ref --all
  node run.mjs --selftest
screens: ${SCREEN_IDS.join(", ")}`;
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    console.error(usage());
    process.exit(2);
  }
  if (opts.help) {
    console.error(usage());
    process.exit(0);
  }
  if (opts.selftest) {
    await ensureDesignServer();
    const pw = loadPlaywright();
    const browser = await pw.chromium.launch({ executablePath: findChrome() });
    try {
      process.exit(await runSelftest(browser));
    } finally {
      await browser.close();
    }
  }
  let ids;
  if (opts.all) ids = SCREEN_IDS;
  else if (opts.screen) {
    if (!SCREENS[opts.screen]) {
      console.error(`unknown screen: ${opts.screen}\n${usage()}`);
      process.exit(2);
    }
    ids = [opts.screen];
  } else {
    console.error(usage());
    process.exit(2);
  }
  const started = await ensureDesignServer();
  if (started) log("design server was down; started locally on :5301");
  const pw = loadPlaywright();
  const browser = await pw.chromium.launch({ executablePath: findChrome() });
  try {
    let failed;
    if (opts.updateRef) failed = await runUpdateRef(browser, ids, pw);
    else failed = await runDiff(browser, ids, opts);
    process.exit(failed ? 1 : 0);
  } catch (e) {
    console.error(`FATAL ${e.stack || e}`);
    process.exit(2);
  } finally {
    await browser.close();
  }
}

main();
