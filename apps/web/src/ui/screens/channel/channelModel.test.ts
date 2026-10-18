// C1 Channel package: node tests for the pure channel view-models.
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/channelModel.test.ts

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  inputVm,
  masterStripVm,
  sendRowsVm,
  tabListEntries,
  CHANNEL_TAB_ORDER,
  inputLevelText,
} from './channelModel.ts';
import {
  expirePhantom,
  initialWorld,
  selectChannel,
  sendFromDrag,
  setTab,
  stepChannel,
  togglePhantom,
  toggleSendPre,
  type World,
} from '../../state/uiLogic.ts';
import { validateLayout } from '../../modules/validate.ts';

const HERE = dirname(fileURLToPath(import.meta.url));

function worldAt(sel: string): World {
  return selectChannel(initialWorld(), sel);
}

let n = 0;
function ok(cond: unknown, msg: string): void {
  n++;
  assert(cond, msg);
}

// --- tab list entries: data-driven, exact selected state ---

{
  const w = worldAt('in2');
  const ch = w.chans['in2'];
  const entries = tabListEntries(ch, 'input');
  ok(entries.map(e => e.id).join(',') === CHANNEL_TAB_ORDER.join(','), 'rail order is input,eq,gate,comp,sends');
  ok(entries.map(e => e.title).join('|') === 'Input|HPF + EQ|Gate|Compressor|Sends', 'titles: ' + entries.map(e => e.title).join('|'));
  ok(entries[0].note === 'Lcl 02', 'input sub-label is the patch source, got ' + entries[0].note);
  ok(entries[0].current && entries[0].border === ch.color && entries[0].bg === '#363d48', 'selected tab has channel-colour border');
  for (const e of entries.slice(1)) {
    ok(!e.current && e.border === 'var(--u4-btn-bg)' && e.bg === 'var(--u4-btn-bg)', 'unselected tab neutral: ' + e.id);
  }
  const eq = tabListEntries(ch, 'eq');
  ok(eq[1].current && eq[1].border === ch.color, 'selection follows the store tab');
}

// --- master strip vm ---

{
  const w = worldAt('in2');
  const vm = masterStripVm(w);
  ok(vm.name === 'Ch 2', 'name ' + vm.name);
  ok(vm.src === 'Lcl 02', 'src ' + vm.src);
  ok(vm.color === '#e8853d', 'tape chip ' + vm.color);
  ok(vm.panText === 'R28', 'pan text ' + vm.panText);
  ok(vm.panAria === 'Pan Ch 2 R28', 'pan aria ' + vm.panAria);
  ok(vm.muteAria === 'Mute Ch 2', 'mute aria');
  ok(vm.muteEnabled === true, 'mute enabled by default');
  ok(vm.accent === 'chan', 'accent chan when SOF off');
  ok(vm.faderPos === w.chans['in2'].fader, 'fader pos');
  ok(vm.faderAria === 'Fader Ch 2 ' + vm.readout + ' dB', 'fader aria ' + vm.faderAria);
}

// --- channel prev/next wraps (store action the strip buttons call) ---

{
  const w0 = initialWorld(); // sel in1
  const back = stepChannel(w0, -1);
  const order = ['in1'];
  void order;
  ok(back.state.sel !== 'in1', 'prev from first wraps, got ' + back.state.sel);
  const fwd = stepChannel(back, 1);
  ok(fwd.state.sel === 'in1', 'next wraps back to in1, got ' + fwd.state.sel);
  ok(setTab(w0, 'sends').state.tab === 'sends', 'tab switch');
}

// --- input vm: texts, colours, phantom arm flow ---

{
  const w = worldAt('in2');
  const vm = inputVm(w)!;
  ok(vm.hasInput, 'in channel has input');
  ok(vm.gain === 29 && vm.gainText === '29.0 dB', 'gain ' + vm.gainText);
  ok(vm.trimText === '0.0 dB', 'trim ' + vm.trimText);
  ok(vm.delayText === '0.00 ms', 'delay ' + vm.delayText);
  ok(vm.delayDist === '≈ 0.00 m of distance', 'dist ' + vm.delayDist);
  ok(vm.srcName === 'Lcl 02' && vm.srcGroup === 'Local input', 'source');
  ok(vm.delayLabel === 'Off' && vm.delayBg === 'var(--u4-btn-bg)', 'delay off toggle');
  ok(vm.delayKnobColor === '#4a5262', 'delay knob grey when off');
  ok(vm.phantomLabel === '48V Off' && vm.phantomBg === 'var(--u4-btn-bg)', 'phantom off');
  const armed = togglePhantom(w, 'in2');
  const avm = inputVm(armed)!;
  ok(avm.phantomLabel === 'Tap again: 48V on', 'armed label ' + avm.phantomLabel);
  ok(avm.phantomBg === '#f0b44a' && avm.phantomBorder === '#f6c76a', 'armed amber');
  const confirmed = togglePhantom(armed, 'in2');
  ok(inputVm(confirmed)!.phantomLabel === '48V On', 'second tap commits');
  const expired = expirePhantom(armed, 'in2');
  ok(inputVm(expired)!.phantomLabel === '48V Off', '3 s expiry disarms');
  ok(inputVm(worldAt('mix1'))!.hasInput === false, 'bus has no preamp');
}

// --- sends vm: 16 rows, pre white / post cyan ---

{
  const w = worldAt('in2');
  const rows = sendRowsVm(w.chans['in2']);
  ok(rows.length === 16, '16 mixes');
  ok(rows[0].name === 'Mix 1' && rows[15].name === 'Mix 16', 'names');
  ok(rows[0].pre && rows[0].preLabel === 'Pre' && rows[0].fill === 'var(--u4-text-1)', 'first 8 pre/white');
  ok(!rows[8].pre && rows[8].preLabel === 'Post' && rows[8].fill === '#22d3e6', 'rest post/cyan');
  ok(rows[0].aria === 'Send to Mix 1 ' + rows[0].text + ' dB', 'row aria ' + rows[0].aria);
  ok(rows[0].preAria === 'Mix 1 pre-fader', 'pre aria');
  ok(rows[0].preBg === 'var(--u4-text-1)' && rows[0].preFg === 'var(--u4-seg-on-fg)', 'pre button white');
  ok(rows[8].preBg === 'var(--u4-btn-bg)' && rows[8].preFg === 'var(--u4-text-4)', 'post button dark');
  ok(rows[0].pct === (w.chans['in2'].sends![0] * 100).toFixed(1), 'pct');
  const toggled = toggleSendPre(w, 'in2', 0);
  ok(sendRowsVm(toggled.chans['in2'])[0].preLabel === 'Post', 'pre/post toggle');
}

// --- send drag math (exact design 'h' branch) ---

{
  ok(sendFromDrag(0.5, 90, 180, false) === 1, 'half track right clamps to 1');
  ok(sendFromDrag(0.5, 45, 180, true) === 0.5625, 'fine quarter: ' + sendFromDrag(0.5, 45, 180, true));
  ok(sendFromDrag(0.2, -36, 180, false) === 0, 'clamps to 0');
}

// --- input level text ---

{
  ok(inputLevelText(-90) === '-∞ dBFS', 'silent');
  ok(inputLevelText(-13.5) === '-13.5 dBFS', 'frozen in2 level');
}

// --- default layouts validate against the L0 schema ---

{
  for (const f of ['channel.default.json', 'input.default.json', 'sends.default.json']) {
    const tree = JSON.parse(readFileSync(join(HERE, '..', '..', 'layouts', f), 'utf8'));
    const v = validateLayout(tree);
    ok(v.ok, f + ' validates: ' + JSON.stringify(v.errors));
  }
  const channel = JSON.parse(readFileSync(join(HERE, '..', '..', 'layouts', 'channel.default.json'), 'utf8'));
  ok(channel.children.map((c: { id: string }) => c.id).join(',') === 'tab-list,content,master', 'channel layout order');
  ok(channel.children[0].module === 'col' && channel.children[2].module === 'col', 'tab list + master strip are layout containers');
  const tabIds = (channel.children[1].children as { id: string }[]).map(c => c.id).join(',');
  ok(tabIds === 'tab-input,tab-sends,tab-eq,tab-gate,tab-comp', 'when-switched tab layouts: ' + tabIds);
  const input = JSON.parse(readFileSync(join(HERE, '..', '..', 'layouts', 'input.default.json'), 'utf8'));
  ok(input.children.map((c: { id: string }) => c.id).join(',') === 'input-empty,preamp,source,delay', 'input cards');
  const sends = JSON.parse(readFileSync(join(HERE, '..', '..', 'layouts', 'sends.default.json'), 'utf8'));
  ok(sends.children[1].module === 'repeat' && sends.children[1].children[0].module === 'send-row', 'sends repeat');
}

console.log(`channelModel.test.ts: ${n} assertions passed`);
