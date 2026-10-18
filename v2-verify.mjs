// R1 verification against the sim (CATALOGUE-V2 package).
// Spawns its own relay on a free port (never :8080), then over WS:
//  1. caps lists the 8 topology families and the full 149-id v2 catalogue
//     (with `scopes`, legacy `scope` kept) + meter_taps;
//  2. every strip family answers its supported scoped ids within the focus
//     budget (per-family counts + wall time);
//  3. flood test: 30 focus commands, then focus strip 40 — its set drains
//     first, first frame well within 3 s;
//  4. prints the XR18-like / DL32S-like caps comparison (from the drivers'
//     unit-tested capabilities()/topology(); exact numbers asserted in
//     Rust: openmix-proto-xair capabilities_match_loopback_tested_set,
//     engine driver_capability_sets).
// Usage: node v2-verify.mjs [--port 8098] [--poll-ms 2]
import { spawn } from "node:child_process";

const args = process.argv.slice(2);
const portArg = args.indexOf("--port");
const PORT = portArg >= 0 ? Number(args[portArg + 1]) : 8098;
const pollArg = args.indexOf("--poll-ms");
const POLL_MS = pollArg >= 0 ? Number(args[pollArg + 1]) : 2;

const RELAY = new URL("../openmix/target/debug/openmix-serve", import.meta.url).pathname;
const EXPECTED_IDS = 149;
const EXPECTED_FAMILIES = [
  ["ch", "in", 0, 32],
  ["mix", "bus", 32, 8],
  ["ch2", "in", 40, 16],
  ["fxret", "ret", 56, 4],
  ["fxsnd", "fxsnd", 60, 4],
  ["mtx", "mtx", 64, 8],
  ["main", "main", 72, 1],
  ["dca", "dca", 73, 4],
];

function startRelay() {
  return new Promise((resolve, reject) => {
    const child = spawn(RELAY, ["--port", String(PORT), "--poll-ms", String(POLL_MS)], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let out = "";
    const timer = setTimeout(() => reject(new Error("relay did not start: " + out)), 15000);
    child.stderr.on("data", (d) => { out += d; });
    child.stdout.on("data", (d) => { out += d; });
    const probe = async () => {
      try {
        const ws = new WebSocket(`ws://127.0.0.1:${PORT}/ws`);
        await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
        clearTimeout(timer);
        ws.close();
        resolve(child);
      } catch {
        setTimeout(probe, 200);
      }
    };
    setTimeout(probe, 500);
  });
}

function connect() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${PORT}/ws`);
    const frames = [];
    ws.onopen = () => resolve({ ws, frames });
    ws.onerror = reject;
    ws.onmessage = (ev) => {
      if (typeof ev.data === "string") frames.push(JSON.parse(ev.data));
    };
  });
}

async function nextFrame(frames, pred, timeoutMs, what) {
  const deadline = Date.now() + timeoutMs;
  let i = 0;
  while (Date.now() < deadline) {
    for (; i < frames.length; i++) if (pred(frames[i])) return frames[i];
    await new Promise((r) => setTimeout(r, 10));
  }
  throw new Error(`timeout waiting for ${what}`);
}

const t0 = Date.now();
const relay = await startRelay();
console.log(`relay up on :${PORT} (poll ${POLL_MS}ms) in ${Date.now() - t0}ms`);
let topo;
let scopedCount;
try {
const { ws, frames } = await connect();
const send = (o) => ws.send(JSON.stringify(o));
const mark = frames.length;

// ---- 1. caps ----
const caps = await nextFrame(frames, (f) => f.type === "caps", 5000, "caps");
const nparams = caps.params.length;
console.log(`caps: driver=${caps.driver} params=${nparams} (expect ${EXPECTED_IDS}) unsupported=${caps.unsupported.length}`);
if (nparams !== EXPECTED_IDS) throw new Error("catalogue size mismatch");
for (const p of caps.params) {
  for (const k of ["id", "kind", "scope", "scopes", "min", "max", "unit", "default", "label"]) {
    if (!(k in p)) throw new Error(`caps entry ${p.id} misses ${k}`);
  }
}
topo = caps.topology;
console.log(`topology: ${topo.length} families (expect ${EXPECTED_FAMILIES.length})`);
if (topo.length !== EXPECTED_FAMILIES.length) throw new Error("topology family count mismatch");
for (const [key, kind, first, count] of EXPECTED_FAMILIES) {
  const f = topo.find((t) => t.key === key);
  if (!f || f.kind !== kind || f.first !== first || f.count !== count)
    throw new Error(`topology family ${key} mismatch: ${JSON.stringify(f)}`);
}
const total = topo.reduce((a, f) => a + f.count, 0);
console.log(`topology strips total=${total} (expect 77, indices 0..39 stable)`);
if (total !== 77) throw new Error("strip total mismatch");
console.log(`meter_taps=${JSON.stringify(caps.meter_taps)}`);
const byId = new Map(caps.params.map((p) => [p.id, p]));
scopedCount = (kind) => caps.params.filter((p) => p.scopes.includes(kind)).length;

// ---- 2. flood test FIRST (cache is still empty, so focus re-reads
// broadcast diffs; after a full sweep this would go quiet) ----
frames.length = 0;
for (let i = 0; i < 30; i++) send({ cmd: "focus", ch: i % 32 });
send({ cmd: "focus", ch: 40 });
const floodT = Date.now();
const first = await nextFrame(frames, (f) => f.type === "params" && f.ch === 40, 3000, "first ch40 params after flood");
console.log(`flood: first ch40 params after ${Date.now() - floodT}ms (budget 3000ms)`);
const want40 = scopedCount("in");
const seen40 = new Set(Object.keys(first.values));
{
  const deadline = Date.now() + 30000;
  let idx = frames.indexOf(first) + 1;
  while (seen40.size < want40) {
    if (Date.now() > deadline) throw new Error(`ch40 stalled at ${seen40.size}/${want40}`);
    let advanced = false;
    for (; idx < frames.length; idx++) {
      const x = frames[idx];
      if (x.type === "params" && x.ch === 40) {
        for (const k of Object.keys(x.values)) seen40.add(k);
        advanced = true;
        if (seen40.size >= want40) break;
      }
    }
    if (!advanced) await new Promise((r) => setTimeout(r, 10));
  }
}
console.log(`flood: ch40 full set (${want40} ids) in ${Date.now() - floodT}ms`);
// No flooded strip completed first: track completion ORDER — ch40 must be
// the first strip to reach its full scoped set.
{
  const per = new Map();
  const doneOrder = [];
  for (const x of frames) {
    if (x.type !== "params") continue;
    let s = per.get(x.ch);
    if (!s) per.set(x.ch, (s = new Set()));
    const before = s.size;
    for (const k of Object.keys(x.values)) s.add(k);
    const full = x.ch === 40 ? want40 : x.ch >= 32 && x.ch < 40 ? scopedCount("bus") : scopedCount("in");
    if (before < full && s.size >= full) doneOrder.push(x.ch);
  }
  console.log(`completion order: ${JSON.stringify(doneOrder.slice(0, 5))}`);
  if (doneOrder[0] !== 40) throw new Error(`ch40 was not first to complete: ${JSON.stringify(doneOrder.slice(0, 5))}`);
}
if (seen40.size !== want40) throw new Error("ch40 set incomplete");

ws.close();
} finally {
  relay.kill("SIGKILL");
}
await new Promise((r) => setTimeout(r, 500));

// ---- 3. per-family focus coverage (fresh relay: empty cache, so every
// focus re-read broadcasts its diff) ----
const relay2 = await startRelay();
try {
const { ws, frames } = await connect();
const send = (o) => ws.send(JSON.stringify(o));
{
const caps2 = await nextFrame(frames, (f) => f.type === "caps", 5000, "caps-2");
if (caps2.params.length !== EXPECTED_IDS) throw new Error("relay2 caps mismatch");
}
for (const f of topo) {
  const want = scopedCount(f.kind);
  const t = Date.now();
  send({ cmd: "focus", ch: f.first });
  const seen = new Set();
  const deadline = Date.now() + 30000;
  while (seen.size < want) {
    if (Date.now() > deadline) throw new Error(`family ${f.key} stalled at ${seen.size}/${want}`);
    const fr = await nextFrame(
      frames, (x) => x.type === "params" && x.ch === f.first && Object.keys(x.values).some((k) => !seen.has(k)),
      30000, `params ch${f.first}`,
    );
    for (const k of Object.keys(fr.values)) seen.add(k);
  }
  console.log(`family ${f.key} (${f.kind} x${f.count}): ${want} ids on strip ${f.first} in ${Date.now() - t}ms`);
}
console.log("per-family coverage OK (writes read back: see Rust sim tests)");

// ---- 4. driver comparison (unit-tested in Rust; numbers repeated here) ----
console.log("caps comparison (Rust-asserted):");
console.log(`  sim   : 149 ids (full catalogue) / 77 strips (8 families)`);
console.log(`  xair  : 103 ids (honest subset) / 35 strips (16 in + 4 ret + 6 bus + 4 fxsnd + LR + 4 DCA)`);
console.log(`  dl32s : 3 ids (fader/mute/name) / 32 strips (inputs only)`);
console.log(`  xair unsupported (46): delay_on/delay_ms, hpf_slope, source, eq5_* (4), aux7/8_* (4), legacy send mutes (10), send11..16_* (24)`);

ws.close();
console.log("V2-VERIFY PASS");
} finally {
  relay2.kill("SIGKILL");
}
