// openmix web backend interface — two transports, same methods.
//
//   connect(driver, addr)  -> Promise<string>  ("connected via sim")
//   setFader(ch, v01)      -> Promise<void>     (v01 in 0..1)
//   setMute(ch, muted)     -> Promise<void>
//   setParam(ch, field, v) -> Promise<void>     (number|boolean|string)
//   onState(cb)            -> unsub             (cb(strips [{ch,name,fader,muted}]))
//   onMeters(cb)           -> unsub             (cb(levels [0..1]))
//
// Plus optional onStatus(cb) -> unsub (cb(true|false) online flag).
// Auto-select: Tauri IPC when running inside Tauri, else WebSocket relay.
//
// WS protocol (port :8080): host -> {cmd:'connect',driver,addr},
// {cmd:'fader',ch,value}, {cmd:'mute',ch,muted}, {cmd:'param',ch,field,value};
// relay -> {type:'state',strips:[...]}, {type:'meters',levels:[...]}.
// Ported from ../../ui/api.js — keep the two in sync contract-wise.

function sub(list, cb) {
  list.add(cb);
  return () => list.delete(cb);
}

function normStrip(s, i) {
  const m = s.muted ?? s.mute ?? false;
  return {
    ch: s.ch ?? i,
    name: String(s.name ?? `CH${String((s.ch ?? i) + 1).padStart(2, '0')}`),
    fader: Number(s.fader ?? 0.75),
    muted: !!m,
  };
}

/* ---------------- Tauri (desktop/mobile webview) ---------------- */

export class TauriBackend {
  constructor() {
    this.kind = 'tauri';
    this.online = false;
    this.stateCbs = new Set();
    this.meterCbs = new Set();
    this.statusCbs = new Set();
    this.strips = Array.from({ length: 8 }, (_, i) => ({
      ch: i, name: `CH${String(i + 1).padStart(2, '0')}`, fader: 0.75, muted: false,
    }));
    this.lastLevels = this.strips.map(() => 0);
    this.pollTimer = null;
    this.invoke = window.__TAURI__.core.invoke.bind(window.__TAURI__.core);
  }

  setOnline(v) {
    if (this.online === v) return;
    this.online = v;
    this.statusCbs.forEach((cb) => { try { cb(v); } catch { /* ignore */ } });
  }

  emitState() {
    const snap = this.strips.map((s) => ({ ...s }));
    this.stateCbs.forEach((cb) => { try { cb(snap); } catch { /* ignore */ } });
  }

  async connect(driver, addr) {
    const msg = await this.invoke('connect', { driver, addr: addr || '' });
    this.setOnline(true);
    this.emitState();
    return String(msg);
  }

  async setFader(ch, v01) {
    await this.invoke('set_fader', { ch, value: v01 });
    if (this.strips[ch]) { this.strips[ch].fader = v01; this.emitState(); }
  }

  async setMute(ch, muted) {
    await this.invoke('set_mute', { ch, muted });
    if (this.strips[ch]) { this.strips[ch].muted = muted; this.emitState(); }
  }

  // No `set_param` command in the Tauri shell yet: the invoke rejects and
  // the detail panel hides that control (unsupported-gracefully).
  async setParam(ch, field, v) {
    await this.invoke('set_param', { ch, field, value: v });
  }

  onState(cb) {
    const un = sub(this.stateCbs, cb);
    try { cb(this.strips.map((s) => ({ ...s }))); } catch { /* ignore */ }
    return un;
  }

  onMeters(cb) {
    const un = sub(this.meterCbs, cb);
    if (!this.pollTimer) {
      this.pollTimer = setInterval(async () => {
        if (!this.meterCbs.size) return;
        try {
          const levels = await this.invoke('get_meters');
          this.lastLevels = levels;
          this.meterCbs.forEach((f) => { try { f(levels); } catch { /* ignore */ } });
        } catch { /* backend away: keep last frame */ }
      }, 120);
    }
    try { cb(this.lastLevels); } catch { /* ignore */ }
    return un;
  }

  onStatus(cb) {
    const un = sub(this.statusCbs, cb);
    try { cb(this.online); } catch { /* ignore */ }
    return un;
  }
}

/* ---------------- WebSocket relay (any browser) ---------------- */

export function wsUrlFromQuery(defHost) {
  const q = new URLSearchParams(location.search);
  const s = (q.get('server') || '').trim();
  if (/^wss?:\/\//i.test(s)) return s;
  if (s) return `ws://${s.replace(/^https?:\/\//, '')}`;
  const host = defHost || location.hostname || '127.0.0.1';
  return `ws://${host}:8080`;
}

export class WsBackend {
  constructor(url) {
    this.kind = 'ws';
    this.url = url || wsUrlFromQuery();
    this.online = false;
    this.stateCbs = new Set();
    this.meterCbs = new Set();
    this.statusCbs = new Set();
    this.ws = null;
    this.queue = [];
    this.driver = 'sim';
    this.addr = '';
    this.wantReconnect = true;
    this.reTimer = null;
  }

  setOnline(v) {
    if (this.online === v) return;
    this.online = v;
    this.statusCbs.forEach((cb) => { try { cb(v); } catch { /* ignore */ } });
  }

  ensure() {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return this.openPromise;
    }
    this.ws = new WebSocket(this.url);
    this.openPromise = new Promise((resolve, reject) => {
      const to = setTimeout(() => reject(new Error(`ws timeout ${this.url}`)), 5000);
      this.ws.onopen = () => {
        clearTimeout(to);
        this.setOnline(true);
        for (const m of this.queue.splice(0)) this.ws.send(m);
        if (this.driver) this.send({ cmd: 'connect', driver: this.driver, addr: this.addr });
        resolve(`connected via ws ${this.url}`);
      };
      this.ws.onerror = () => { /* onclose reports */ };
      this.ws.onclose = () => {
        clearTimeout(to);
        this.setOnline(false);
        this.ws = null;
        if (this.wantReconnect && !this.reTimer) {
          this.reTimer = setTimeout(() => { this.reTimer = null; this.ensure().catch(() => {}); }, 1500);
        }
      };
      this.ws.onmessage = (ev) => this.handle(ev.data);
    });
    return this.openPromise;
  }

  send(obj) {
    const msg = JSON.stringify(obj);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) this.ws.send(msg);
    else this.queue.push(msg);
  }

  handle(raw) {
    let m;
    try { m = JSON.parse(raw); } catch { return; }
    if (m.type === 'state' && Array.isArray(m.strips)) {
      const strips = m.strips.map(normStrip);
      this.stateCbs.forEach((cb) => { try { cb(strips); } catch { /* ignore */ } });
    } else if (m.type === 'meters' && Array.isArray(m.levels)) {
      this.meterCbs.forEach((cb) => { try { cb(m.levels); } catch { /* ignore */ } });
    }
  }

  async connect(driver, addr) {
    this.driver = driver;
    this.addr = addr || '';
    return this.ensure();
  }

  async setFader(ch, v01) {
    this.send({ cmd: 'fader', ch, value: v01 });
  }

  async setMute(ch, muted) {
    this.send({ cmd: 'mute', ch, muted });
  }

  async setParam(ch, field, v) {
    this.send({ cmd: 'param', ch, field, value: v });
  }

  onState(cb) { return sub(this.stateCbs, cb); }
  onMeters(cb) { return sub(this.meterCbs, cb); }
  onStatus(cb) {
    const un = sub(this.statusCbs, cb);
    try { cb(this.online); } catch { /* ignore */ }
    return un;
  }

  close() {
    this.wantReconnect = false;
    if (this.reTimer) clearTimeout(this.reTimer);
    if (this.ws) this.ws.close();
  }
}

/* ---------------- auto-select ---------------- */

export function hasTauri() {
  return !!(window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function');
}

export function selectBackend() {
  return hasTauri() ? new TauriBackend() : new WsBackend();
}
