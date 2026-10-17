// openmix web WS end-to-end smoke test (Node 22+, no deps — global WebSocket).
//
// Against a RUNNING openmix-serve (default ws://localhost:8080/ws,
// overridable via OPENMIX_WS). Does NOT start the server.
//
// Flow: connect sim -> expect state -> fader ch0 0.5 -> expect state echo ->
// mute ch0 -> param gain -> capture scene -> move fader -> recall fade_ms 0
// (snap-back) -> delete scene -> one spectrum msg (64 bins) -> list_shows.
//
// Usage:  npm run smoke   (or: node e2e-smoke.mjs)
// Env:    OPENMIX_WS=ws://host:8080/ws
//
// Every step asserts; first mismatch prints `FAIL [<step>]` and exits 1.
// Success prints a PASS line per step and exits 0.

const URL = process.env.OPENMIX_WS || "ws://localhost:8080/ws";
const TIMEOUT_MS = 5000;
const SHOW = "smoke";
const SCENE = `e2e-${Date.now()}`;

const EPS = 1e-6;
const approx = (a, b) => Math.abs(a - b) < EPS;

function pass(step, detail) {
  console.log(`PASS [${step}]${detail ? ` ${detail}` : ""}`);
}

function fail(step, detail) {
  console.error(`FAIL [${step}] ${detail}`);
  process.exit(1);
}

// --- minimal WS client with a queued inbox + typed waiters ----------------

const inbox = [];
let opened = false;
let openError = null;
let closed = false;

const ws = new WebSocket(URL);

ws.addEventListener("open", () => {
  opened = true;
});
ws.addEventListener("message", (ev) => {
  const raw = typeof ev.data === "string" ? ev.data : String(ev.data);
  let msg;
  try {
    msg = JSON.parse(raw);
  } catch {
    return; // ignore non-JSON frames
  }
  inbox.push(msg);
});
ws.addEventListener("error", (ev) => {
  if (!opened) openError = ev?.message || "connection error";
});
ws.addEventListener("close", () => {
  closed = true;
});

function send(obj) {
  ws.send(JSON.stringify(obj));
}

function takeWhere(pred) {
  const i = inbox.findIndex(pred);
  if (i === -1) return null;
  return inbox.splice(i, 1)[0];
}

/** Resolve with the next inbox message matching pred (skipping others). */
function waitFor(step, pred, timeoutMs = TIMEOUT_MS) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + timeoutMs;
    const poll = () => {
      const hit = takeWhere(pred);
      if (hit) return resolve(hit);
      if (Date.now() > deadline) {
        const types = inbox
          .slice(-5)
          .map((m) => m?.type ?? "?")
          .join(",");
        return reject(
          new Error(`timed out after ${timeoutMs}ms (recent inbox types: ${types || "empty"})`),
        );
      }
      if (closed) return reject(new Error("connection closed while waiting"));
      setTimeout(poll, 25);
    };
    poll();
  });
}

const isType = (t) => (m) => m && m.type === t;

function stripOf(state, ch) {
  const strips = state?.strips;
  if (!Array.isArray(strips)) return null;
  return strips.find((s) => s && s.ch === ch) ?? null;
}

function findScene(shows, show, scene) {
  const lib = shows?.shows;
  if (!Array.isArray(lib)) return null;
  const slot = lib.find((s) => s && s.name === show);
  const scenes = slot?.scenes;
  if (!Array.isArray(scenes)) return null;
  return scenes.find((s) => s && s.name === scene) ?? null;
}

async function waitState(step, wantFn) {
  try {
    return await waitFor(step, (m) => m?.type === "state" && wantFn(m));
  } catch (e) {
    fail(step, e.message);
  }
}

async function waitShows(step, wantFn) {
  try {
    return await waitFor(step, (m) => m?.type === "shows" && wantFn(m));
  } catch (e) {
    fail(step, e.message);
  }
}

// --- main ------------------------------------------------------------------

async function main() {
  // 1. open: fail clearly when no server is listening.
  await new Promise((resolve) => {
    if (opened) return resolve();
    const t = setInterval(() => {
      if (opened || openError || closed) {
        clearInterval(t);
        resolve();
      }
    }, 25);
    setTimeout(() => {
      clearInterval(t);
      resolve();
    }, TIMEOUT_MS);
  });
  if (!opened) {
    fail(
      "open",
      `cannot reach ${URL} (${openError || "connection failed/closed"}). ` +
        `Is openmix-serve running? Start it first, or set OPENMIX_WS.`,
    );
  }
  pass("open", `connected to ${URL}`);

  // The server sends state + shows immediately on TCP accept; drain them.
  try {
    await waitFor("hello", isType("state"));
  } catch (e) {
    fail("hello", `no initial state snapshot: ${e.message}`);
  }
  try {
    await waitFor("hello", isType("shows"));
  } catch (e) {
    fail("hello", `no initial shows snapshot: ${e.message}`);
  }
  pass("hello", "initial state+shows snapshots");

  // 2. connect sim -> expect state.
  send({ cmd: "connect", driver: "sim" });
  const st0 = await waitState(
    "connect",
    (s) => s.driver === "sim" && Array.isArray(s.strips) && s.strips.length > 0,
  );
  pass("connect", `driver=sim strips=${st0.strips.length}`);

  // 3. fader ch0 0.5 -> expect state echo.
  send({ cmd: "fader", ch: 0, value: 0.5 });
  await waitState(
    "fader",
    (s) => {
      const f = stripOf(s, 0)?.fader;
      return typeof f === "number" && approx(f, 0.5);
    },
  );
  pass("fader", "ch0=0.5 echoed in state");

  // 4. mute ch0 on -> expect state echo (then restore off to keep sim clean).
  send({ cmd: "mute", ch: 0, muted: true });
  await waitState("mute", (s) => stripOf(s, 0)?.muted === true);
  pass("mute", "ch0 muted=true echoed in state");
  send({ cmd: "mute", ch: 0, muted: false });
  await waitState("unmute", (s) => stripOf(s, 0)?.muted === false);
  pass("unmute", "ch0 muted=false restored");

  // 5. param gain: sim may not implement it — accept a state echo (supported)
  //    or an error frame (unsupported) as long as the connection stays alive.
  send({ cmd: "param", ch: 0, field: "gain", value: 0.5 });
  let paramOutcome;
  try {
    paramOutcome = await waitFor(
      "param",
      (m) => m?.type === "state" || m?.type === "error",
    );
  } catch (e) {
    fail("param", `no reply to param gain: ${e.message}`);
  }
  if (paramOutcome.type === "state") {
    pass("param", "gain accepted, state echoed");
  } else {
    pass("param", `gain not supported (${paramOutcome.message}), connection alive`);
  }

  // 6. capture scene -> expect shows library to contain it with ch0=0.5.
  send({ cmd: "capture_scene", show: SHOW, name: SCENE });
  const captured = await waitShows(
    "capture",
    (s) => findScene(s, SHOW, SCENE) != null,
  );
  const capStrip = findScene(captured, SHOW, SCENE)?.strips?.find((s) => s?.ch === 0);
  const capFader = capStrip?.fader;
  if (typeof capFader !== "number" || !approx(capFader, 0.5)) {
    fail("capture", `scene ch0 fader = ${capFader}, expected 0.5`);
  }
  pass("capture", `${SHOW}/${SCENE} captured ch0=${capFader}`);

  // 7. move fader away -> expect state echo.
  const moved = Math.min(1, capFader + 0.3);
  if (Math.abs(moved - capFader) < 0.05) fail("move", "moved value too close to captured");
  send({ cmd: "fader", ch: 0, value: moved });
  await waitState(
    "move",
    (s) => {
      const f = stripOf(s, 0)?.fader;
      return typeof f === "number" && approx(f, moved);
    },
  );
  pass("move", `ch0=${moved}`);

  // 8. recall fade_ms 0 -> expect snap-back to captured value.
  send({ cmd: "recall_scene", show: SHOW, name: SCENE, fade_ms: 0 });
  await waitState(
    "recall",
    (s) => {
      const f = stripOf(s, 0)?.fader;
      return typeof f === "number" && approx(f, capFader);
    },
  );
  pass("recall", `ch0 snapped back to ${capFader} (fade_ms=0)`);

  // 9. delete scene -> expect shows library without it.
  send({ cmd: "delete_scene", show: SHOW, name: SCENE });
  await waitShows("delete", (s) => findScene(s, SHOW, SCENE) == null);
  pass("delete", `${SHOW}/${SCENE} removed from library`);

  // 10. one spectrum message, 64 normalized bins.
  let spec;
  try {
    spec = await waitFor("spectrum", isType("spectrum"));
  } catch (e) {
    fail("spectrum", `no spectrum frame: ${e.message}`);
  }
  const bins = spec.bins;
  if (!Array.isArray(bins) || bins.length !== 64) {
    fail("spectrum", `bins length = ${Array.isArray(bins) ? bins.length : typeof bins}, expected 64`);
  }
  for (const b of bins) {
    if (typeof b !== "number" || !(b >= 0 && b <= 1)) {
      fail("spectrum", `bin out of [0,1]: ${b}`);
    }
  }
  pass("spectrum", `ch=${spec.ch} bins=64 normalized`);

  // 11. list_shows -> expect direct shows reply (deleted scene stays gone).
  send({ cmd: "list_shows" });
  const listed = await waitShows("list_shows", () => true);
  if (!Array.isArray(listed.shows)) fail("list_shows", "reply has no shows array");
  if (findScene(listed, SHOW, SCENE) != null) {
    fail("list_shows", "deleted scene reappeared in list_shows");
  }
  pass("list_shows", `library ok (${listed.shows.length} show(s))`);

  ws.close();
  console.log("SMOKE OK");
}

main().catch((e) => fail("unhandled", e?.stack || String(e)));
