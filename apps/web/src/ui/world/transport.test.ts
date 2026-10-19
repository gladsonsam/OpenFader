// W2a transport tests. Run: node --experimental-strip-types transport.test.ts

import {
  ReplayTransport,
  TauriTransport,
  WsTransport,
  wsUrlFromQueryString,
  type WsSocket,
} from './transport.ts';

let passed = 0;
let failed = 0;
const failures: string[] = [];
function ok(cond: boolean, name: string, extra?: string): void {
  if (cond) passed++;
  else {
    failed++;
    failures.push(name + (extra ? ' :: ' + extra : ''));
  }
}

// ---- URL rule (mirrors lib/backend.ts wsUrlFromQuery) ----

ok(wsUrlFromQueryString('?server=127.0.0.1:8080', 'http:', 'x', 'x') === 'ws://127.0.0.1:8080/ws', 'bare host:port http');
ok(wsUrlFromQueryString('?server=127.0.0.1:8080', 'https:', 'x', 'x') === 'wss://127.0.0.1:8080/ws', 'bare host:port https');
ok(wsUrlFromQueryString('?server=ws://a:1/ws', 'http:', 'x', 'x') === 'ws://a:1/ws', 'ws url passthrough');
ok(wsUrlFromQueryString('?server=wss://a/ws', 'http:', 'x', 'x') === 'wss://a/ws', 'wss passthrough');
ok(wsUrlFromQueryString('?server=http://a:8080', 'http:', 'x', 'x') === 'ws://a:8080', 'http url -> ws');
ok(wsUrlFromQueryString('?server=https://a:8080', 'http:', 'x', 'x') === 'wss://a:8080', 'https url -> wss');
ok(wsUrlFromQueryString('?server=a:1/with/path', 'http:', 'x', 'x') === 'ws://a:1/with/path', 'path keeps no /ws append');
ok(wsUrlFromQueryString('', 'http:', 'lan:8080', 'lan') === 'ws://lan:8080/ws', 'same-origin fallback');
ok(wsUrlFromQueryString('', 'https:', 'lan', 'lan') === 'wss://lan/ws', 'same-origin https');
ok(wsUrlFromQueryString('', 'http:', '', '') === 'ws://127.0.0.1:8080', 'no-host fallback');

// ---- WsTransport with a fake socket ----

class FakeSocket implements WsSocket {
  binaryType = '';
  readyState = 0;
  sent: string[] = [];
  closed = false;
  readonly url: string;
  onopen: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  constructor(url: string) {
    this.url = url;
  }
  send(d: string): void {
    this.sent.push(d);
  }
  close(): void {
    this.closed = true;
  }
}

interface Timer {
  fn: () => void;
  ms: number;
}
function manualTimers(timers: Timer[]): {
  setTimer: (fn: () => void, ms: number) => unknown;
  clearTimer: (t: unknown) => void;
  run: () => void;
} {
  return {
    setTimer: (fn, ms) => {
      const t: Timer = { fn, ms };
      timers.push(t);
      return t;
    },
    clearTimer: (t) => {
      const i = timers.indexOf(t as Timer);
      if (i >= 0) timers.splice(i, 1);
    },
    run: () => {
      for (const t of timers.splice(0)) t.fn();
    },
  };
}

{
  const timers: Timer[] = [];
  const tm = manualTimers(timers);
  const sockets: FakeSocket[] = [];
  const tr = new WsTransport('ws://t/ws', {
    createSocket: (u) => {
      const s = new FakeSocket(u);
      sockets.push(s);
      return s;
    },
    backoffMs: [50],
    ...tm,
  });
  const statuses: boolean[] = [];
  tr.onStatus((v) => statuses.push(v));
  const msgs: unknown[] = [];
  tr.onMessage((m) => msgs.push(m));
  tr.connect();
  ok(sockets.length === 1, 'ws opens on connect');
  // Pre-open send queues.
  tr.send('{"cmd":"fader","ch":0,"value":0.5}');
  ok(sockets[0]!.sent.length === 0, 'queued while closed (fake never opened)');
  // Open the socket: queue flushes, then hello.
  sockets[0]!.readyState = 1;
  sockets[0]!.onopen?.({});
  const hello = JSON.parse(sockets[0]!.sent[sockets[0]!.sent.length - 1]!);
  ok(sockets[0]!.sent[0] === '{"cmd":"fader","ch":0,"value":0.5}', 'queue flushed first');
  ok(hello.cmd === 'hello' && hello.binary_meters === true, 'hello binary_meters on connect');
  ok(statuses[statuses.length - 1] === true, 'online on open');
  // Incoming text + binary.
  sockets[0]!.onmessage?.({ data: '{"type":"you","id":"c1"}' });
  sockets[0]!.onmessage?.({ data: new Uint8Array([1, 1, 0, 0]).buffer });
  ok(msgs.length === 2, 'messages delivered');
  // Drop: offline + reconnect scheduled; hello re-sent on re-open.
  sockets[0]!.onclose?.({});
  ok(statuses[statuses.length - 1] === false, 'offline on close');
  ok(timers.length === 1, 'reconnect scheduled');
  tm.run();
  ok(sockets.length === 2, 'reconnect opens new socket');
  sockets[1]!.readyState = 1;
  sockets[1]!.onopen?.({});
  const hello2 = JSON.parse(sockets[1]!.sent[sockets[1]!.sent.length - 1]!);
  ok(hello2.cmd === 'hello', 'hello re-sent on reconnect');
  tr.close();
  ok(sockets[1]!.closed, 'close closes socket');
  ok(timers.length === 0, 'close clears retry');
}

// ---- ReplayTransport ----

{
  const r = new ReplayTransport();
  const got: unknown[] = [];
  r.onMessage((m) => got.push(m));
  r.connect();
  r.feedText('{"type":"you","id":"c9"}');
  ok(got.length === 1 && got[0] === '{"type":"you","id":"c9"}', 'replay text');
  r.feedBinary(new Uint8Array([1, 1, 0, 0]).buffer);
  ok(got.length === 2 && got[1] instanceof ArrayBuffer, 'replay binary');
  r.send('{"cmd":"focus","ch":3}');
  ok(r.lastSent()?.['cmd'] === 'focus', 'replay records sent');
  const st: boolean[] = [];
  r.onStatus((v) => st.push(v));
  r.setOnline(false);
  ok(st[st.length - 1] === false, 'replay status');
}

// ---- TauriTransport (embedded engine IPC; invoke is injected) ----

{
  // Constructing + wiring callbacks never throws, with or without a shell.
  const seen: string[] = [];
  const t = new TauriTransport({
    invoke: async (cmd) => {
      seen.push(cmd);
      return [];
    },
    pollMs: 100000,
    setTimer: () => null,
    clearTimer: () => {},
  });
  ok(t.url === 'tauri://engine', 'tauri url');
  let threw = false;
  try {
    t.connect();
    t.send('{"cmd":"hello"}');
    t.onMessage(() => {});
    t.onStatus(() => {});
    t.close();
  } catch {
    threw = true;
  }
  ok(!threw, 'tauri methods do not throw without a shell');
}

console.log(`transport: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures) console.log('FAIL', f);
  process.exit(1);
}
