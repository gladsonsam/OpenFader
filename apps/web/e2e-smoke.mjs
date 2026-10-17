// openmix web WS end-to-end smoke test (Node 22+, no deps — global WebSocket).
//
// Against a RUNNING openmix-serve (default ws://localhost:8080/ws,
// overridable via OPENMIX_WS). Does NOT start the server.
//
// Sections (each independent — a failure prints `FAIL [<section>]` and the
// run continues; exit code is non-zero when any section fails):
//   open/connect/fader/mute/param      — basic mixer mutations
//   main_fader                          — set/get via state + alias + range check
//   locks                               — lock→denied→unlock→allowed (2 connections)
//   shows                               — capture/recall roundtrip (+2nd-conn
//                                       visibility, param echo, unknown-scene error)
//   spectrum/gr                         — frame shape assertions
//   list_shows/error                    — library pull + bad-command liveness
//
// Usage:  npm run smoke   (or: node e2e-smoke.mjs)
// Env:    OPENMIX_WS=ws://host:8080/ws

const URL = process.env.OPENMIX_WS || "ws://localhost:8080/ws";
const TIMEOUT_MS = 5000;
const SHOW = "smoke";
const SCENE = `e2e-${Date.now()}`;

const EPS = 1e-6;
const approx = (a, b) => Math.abs(a - b) < EPS;

// --- minimal WS client with a queued inbox + typed waiters ----------------

class Client {
  constructor(url, label) {
    this.label = label || url;
    this.inbox = [];
    this.opened = false;
    this.openError = null;
    this.closed = false;
    this.ws = new WebSocket(url);
    this.ws.addEventListener("open", () => {
      this.opened = true;
    });
    this.ws.addEventListener("message", (ev) => {
      const raw = typeof ev.data === "string" ? ev.data : String(ev.data);
      let msg;
      try {
        msg = JSON.parse(raw);
      } catch {
        return; // ignore non-JSON frames
      }
      this.inbox.push(msg);
    });
    this.ws.addEventListener("error", (ev) => {
      if (!this.opened) this.openError = ev?.message || "connection error";
    });
    this.ws.addEventListener("close", () => {
      this.closed = true;
    });
  }

  waitOpen(timeoutMs = TIMEOUT_MS) {
    return new Promise((resolve) => {
      if (this.opened) return resolve(true);
      const t = setInterval(() => {
        if (this.opened || this.openError || this.closed) {
          clearInterval(t);
          resolve(this.opened);
        }
      }, 25);
      setTimeout(() => {
        clearInterval(t);
        resolve(this.opened);
      }, timeoutMs);
    });
  }

  send(obj) {
    this.ws.send(JSON.stringify(obj));
  }

  close() {
    try {
      this.ws.close();
    } catch {
      // already gone
    }
  }

  takeWhere(pred) {
    const i = this.inbox.findIndex(pred);
    if (i === -1) return null;
    return this.inbox.splice(i, 1)[0];
  }

  waitFor(pred, timeoutMs = TIMEOUT_MS) {
    const inbox = this.inbox;
    const isClosed = () => this.closed;
    return new Promise((resolve, reject) => {
      const deadline = Date.now() + timeoutMs;
      const poll = () => {
        const i = inbox.findIndex(pred);
        if (i !== -1) return resolve(inbox.splice(i, 1)[0]);
        if (Date.now() > deadline) {
          const types = inbox
            .slice(-5)
            .map((m) => m?.type ?? "?")
            .join(",");
          return reject(
            new Error(`timed out after ${timeoutMs}ms (recent inbox types: ${types || "empty"})`),
          );
        }
        if (isClosed()) return reject(new Error("connection closed while waiting"));
        setTimeout(poll, 25);
      };
      poll();
    });
  }

  waitState(wantFn, timeoutMs = TIMEOUT_MS) {
    return this.waitFor((m) => m?.type === "state" && wantFn(m), timeoutMs);
  }

  waitShows(wantFn, timeoutMs = TIMEOUT_MS) {
    return this.waitFor((m) => m?.type === "shows" && wantFn(m), timeoutMs);
  }

  waitLocks(wantFn, timeoutMs = TIMEOUT_MS) {
    return this.waitFor((m) => m?.type === "locks" && wantFn(m), timeoutMs);
  }
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

function holdsCh(locks, ch) {
  const h = locks?.holds?.[String(ch)];
  return typeof h === "string" ? h : null;
}

// --- section runner: independent, failures reported per section -----------

const results = [];
let primary = null;

async function section(name, fn) {
  try {
    const detail = await fn();
    console.log(`PASS [${name}]${detail ? ` ${detail}` : ""}`);
    results.push({ name, ok: true });
  } catch (e) {
    console.error(`FAIL [${name}] ${e?.stack || e?.message || String(e)}`);
    results.push({ name, ok: false, error: e });
  }
}

function need(client) {
  if (!client || !client.opened || client.closed) {
    throw new Error("primary connection not open (see [open] section)");
  }
  return client;
}

// --- main ------------------------------------------------------------------

async function main() {
  await section("open", async () => {
    primary = new Client(URL, "primary");
    const ok = await primary.waitOpen();
    if (!ok) {
      throw new Error(
        `cannot reach ${URL} (${primary.openError || "connection failed/closed"}). ` +
          `Is openmix-serve running? Start it first, or set OPENMIX_WS.`,
      );
    }
    return `connected to ${URL}`;
  });

  await section("hello", async () => {
    const c = need(primary);
    await c.waitFor(isType("state"));
    await c.waitFor(isType("shows"));
    return "initial state+shows snapshots";
  });

  await section("connect", async () => {
    const c = need(primary);
    c.send({ cmd: "connect", driver: "sim" });
    const st = await c.waitState(
      (s) => s.driver === "sim" && Array.isArray(s.strips) && s.strips.length > 0,
    );
    return `driver=sim strips=${st.strips.length}`;
  });

  await section("fader", async () => {
    const c = need(primary);
    c.send({ cmd: "fader", ch: 0, value: 0.5 });
    await c.waitState((s) => {
      const f = stripOf(s, 0)?.fader;
      return typeof f === "number" && approx(f, 0.5);
    });
    return "ch0=0.5 echoed in state";
  });

  await section("mute", async () => {
    const c = need(primary);
    c.send({ cmd: "mute", ch: 0, muted: true });
    await c.waitState((s) => stripOf(s, 0)?.muted === true);
    c.send({ cmd: "mute", ch: 0, muted: false });
    await c.waitState((s) => stripOf(s, 0)?.muted === false);
    return "ch0 muted=true then false echoed in state";
  });

  await section("param", async () => {
    const c = need(primary);
    // sim may not implement gain — accept a state echo (supported) or an
    // error frame (unsupported) as long as the connection stays alive.
    c.send({ cmd: "param", ch: 0, field: "gain", value: 0.5 });
    const outcome = await c.waitFor((m) => m?.type === "state" || m?.type === "error");
    if (outcome.type === "state") return "gain accepted, state echoed";
    return `gain not supported (${outcome.message}), connection alive`;
  });

  await section("main_fader", async () => {
    const c = need(primary);
    // set/get via state; the broadcast is writer-tagged like strips.
    c.send({ cmd: "main_fader", value: 0.55 });
    const st = await c.waitState(
      (s) => typeof s.main_fader === "number" && approx(s.main_fader, 0.55),
    );
    if (typeof st.by !== "string" || st.by.length === 0) {
      throw new Error("main_fader mutation state has no writer tag");
    }
    // set_main_fader alias (the Tauri command name) works the same.
    c.send({ cmd: "set_main_fader", value: 0.65 });
    await c.waitState((s) => typeof s.main_fader === "number" && approx(s.main_fader, 0.65));
    // out-of-range is rejected and the fader does not move: no state
    // follows a rejected command, so re-poll via an explicit reconnect
    // snapshot (connect re-emits state).
    c.send({ cmd: "main_fader", value: 1.5 });
    const err = await c.waitFor(isType("error"));
    if (!err.message) throw new Error("range error explains itself (no message)");
    c.send({ cmd: "connect", driver: "sim" });
    const re = await c.waitState(
      (s) => typeof s.main_fader === "number" && approx(s.main_fader, 0.65),
    );
    if (!approx(re.main_fader, 0.65)) throw new Error(`moved on reject: ${re.main_fader}`);
    // missing value errors too; connection stays alive.
    c.send({ cmd: "main_fader" });
    const err2 = await c.waitFor(isType("error"));
    if (!err2.message) throw new Error("missing-value error has no message");
    // restore the sim default so later sections see a known main.
    c.send({ cmd: "main_fader", value: 0.8 });
    await c.waitState((s) => typeof s.main_fader === "number" && approx(s.main_fader, 0.8));
    return "set/get 0.55→0.65 via state, 1.5 rejected, missing rejected, restored 0.8";
  });

  await section("locks", async () => {
    const a = need(primary);
    const CH = 29; // high strip, clear of other sections
    const b = new Client(`${URL}${URL.includes("?") ? "&" : "?"}name=smoke-bob`, "second");
    try {
      if (!(await b.waitOpen())) throw new Error("second connection failed to open");
      // drain the 4-frame snapshot (state/shows/clients/locks)
      await b.waitFor(isType("state"));
      await b.waitFor(isType("shows"));
      await b.waitFor(isType("locks"));
      // A locks; both sides observe holds {"29": holder}.
      a.send({ cmd: "lock", ch: CH });
      const l1 = await a.waitLocks((v) => holdsCh(v, CH) != null);
      const holder = holdsCh(l1, CH);
      const l2 = await b.waitLocks((v) => holdsCh(v, CH) === holder);
      if (holdsCh(l2, CH) !== holder) throw new Error("second client sees different holder");
      // locked strip rejects other writers' fader/mute/param with locked_by.
      const denied = async (cmd) => {
        b.send(cmd);
        const e = await b.waitFor(
          (m) => m?.type === "error" && m?.locked_by === holder,
        );
        if (e.locked_by !== holder) throw new Error(`wrong locked_by: ${e.locked_by}`);
      };
      await denied({ cmd: "fader", ch: CH, value: 0.9 });
      await denied({ cmd: "mute", ch: CH, muted: true });
      await denied({ cmd: "param", ch: CH, field: "gain", value: 0.5 });
      // contending lock + foreign unlock are denied the same way.
      await denied({ cmd: "lock", ch: CH });
      await denied({ cmd: "unlock", ch: CH });
      // holder writes through on the pinned strip; sibling sees it.
      a.send({ cmd: "fader", ch: CH, value: 0.6 });
      await b.waitState((s) => {
        const f = stripOf(s, CH)?.fader;
        return typeof f === "number" && approx(f, 0.6);
      });
      // holder unlocks; both sides see the empty table; B writes freely.
      a.send({ cmd: "unlock", ch: CH });
      await a.waitLocks((v) => holdsCh(v, CH) == null);
      await b.waitLocks((v) => holdsCh(v, CH) == null);
      b.send({ cmd: "fader", ch: CH, value: 0.1 });
      await a.waitState((s) => {
        const f = stripOf(s, CH)?.fader;
        return typeof f === "number" && approx(f, 0.1);
      });
      return `ch${CH} lock→denied(locked_by=${holder})→unlock→allowed`;
    } finally {
      b.close();
    }
  });

  await section("shows", async () => {
    const c = need(primary);
    const CH = 2; // clear of fader/mute (ch0) and locks (ch29)
    // pin a known value, then capture.
    c.send({ cmd: "fader", ch: CH, value: 0.5 });
    await c.waitState((s) => {
      const f = stripOf(s, CH)?.fader;
      return typeof f === "number" && approx(f, 0.5);
    });
    c.send({ cmd: "capture_scene", show: SHOW, name: SCENE });
    const captured = await c.waitShows((s) => findScene(s, SHOW, SCENE) != null);
    const scene = findScene(captured, SHOW, SCENE);
    const capStrip = scene?.strips?.find((s) => s?.ch === CH);
    if (typeof capStrip?.fader !== "number" || !approx(capStrip.fader, 0.5)) {
      throw new Error(`scene ch${CH} fader = ${capStrip?.fader}, expected 0.5`);
    }
    if (!("main_fader" in scene)) {
      throw new Error("captured scene has no main_fader field");
    }
    // library survives a fresh connection (server-side, no localStorage).
    const peer = new Client(URL, "shows-peer");
    try {
      if (!(await peer.waitOpen())) throw new Error("peer connection failed to open");
      await peer.waitFor(isType("state"));
      const peerShows = await peer.waitFor(isType("shows"));
      if (findScene(peerShows, SHOW, SCENE) == null) {
        throw new Error("second connection does not see the server-side library");
      }
    } finally {
      peer.close();
    }
    // move away, then recall fade_ms 0: expect the state snap-back AND
    // the per-param echo so sibling UIs catch up.
    const moved = 0.8;
    c.send({ cmd: "fader", ch: CH, value: moved });
    await c.waitState((s) => {
      const f = stripOf(s, CH)?.fader;
      return typeof f === "number" && approx(f, moved);
    });
    c.send({ cmd: "recall_scene", show: SHOW, name: SCENE, fade_ms: 0 });
    await c.waitState((s) => {
      const f = stripOf(s, CH)?.fader;
      return typeof f === "number" && approx(f, 0.5);
    });
    const echo = await c.waitFor(
      (m) =>
        m?.type === "param" && m.ch === CH && m.field === "fader" && approx(m.value, 0.5),
    );
    if (!approx(echo.value, 0.5)) throw new Error(`recall param echo = ${echo.value}`);
    // unknown scene errors; connection stays alive.
    c.send({ cmd: "recall_scene", show: SHOW, name: `${SCENE}-nope`, fade_ms: 0 });
    const err = await c.waitFor(isType("error"));
    if (!err.message) throw new Error("unknown-scene error has no message");
    // delete; the scene stays gone.
    c.send({ cmd: "delete_scene", show: SHOW, name: SCENE });
    await c.waitShows((s) => findScene(s, SHOW, SCENE) == null);
    return `${SHOW}/${SCENE} capture→recall(echo)→delete, visible to 2nd conn`;
  });

  await section("spectrum", async () => {
    const c = need(primary);
    const spec = await c.waitFor(isType("spectrum"));
    if (!Number.isInteger(spec.ch) || spec.ch < 0 || spec.ch > 15) {
      throw new Error(`spectrum ch = ${spec.ch}, expected 0..15 (first 16 channels)`);
    }
    const bins = spec.bins;
    if (!Array.isArray(bins) || bins.length !== 64) {
      throw new Error(`bins length = ${Array.isArray(bins) ? bins.length : typeof bins}, expected 64`);
    }
    for (const x of bins) {
      if (typeof x !== "number" || !(x >= 0 && x <= 1)) {
        throw new Error(`bin out of [0,1]: ${x}`);
      }
    }
    return `ch=${spec.ch} bins=64 normalized`;
  });

  await section("gr", async () => {
    const c = need(primary);
    const frame = await c.waitFor(isType("gr"));
    if (!Number.isInteger(frame.ch) || frame.ch < 0 || frame.ch > 15) {
      throw new Error(`gr ch = ${frame.ch}, expected 0..15 (first 16 channels)`);
    }
    for (const key of ["gate", "comp"]) {
      const v = frame[key];
      if (typeof v !== "number" || !(v >= 0)) {
        throw new Error(`gr ${key} = ${v}, expected dB reduction >= 0`);
      }
    }
    return `ch=${frame.ch} gate=${frame.gate} comp=${frame.comp}`;
  });

  await section("list_shows", async () => {
    const c = need(primary);
    c.send({ cmd: "list_shows" });
    const listed = await c.waitShows(() => true);
    if (!Array.isArray(listed.shows)) throw new Error("reply has no shows array");
    if (findScene(listed, SHOW, SCENE) != null) {
      throw new Error("deleted scene reappeared in list_shows");
    }
    return `library ok (${listed.shows.length} show(s))`;
  });

  await section("error", async () => {
    const c = need(primary);
    c.send({ cmd: "bogus" });
    const err = await c.waitFor(isType("error"));
    if (!err.message) throw new Error("bad-command error has no message");
    // connection stays alive: a benign command still gets a reply.
    c.send({ cmd: "list_shows" });
    await c.waitShows(() => true);
    return "bogus cmd errored, connection alive";
  });

  try {
    primary?.close();
  } catch {
    // ignore
  }

  const failed = results.filter((r) => !r.ok);
  console.log(
    `\n${results.length - failed.length}/${results.length} sections passed` +
      (failed.length ? `; failed: ${failed.map((r) => r.name).join(", ")}` : ""),
  );
  if (failed.length > 0) {
    console.error("SMOKE FAIL");
    process.exit(1);
  }
  console.log("SMOKE OK");
}

main().catch((e) => {
  console.error(`FAIL [unhandled] ${e?.stack || String(e)}`);
  process.exit(1);
});
