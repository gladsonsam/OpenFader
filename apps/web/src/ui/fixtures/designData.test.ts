// D2 Data package: parity test — asserts the TypeScript port in designData.ts
// reproduces the live design's display values (designTruth.json, captured by
// captureTruth.mjs from http://127.0.0.1:5301/mixing-station-mixer-eq.html).
//
// Run: node --experimental-strip-types apps/web/src/ui/fixtures/designData.test.ts

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  buildChannels,
  buildFx,
  compKnobTexts,
  compSummary,
  delayDistanceText,
  delayKnobTexts,
  devices,
  eqBandTexts,
  faderAria,
  fmtDb,
  fmtF,
  fmtG,
  fmtMs,
  fmtRatio,
  fxKnobTexts,
  fxRetLabel,
  fxStateLabel,
  gateKnobTexts,
  gateSummary,
  hpfTexts,
  inputKnobTexts,
  panAria,
  panText,
  posToDb,
  scenes,
  sendAria,
  spec,
} from './designData.ts';
import { fxTitleText } from '../state/uiLogic.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
const truth = JSON.parse(readFileSync(join(HERE, 'designTruth.json'), 'utf8'));

let compared = 0;
let sections = 0;
function eq(actual: unknown, expected: unknown, label: string): void {
  compared++;
  assert.strictEqual(actual, expected, `${label}\n  actual:   ${JSON.stringify(actual)}\n  expected: ${JSON.stringify(expected)}`);
}
function section(name: string): void {
  sections++;
  console.log(`-- ${name}`);
}

// ---------- mixer strips ----------
section('mixer faders');
{
  const chans = buildChannels();
  for (let i = 1; i <= 16; i++) {
    const c = chans['in' + i];
    eq(faderAria(c.name, c.fader, false, 0), truth.faders['Ch ' + i], `fader in${i}`);
  }
  const main = chans['mainlr'];
  eq(faderAria(main.name, main.fader, false, 0), truth.faders['Main LR'], 'fader mainlr');
  eq(Object.keys(truth.faders).length, 17, 'fader count');
}

section('mixer pans');
{
  const chans = buildChannels();
  for (let i = 1; i <= 16; i++) {
    const c = chans['in' + i];
    eq(panAria(c.name, c.pan), truth.pans['Ch ' + i], `pan in${i}`);
  }
}

// ---------- Ch 2 EQ ----------
section('ch2 eq');
{
  const ch = buildChannels()['in2'];
  const got: string[] = ['Freq ' + hpfTexts(ch.eq!.hpf).freq];
  ch.eq!.bands.forEach(b => {
    const t = eqBandTexts(b);
    got.push('Freq ' + t.freq, 'Gain ' + t.gain, 'Q ' + t.q);
  });
  assert.deepStrictEqual(got, truth.ch2eq.knobs, 'eq knob aria list');
  compared += got.length;
  eq(ch.eq!.on ? 'EQ In' : 'EQ Bypassed', truth.ch2eq.onLabels[0], 'eq on label');
}

// ---------- Ch 2 gate / comp ----------
section('ch2 gate');
{
  const g = buildChannels()['in2'].dyn!.gate;
  const got = gateKnobTexts(g).map(k => k.label + ' ' + k.value);
  assert.deepStrictEqual(got, truth.ch2gate.knobs, 'gate knob aria list');
  compared += got.length;
  eq(gateSummary(g), truth.ch2gate.summary, 'gate summary');
  eq(g.on ? 'Gate In' : 'Gate Bypassed', truth.ch2gate.onLabel, 'gate on label');
}

section('ch2 comp');
{
  const c = buildChannels()['in2'].dyn!.comp;
  const got = compKnobTexts(c).map(k => k.label + ' ' + k.value);
  assert.deepStrictEqual(got, truth.ch2comp.knobs, 'comp knob aria list');
  compared += got.length;
  eq(compSummary(c), truth.ch2comp.summary, 'comp summary');
  eq(c.on ? 'Compressor In' : 'Compressor Bypassed', truth.ch2comp.onLabel, 'comp on label');
}

// ---------- Ch 2 sends ----------
section('ch2 sends');
{
  const ch = buildChannels()['in2'];
  assert.strictEqual(truth.ch2sends.length, 16, 'send row count');
  truth.ch2sends.forEach((row: { aria: string; text: string; pre: string }, k: number) => {
    const v = ch.sends![k];
    eq(sendAria(k, v), row.aria, `send ${k + 1} aria`);
    eq(fmtDb(v), row.text, `send ${k + 1} text`);
    eq(ch.sendPre![k] ? 'Pre' : 'Post', row.pre, `send ${k + 1} pre`);
  });
}

// ---------- Ch 2 input ----------
section('ch2 input');
{
  const inp = buildChannels()['in2'].input!;
  const [gain, trim] = inputKnobTexts(inp);
  const [delay] = delayKnobTexts(inp);
  const got = ['Gain ' + gain.value, 'Trim ' + trim.value, 'Time ' + delay.value];
  assert.deepStrictEqual(got, truth.ch2input.knobs, 'input knob aria list');
  compared += got.length;
  eq(delayDistanceText(inp.delay), truth.ch2input.dist, 'delay distance');
  eq('Lcl 02', truth.ch2input.srcName, 'src name');
  eq('Local input', truth.ch2input.srcGroup, 'src group');
  eq(inp.phantom ? '48V On' : '48V Off', truth.ch2input.phantom, 'phantom label');
  eq(inp.delayOn ? 'On' : 'Off', truth.ch2input.delayLabel, 'delay label');
}

// ---------- FX ----------
section('fx');
{
  const fx = buildFx();
  assert.strictEqual(truth.fx.slots.length, 8, 'fx slot count');
  fx.forEach((s, k) => {
    const want = `FX ${k + 1} ${fxStateLabel(s)} ${s.type} ${fxRetLabel(k)}`;
    eq(want, truth.fx.slots[k], `fx slot ${k + 1}`);
  });
  eq(fxTitleText(0, fx[0]), truth.fx.title, 'fx title');
  const got = fxKnobTexts(fx[0]).map(k => k.label + ' ' + k.value);
  assert.deepStrictEqual(got, truth.fx.knobs, 'fx knob aria list');
  compared += got.length;
  eq(fx[0].on ? 'On' : 'Bypassed', truth.fx.onLabel, 'fx on label');
  eq(fx[0].type, truth.fx.typePressed[0], 'fx type pressed');
}

// ---------- scenes ----------
section('scenes');
{
  const names = scenes();
  assert.strictEqual(truth.scenes.length, 10, 'scene card count');
  names.forEach((nm, i) => {
    const num = String(i + 1).padStart(3, '0');
    const tag = i === 4 ? 'Current ' : '';
    eq(`${num} ${tag}${nm}`.replace(/\s+/g, ' ').trim(), truth.scenes[i], `scene ${i + 1}`);
  });
  eq('005' + names[4], truth.sceneBar.preview, 'scene preview');
  eq('Current scene', truth.sceneBar.tag, 'scene tag');
}

// ---------- devices / connection ----------
section('devices');
{
  const devs = devices();
  assert.strictEqual(truth.devices.length, 3, 'device count');
  devs.forEach((d, i) => {
    const want = `${d.name} ${d.model} · ${d.ip}`;
    assert.ok(
      (truth.devices[i].card as string).startsWith(want),
      `device ${d.key}: ${truth.devices[i].card}`,
    );
    compared++;
  });
  eq('Connected to ' + devs[0].name, truth.connStatus.title.split('\n')[0].trim(), 'conn title');
  eq('Scanned at start-up', truth.connStatus.lastScan, 'last scan');
}

// ---------- routing tiles ----------
section('routing');
{
  const chans = buildChannels();
  truth.routing.tiles.slice(0, 16).forEach((t: string, k: number) => {
    eq(`Ch ${k + 1} ${chans['in' + (k + 1)].src}`, t, `route tile ${k + 1}`);
  });
}

// ---------- formatting units ----------
section('formatting');
{
  eq(fmtDb(0.75), '0.0', 'fmtDb unity');
  eq(fmtDb(0), '-∞', 'fmtDb bottom');
  eq(fmtDb(1), '+10.0', 'fmtDb top');
  eq(fmtF(994), '994 Hz', 'fmtF hz');
  eq(fmtF(2940), '2.94 kHz', 'fmtF khz2');
  eq(fmtF(8000), '8.00 kHz', 'fmtF khz');
  eq(fmtF(12000), '12.0 kHz', 'fmtF khz1');
  eq(fmtG(0), '0.0 dB', 'fmtG zero');
  eq(fmtG(0.5), '+0.5 dB', 'fmtG pos');
  eq(fmtG(-5.5), '-5.5 dB', 'fmtG neg');
  eq(fmtMs(0.5), '0.50 ms', 'fmtMs sub10');
  eq(fmtMs(10), '10.0 ms', 'fmtMs sub100');
  eq(fmtMs(150), '150 ms', 'fmtMs int');
  eq(fmtMs(1500), '1.50 s', 'fmtMs s');
  eq(panText(0), 'C', 'pan c');
  eq(panText(null), '', 'pan none');
  eq(panText(-0.32), 'L32', 'pan l');
  eq(panText(0.28), 'R28', 'pan r');
  eq(fmtRatio(3), '3.0:1', 'ratio');
  eq(posToDb(0.75), 0, 'posToDb unity');
  eq(spec('ratio').fmt(3), '3.0:1', 'spec ratio');
  eq(spec('gain').fmt(29), '29.0 dB', 'spec gain');
}

console.log(`\nparity: ${compared} values compared across ${sections} sections — all match`);
