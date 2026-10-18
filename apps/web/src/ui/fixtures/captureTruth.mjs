// D2 Data package: captures GROUND TRUTH display values from the live design
// page (docs/design served at http://127.0.0.1:5301/mixing-station-mixer-eq.html)
// and writes them to designTruth.json, which designData.test.ts asserts the
// TypeScript port reproduces exactly.
//
// Usage: node apps/web/src/ui/fixtures/captureTruth.mjs [--url ...] [--out ...]
// Requires a standalone Playwright + Chromium (see ~/.cache/ms-playwright);
// at most one headless browser per agent, closed when done.

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (k, d) => {
  const i = args.indexOf(k);
  return i >= 0 && args[i + 1] ? args[i + 1] : d;
};
const URL = opt('--url', 'http://127.0.0.1:5301/mixing-station-mixer-eq.html');
const OUT = opt('--out', join(HERE, 'designTruth.json'));

const CHROME = '/home/user/project';

function findButton(page, startsWith) {
  return page.evaluateHandle(s => {
    const btns = [...document.querySelectorAll('button')];
    return btns.find(b => (b.textContent || '').trim().startsWith(s)) || null;
  }, startsWith);
}

async function clickButtonStarting(page, text) {
  const h = await findButton(page, text);
  if (!h) throw new Error('button not found: ' + text);
  await h.click();
  await page.waitForTimeout(250);
  await h.dispose();
}

async function clickAria(page, label) {
  await page.click(`button[aria-label="${label}"]`, { timeout: 10000 });
  await page.waitForTimeout(250);
}

const browser = await chromium.launch({ executablePath: CHROME, args: ['--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto(URL, { waitUntil: 'load', timeout: 30000 });
await page.waitForTimeout(1200);

const truth = {};

// ---- 1. Mixer: every strip fader + pan aria (meter-independent) ----
truth.faders = await page.evaluate(() =>
  Object.fromEntries(
    [...document.querySelectorAll('[aria-label]')]
      .map(e => e.getAttribute('aria-label'))
      .filter(a => a && (a.startsWith('Fader ') || a.startsWith('Send to Mix ')))
      .map(a => {
        const m = a.match(/(?:Fader |from )(Ch \d+|FxRet \d+|Mix \d+|Grp \d+|FxSnd \d+|Mtx \d+|Main (?:LR|C)|DCA \d+)/);
        return [m ? m[1] : a, a];
      }),
  ),
);
truth.pans = await page.evaluate(() =>
  Object.fromEntries(
    [...document.querySelectorAll('[aria-label]')]
      .map(e => e.getAttribute('aria-label'))
      .filter(a => a && a.startsWith('Pan '))
      .map(a => {
        const m = a.match(/Pan (Ch \d+|FxRet \d+|Mix \d+)/);
        return [m ? m[1] : a, a];
      }),
  ),
);
truth.mixerCount = await page.evaluate(
  () => document.querySelectorAll('[aria-label^="Fader "],[aria-label^="Send to Mix "]').length,
);

// ---- 2. Channel Ch 2: EQ tab ----
await clickAria(page, 'Open EQ for Ch 2');
truth.ch2eq = await page.evaluate(() => {
  const knobs = [...document.querySelectorAll('button[aria-label]')]
    .map(e => e.getAttribute('aria-label'))
    .filter(a => a && /^(Freq|Gain|Q) /.test(a));
  const selInfo = ([...document.querySelectorAll('div')]
    .map(e => e.textContent.trim())
    .find(t => /^(HPF|Low|Lo-Mid|Hi-Mid|High) · /.test(t)) || null);
  const onLabels = [...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .filter(t => t === 'EQ In' || t === 'EQ Bypassed');
  return { knobs, selInfo, onLabels };
});

// ---- 3. Gate + Compressor tabs ----
await clickButtonStarting(page, 'Gate');
truth.ch2gate = await page.evaluate(() => {
  const knobs = [...document.querySelectorAll('button[aria-label]')]
    .map(e => e.getAttribute('aria-label'))
    .filter(a => a && /^(Threshold|Range|Attack|Hold|Release|Knee|Ratio|Makeup) /.test(a));
  const summary = [...document.querySelectorAll('div')]
    .map(e => e.textContent.trim())
    .find(t => /^Thr /.test(t)) || null;
  const onLabel = [...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .find(t => /^Gate /.test(t)) || null;
  return { knobs, summary, onLabel };
});

await clickButtonStarting(page, 'Compressor');
truth.ch2comp = await page.evaluate(() => {
  const knobs = [...document.querySelectorAll('button[aria-label]')]
    .map(e => e.getAttribute('aria-label'))
    .filter(a => a && /^(Threshold|Range|Attack|Hold|Release|Knee|Ratio|Makeup) /.test(a));
  const summary = [...document.querySelectorAll('div')]
    .map(e => e.textContent.trim())
    .find(t => /^Thr /.test(t)) || null;
  const onLabel = [...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .find(t => /^Compressor /.test(t)) || null;
  return { knobs, summary, onLabel };
});

// ---- 4. Sends tab (Ch 2) ----
await clickButtonStarting(page, 'Sends');
truth.ch2sends = await page.evaluate(() => {
  const rows = [...document.querySelectorAll('button[aria-label^="Send to Mix "]')].map(slider => {
    const row = slider.closest('div');
    const spans = row ? [...row.querySelectorAll(':scope > span')] : [];
    const nameSpan = spans.find(s => /font-weight: 600/.test(s.getAttribute('style') || ''));
    const text = spans.filter(s => s !== nameSpan).map(s => s.textContent.trim()).find(t => /^[+-]?[\d∞]/.test(t)) || null;
    const preBtn = row ? [...row.querySelectorAll('button')].find(b => /^(Pre|Post)$/.test(b.textContent.trim())) : null;
    return { aria: slider.getAttribute('aria-label'), text, pre: preBtn ? preBtn.textContent.trim() : null };
  });
  return rows;
});

// ---- 5. Input tab (Ch 2) ----
await clickButtonStarting(page, 'Input');
truth.ch2input = await page.evaluate(() => {
  const knobs = [...document.querySelectorAll('button[aria-label]')]
    .map(e => e.getAttribute('aria-label'))
    .filter(a => a && /^(Gain|Trim|Time) /.test(a));
  const dist = ([...document.querySelectorAll('div')]
    .map(e => e.textContent.trim())
    .find(t => /^≈ .* m of distance$/.test(t)) || null);
  const srcName = ([...document.querySelectorAll('span')]
    .map(e => e.textContent.trim())
    .find(t => /^(Lcl|Net|USB) \d+$/.test(t)) || null);
  const srcGroup = ([...document.querySelectorAll('span')]
    .map(e => e.textContent.trim())
    .find(t => /^(Local input|Network input|USB input|Unpatched)$/.test(t)) || null);
  const phantom = ([...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .find(t => /48V/.test(t)) || null);
  const delayLabel = ([...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .find(t => t === 'On' || t === 'Off') || null);
  return { knobs, dist, srcName, srcGroup, phantom, delayLabel };
});

// ---- 6. FX screen ----
await clickAria(page, 'Effects');
truth.fx = await page.evaluate(() => {
  const slots = [...document.querySelectorAll('button[aria-pressed]')]
    .map(b => b.textContent.replace(/\s+/g, ' ').trim())
    .filter(t => /^FX \d+/.test(t));
  const title = ([...document.querySelectorAll('div')]
    .map(e => e.textContent.trim())
    .find(t => /^FX \d+ · /.test(t)) || '').split('\n')[0].trim() || null;
  const knobs = [...document.querySelectorAll('button[aria-label]')]
    .map(e => e.getAttribute('aria-label'))
    .filter(a => a && /^(Decay|Pre-delay|Size|Hi Damp|Mix|Time|Feedback|Hi Cut|Spread|Rate|Depth) /.test(a));
  const onLabel = [...document.querySelectorAll('button')]
    .map(b => b.textContent.trim())
    .find(t => t === 'On' || t === 'Bypassed') || null;
  const typePressed = [...document.querySelectorAll('button[aria-pressed="true"]')]
    .map(b => b.textContent.trim())
    .filter(t => /^(Hall|Plate|Room|Delay|Chorus|Flanger)$/.test(t));
  return { slots, title, knobs, onLabel, typePressed };
});

// ---- 7. Scenes ----
await page.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b =>
    (b.getAttribute('aria-label') || '').startsWith('Scene '));
  if (btn) btn.click();
});
await page.waitForTimeout(300);
truth.scenes = await page.evaluate(() =>
  [...document.querySelectorAll('button[aria-pressed]')]
    .map(b => b.textContent.replace(/\s+/g, ' ').trim())
    .filter(t => /^\d{3}/.test(t)),
);
truth.sceneBar = await page.evaluate(() => {
  const texts = [...document.querySelectorAll('div')].map(e => e.textContent.trim());
  return {
    tag: texts.find(t => t === 'Selected · not recalled yet' || t === 'Current scene') || null,
    preview: texts.find(t => /^\d{3}(Soundcheck|Doors|Walk-in|Support act|Changeover|Set 1|Acoustic|Set 2|Encore|Walk-out)/.test(t)) || null,
  };
});

// ---- 8. Connection ----
await page.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b =>
    (b.getAttribute('aria-label') || '').startsWith('Connection:'));
  if (btn) btn.click();
});
await page.waitForTimeout(300);
truth.devices = await page.evaluate(() => {
  const names = ['FOH Console', 'Monitor Console', 'Stage Box A'];
  return names.map(n => {
    const el = [...document.querySelectorAll('div')].find(e => e.textContent.trim() === n);
    const card = el ? el.closest('div').parentElement : null;
    return { name: n, card: card ? card.textContent.replace(/\s+/g, ' ').trim().slice(0, 160) : null };
  });
});
truth.connStatus = await page.evaluate(() => {
  const texts = [...document.querySelectorAll('div,span')].map(e => e.textContent.trim());
  return {
    title: texts.find(t => /^Connected to /.test(t) && t.length < 80) || null,
    sub: texts.find(t => /48 kHz · latency/.test(t) && t.length < 80) || null,
    lastScan: texts.find(t => /^Scanned at/.test(t) && t.length < 40) || null,
  };
});

// ---- 9. Routing (first bank tiles) ----
await page.evaluate(() => {
  const btns = [...document.querySelectorAll('button')];
  const b = btns.find(x => (x.getAttribute('aria-label') || '') === 'Routing');
  if (b) b.click();
});
await page.waitForTimeout(300);
truth.routing = await page.evaluate(() => ({
  tiles: [...document.querySelectorAll('button[aria-pressed]')]
    .map(b => b.textContent.replace(/\s+/g, ' ').trim())
    .filter(t => /^(Ch \d+|Mix |Grp |Fx)/.test(t))
    .slice(0, 16),
}));

truth.meta = { url: URL, viewport: [1280, 720], capturedAt: new Date().toISOString() };

writeFileSync(OUT, JSON.stringify(truth, null, 2) + '\n');
console.log('wrote ' + OUT);
await browser.close();
