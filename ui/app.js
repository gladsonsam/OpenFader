import { selectBackend } from './api.js';

const $ = (id) => document.getElementById(id);
const backend = selectBackend();
$('via').textContent = backend.kind === 'tauri' ? 'tauri ipc' : `ws ${backend.url}`;

/* ---------- state ---------- */
let strips = Array.from({ length: 8 }, (_, i) => ({
  ch: i, name: `CH${String(i + 1).padStart(2, '0')}`, fader: 0.75, muted: false,
}));
let levels = strips.map(() => 0);
let layer = 'inputs';
let selected = 0;
let solos = new Set();
let unsupported = new Set(); // param fields the backend rejected
let storeArm = false;
let master = { fader: 0.8 }; // local monitor-dim until backend exposes main
let lastMeterAt = 0;

const status = (t) => { $('status').textContent = t || ''; };

/* ---------- helpers ---------- */
function faderDb(v) {
  if (v <= 0.0005) return '-inf';
  return `${(20 * Math.log10(v) >= 0 ? '+' : '') + (20 * Math.log10(v)).toFixed(1)}`;
}
const isBus = (s) => /^bus/i.test(s.name);
function visibleStrips() {
  if (layer === 'mains') return [{ ch: -1, name: 'MAIN', fader: master.fader, muted: false, main: true }];
  const buses = strips.filter(isBus);
  const inputs = strips.filter((s) => !isBus(s));
  if (layer === 'buses') return buses.length ? buses : inputs.slice(-8);
  if (buses.length) return inputs;
  if (layer === 'buses') return []; // <16ch backend: no buses yet
  return strips;
}

/* ---------- strips ---------- */
const stripEls = new Map(); // ch -> {root, fill, handle, db, muteBtn, soloBtn, cv}

function buildStrips() {
  const box = $('strips');
  box.innerHTML = '';
  stripEls.clear();
  const vis = visibleStrips();
  if (!vis.length) {
    box.innerHTML = `<div class="empty">${layer === 'buses' ? 'no buses on this backend yet' : 'no strips'}</div>`;
    return;
  }
  for (const s of vis) {
    const root = document.createElement('div');
    root.className = 'strip' + (s.ch === selected ? ' sel' : '') + (solos.size && !solos.has(s.ch) ? ' solo-dim' : '');
    root.innerHTML = `
      <div class="scribble" title="tap: select / detail">${s.name}<br><small>ch${s.ch < 0 ? 'M' : s.ch + 1}</small></div>
      <div class="mid"><canvas class="meter" width="16" height="190"></canvas>
      <div class="ftrack"><div class="ffill"></div><div class="fhandle"></div></div></div>
      <div class="db"></div>
      <div class="mrow"><button class="mute">M</button><button class="solo">S</button></div>`;
    const fill = root.querySelector('.ffill'), handle = root.querySelector('.fhandle');
    const db = root.querySelector('.db'), cv = root.querySelector('.meter');
    const muteBtn = root.querySelector('.mute'), soloBtn = root.querySelector('.solo');
    if (s.muted) muteBtn.classList.add('lit');
    if (solos.has(s.ch)) soloBtn.classList.add('lit');
    const paint = () => {
      fill.style.height = `${Math.round(s.fader * 100)}%`;
      handle.style.bottom = `calc(${Math.round(s.fader * 100)}% - 7px)`;
      db.textContent = faderDb(s.fader);
    };
    paint();
    const setFromEvent = (e) => {
      const r = root.querySelector('.ftrack').getBoundingClientRect();
      const v = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
      setFader(s, v);
    };
    const track = root.querySelector('.ftrack');
    track.addEventListener('pointerdown', (e) => {
      track.setPointerCapture(e.pointerId);
      select(s.ch);
      setFromEvent(e);
      const mv = (ev) => setFromEvent(ev);
      const up = () => {
        track.removeEventListener('pointermove', mv);
        track.removeEventListener('pointerup', up);
      };
      track.addEventListener('pointermove', mv);
      track.addEventListener('pointerup', up);
    });
    root.querySelector('.scribble').addEventListener('click', () => select(s.ch));
    muteBtn.addEventListener('click', (e) => { e.stopPropagation(); toggleMute(s); });
    soloBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      solos.has(s.ch) ? solos.delete(s.ch) : solos.add(s.ch);
      buildStrips();
    });
    box.appendChild(root);
    stripEls.set(s.ch, { root, fill, handle, db, muteBtn, cv, strip: s });
  }
}

function refreshStrips() {
  const vis = visibleStrips();
  if (vis.length !== stripEls.size) return buildStrips();
  for (const s of vis) {
    const el = stripEls.get(s.ch);
    if (!el) return buildStrips();
    el.strip = s;
    el.fill.style.height = `${Math.round(s.fader * 100)}%`;
    el.handle.style.bottom = `calc(${Math.round(s.fader * 100)}% - 7px)`;
    el.db.textContent = faderDb(s.fader);
    el.muteBtn.classList.toggle('lit', !!s.muted);
    el.root.classList.toggle('sel', s.ch === selected);
    el.root.classList.toggle('solo-dim', solos.size > 0 && !solos.has(s.ch));
    el.root.querySelector('.scribble').innerHTML = `${s.name}<br><small>ch${s.ch < 0 ? 'M' : s.ch + 1}</small>`;
  }
}

async function setFader(s, v) {
  v = Math.min(1, Math.max(0, v));
  if (s.main) { master.fader = v; refreshStrips(); return; }
  s.fader = v; // optimistic for touch latency
  refreshStrips();
  try { await backend.setFader(s.ch, v); }
  catch (e) { status(`fader ch${s.ch + 1}: ${e.message || e}`); }
}

async function toggleMute(s) {
  if (s.main) return;
  const next = !s.muted;
  s.muted = next;
  refreshStrips();
  try { await backend.setMute(s.ch, next); }
  catch (e) { s.muted = !next; refreshStrips(); status(`mute ch${s.ch + 1}: ${e.message || e}`); }
}

function select(ch) {
  selected = ch;
  refreshStrips();
  renderDetail();
}

/* ---------- detail panel ---------- */
// Ranges mirror crates/proto-dl32s address limits (engineering units).
const SECTIONS = [
  { title: 'Input', fields: [
    { f: 'gain', label: 'gain', unit: 'dB', type: 'f', min: -20, max: 60, step: 0.5, def: 0 },
    { f: 'phantom', label: '+48V', type: 'b' },
    { f: 'pan', label: 'pan', unit: 'L/R', type: 'f', min: -1, max: 1, step: 0.01, def: 0 },
  ]},
  { title: 'Filter / dynamics', fields: [
    { f: 'hpf_on', label: 'hpf on', type: 'b' },
    { f: 'hpf_freq', label: 'hpf', unit: 'Hz', type: 'f', min: 20, max: 400, step: 1, def: 80 },
    { f: 'gate_on', label: 'gate on', type: 'b' },
    { f: 'comp_on', label: 'comp on', type: 'b' },
    { f: 'eq_on', label: 'eq on', type: 'b' },
  ]},
  { title: 'EQ bands 1-4 (gain / freq / Q)', fields: [
    ...[1, 2, 3, 4].flatMap((b) => ([
      { f: `eq${b}_gain`, label: `eq${b} gain`, unit: 'dB', type: 'f', min: -20, max: 20, step: 0.5, def: 0 },
      { f: `eq${b}_freq`, label: `eq${b} freq`, unit: 'Hz', type: 'logf', min: 20, max: 20000, def: b === 1 ? 100 : b === 4 ? 12000 : 1000 * b },
      { f: `eq${b}_q`, label: `eq${b} Q`, type: 'f', min: 0.1, max: 20, step: 0.1, def: 1 },
      ...(b === 1 ? [] : [{ f: `eq${b}_type`, label: `eq${b} type`, type: 'enum', opts: [0, 1, 2, 3], def: 0 }]),
    ])),
  ]},
  { title: 'Aux sends 1-8', fields: [
    ...Array.from({ length: 8 }, (_, i) => ([
      { f: `aux${i + 1}_level`, label: `aux${i + 1}`, unit: 'dB', type: 'f', min: -120, max: 10, step: 0.5, def: -120 },
      { f: `aux${i + 1}_mute`, label: `m${i + 1}`, type: 'b' },
    ])).flat(),
  ]},
  { title: 'FX sends 1-4', fields: [
    ...Array.from({ length: 4 }, (_, i) => ([
      { f: `fx${i + 1}_level`, label: `fx${i + 1}`, unit: 'dB', type: 'f', min: -120, max: 10, step: 0.5, def: -120 },
      { f: `fx${i + 1}_mute`, label: `m${i + 1}`, type: 'b' },
    ])).flat(),
  ]},
];

const logToT = (v, min, max) => Math.log(v / min) / Math.log(max / min);
const logFromT = (t, min, max) => min * Math.pow(max / min, t);

async function pushParam(ch, field, v) {
  try { await backend.setParam(ch, field, v); }
  catch (e) {
    unsupported.add(field);
    renderDetail(); // hide this control; backend said no
    status(`${field}: unsupported (${e.message || e})`);
  }
}

function renderDetail() {
  const d = $('detail');
  const s = [...strips, { ch: -1, name: 'MAIN' }].find((x) => x.ch === selected);
  if (!s || s.ch < 0) {
    d.innerHTML = `<h2>MAIN</h2><div class="hint">Local monitor-dim in v1 (backend exposes no main fader yet). Use the console LR fader.</div>`;
    return;
  }
  let h = `<h2>${s.name} <small>ch${s.ch + 1}</small></h2>`;
  h += `<div class="prow"><label>name</label><input id="nm" type="text" maxlength="16" value="${s.name.replace(/"/g, '')}" style="flex:1"></div>`;
  for (const sec of SECTIONS) {
    const vis = sec.fields.filter((p) => !unsupported.has(p.f));
    if (!vis.length) continue;
    h += `<h3>${sec.title}</h3>`;
    for (const p of vis) {
      if (p.type === 'b') {
        h += `<div class="prow"><label>${p.label}</label><button class="tgl" data-f="${p.f}">off</button></div>`;
      } else if (p.type === 'enum') {
        h += `<div class="prow"><label>${p.label}</label><select data-f="${p.f}">${p.opts.map((o) => `<option>${o}</option>`).join('')}</select></div>`;
      } else {
        const isLog = p.type === 'logf';
        const lo = isLog ? 0 : p.min, hi = isLog ? 1000 : p.max;
        const val = isLog ? Math.round(logToT(p.def, p.min, p.max) * 1000) : p.def;
        h += `<div class="prow"><label>${p.label}</label><input type="range" data-f="${p.f}" min="${lo}" max="${hi}" step="${p.step || 1}" value="${val}"><output data-o="${p.f}">${p.def}${p.unit ? p.unit : ''}</output></div>`;
      }
    }
  }
  h += `<div class="hint">Controls the backend rejects disappear (unsupported on this mixer).</div>`;
  d.innerHTML = h;
  $('nm').addEventListener('change', (e) => pushParam(s.ch, 'name', e.target.value));
  d.querySelectorAll('button.tgl').forEach((b) => {
    b.addEventListener('click', () => {
      const on = !b.classList.contains('lit');
      b.classList.toggle('lit', on);
      b.textContent = on ? 'on' : 'off';
      pushParam(s.ch, b.dataset.f, on);
    });
  });
  d.querySelectorAll('select[data-f]').forEach((sel) => {
    sel.addEventListener('change', () => pushParam(s.ch, sel.dataset.f, Number(sel.value)));
  });
  d.querySelectorAll('input[type=range]').forEach((r) => {
    const def = SECTIONS.flatMap((x) => x.fields).find((p) => p.f === r.dataset.f);
    r.addEventListener('input', () => {
      const v = def.type === 'logf' ? Math.round(logFromT(r.value / 1000, def.min, def.max)) : Number(r.value);
      d.querySelector(`[data-o="${r.dataset.f}"]`).textContent = `${v}${def.unit || ''}`;
      pushParam(s.ch, r.dataset.f, v);
    });
  });
}

/* ---------- scenes (local snapshot slots) ---------- */
const SCENE_KEY = 'openmix.scenes.v1';
function loadScenes() {
  try { return JSON.parse(localStorage.getItem(SCENE_KEY)) || {}; }
  catch { return {}; }
}
function renderScenes() {
  const slots = $('slots');
  slots.innerHTML = '';
  for (let i = 1; i <= 4; i++) {
    const has = !!loadScenes()[i];
    const div = document.createElement('div');
    div.className = 'slot';
    div.innerHTML = `<b>${i}${has ? '●' : '○'}</b>`;
    const b = document.createElement('button');
    b.textContent = `S${i}`;
    b.title = has ? 'tap: recall' : 'empty — arm store, then tap';
    b.addEventListener('click', () => (storeArm ? storeScene(i) : recallScene(i)));
    div.appendChild(b);
    slots.appendChild(div);
  }
  $('store').classList.toggle('armed', storeArm);
}
function storeScene(i) {
  const all = loadScenes();
  all[i] = strips.map((s) => ({ ...s }));
  localStorage.setItem(SCENE_KEY, JSON.stringify(all));
  storeArm = false;
  renderScenes();
  status(`scene ${i} stored (${strips.length} strips)`);
}
async function recallScene(i) {
  const snap = loadScenes()[i];
  if (!snap) { status(`scene ${i} empty — tap store, then S${i}`); return; }
  status(`recalling scene ${i}…`);
  for (const s of snap) {
    const cur = strips.find((x) => x.ch === s.ch);
    if (!cur) continue;
    try {
      if (cur.fader !== s.fader) await backend.setFader(s.ch, s.fader);
      if (cur.muted !== s.muted) await backend.setMute(s.ch, s.muted);
      cur.fader = s.fader; cur.muted = s.muted;
    } catch (e) { status(`scene ${i}: ch${s.ch + 1} ${e.message || e}`); break; }
  }
  refreshStrips();
  status(`scene ${i} recalled`);
}
$('store').addEventListener('click', () => {
  storeArm = !storeArm;
  renderScenes();
  status(storeArm ? 'store armed — tap a slot' : '');
});

/* ---------- layers / bridge / connect ---------- */
$('layers').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-layer]');
  if (!b) return;
  layer = b.dataset.layer;
  document.querySelectorAll('#layers button').forEach((x) => x.classList.toggle('on', x === b));
  buildStrips();
});
$('bridgeBtn').addEventListener('click', () => {
  const c = $('bridge');
  c.hidden = !c.hidden;
  $('bridgeBtn').classList.toggle('on', !c.hidden);
  if (!c.hidden) sizeBridge();
});
function sizeBridge() {
  const c = $('bridge');
  c.width = c.clientWidth * devicePixelRatio;
  c.height = 64 * devicePixelRatio;
}
$('conn').addEventListener('click', async () => {
  const driver = $('driver').value, addr = $('addr').value.trim();
  status(`connecting ${driver} ${addr || '(sim)'}…`);
  try {
    const msg = await backend.connect(driver, addr);
    status(msg);
  } catch (e) { status(`connect failed: ${e.message || e}`); }
});

/* ---------- backend events ---------- */
backend.onState((next) => {
  strips = next;
  if (!strips.some((s) => s.ch === selected)) selected = strips[0]?.ch ?? 0;
  refreshStrips();
});
backend.onMeters((lv) => { levels = lv; lastMeterAt = performance.now(); });
if (backend.onStatus) backend.onStatus((on) => {
  const b = $('badge');
  b.textContent = on ? 'online' : 'offline';
  b.className = on ? 'online' : 'offline';
});

/* ---------- meter painting ---------- */
function paintMeter(cv, v) {
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);
  const h = Math.max(1, v * H);
  ctx.fillStyle = v > 0.95 ? '#e5484d' : v > 0.75 ? '#f5b83f' : '#46c46a';
  ctx.fillRect(0, H - h, W, h);
  ctx.fillStyle = 'rgba(255,255,255,.25)';
  for (const f of [0.75, 0.5, 0.25]) ctx.fillRect(0, H * (1 - f), W, 1);
}
function paintBridge() {
  const c = $('bridge');
  if (c.hidden || !levels.length) return;
  const ctx = c.getContext('2d');
  const W = c.width, H = c.height, n = levels.length, w = W / n;
  ctx.clearRect(0, 0, W, H);
  levels.forEach((v, i) => {
    const h = Math.max(1, v * (H - 4));
    ctx.fillStyle = v > 0.95 ? '#e5484d' : v > 0.75 ? '#f5b83f' : '#46c46a';
    ctx.fillRect(i * w + 2, H - 2 - h, w - 4, h);
  });
}
function loop() {
  // offline if no meter frame for >1.5s while expecting one
  if (lastMeterAt && performance.now() - lastMeterAt > 1500) {
    const b = $('badge');
    if (b.className !== 'offline') { b.textContent = 'offline'; b.className = 'offline'; }
  }
  for (const [, el] of stripEls) {
    const v = el.strip.main ? el.strip.fader * 0.9 : (levels[el.strip.ch] ?? 0);
    paintMeter(el.cv, el.strip.muted ? 0 : v);
  }
  paintBridge();
  requestAnimationFrame(loop);
}

/* ---------- keyboard ---------- */
document.addEventListener('keydown', (e) => {
  if (/INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName || '')) return;
  const s = strips.find((x) => x.ch === selected);
  if (!s) return;
  const step = e.shiftKey ? 0.05 : 0.01;
  if (e.key === 'ArrowUp' || e.key === 'ArrowRight') { setFader(s, s.fader + step); e.preventDefault(); }
  else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') { setFader(s, s.fader - step); e.preventDefault(); }
  else if (e.key === 'm' || e.key === 'M') toggleMute(s);
});

/* ---------- boot ---------- */
buildStrips();
renderDetail();
renderScenes();
requestAnimationFrame(loop);
window.addEventListener('resize', () => { if (!$('bridge').hidden) sizeBridge(); });
// auto-connect sim for instant use; failures surface on the badge
backend.connect('sim', '').then((m) => status(m)).catch((e) => status(e.message || e));
