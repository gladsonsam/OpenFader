<!-- P1 signal gallery (?ui=4&gallery=signal): every signal primitive in
     every state, at the design's real sizes, meters driven by a live
     MeterSim with a freeze toggle. Stable ids #sig-<name>. -->
<script lang="ts">
  import AppFrame from '../../AppFrame.svelte';
  import Fader from '../../primitives/signal/Fader.svelte';
  import Meter from '../../primitives/signal/Meter.svelte';
  import MuteButton from '../../primitives/signal/MuteButton.svelte';
  import PanBar from '../../primitives/signal/PanBar.svelte';
  import NameBar from '../../primitives/signal/NameBar.svelte';
  import Knob from '../../primitives/signal/Knob.svelte';
  import ScaleLabels from '../../primitives/signal/ScaleLabels.svelte';
  import { buildChannels, fmtDb } from '../../fixtures/designData.ts';
  import { inputLevelText } from '../../state/uiLogic.ts';
  import { MeterSim } from '../../fixtures/meterSim.ts';

  const chans = buildChannels();
  const params = new URLSearchParams(location.search);
  let frozen = $state(params.get('frozen') === '1' || params.get('freeze') === '1');
  /** Silent mode matches the design with its 80 ms meter loop neutralised
      (levels -90 everywhere, no peaks, no clips) for pixel comparison. */
  const silent = params.get('silent') === '1';

  const sim = new MeterSim(['in1', 'in2', 'in3', 'mix1', 'mainlr']);
  sim.bind(
    (id) => chans[id]?.act ?? 0,
    (id) => chans[id]?.ph ?? 0,
  );
  const clipSlot = sim.slot('in2');
  if (!silent) {
    sim.freeze();
    // Clip-demo slot: hot level, latched clip.
    sim.level[clipSlot] = -0.1;
    sim.peak[clipSlot] = -0.1;
    sim.clipUntil[clipSlot] = Number.POSITIVE_INFINITY;
  }

  const lvl = (id: string) => sim.levelOf(id);
  const pk = (id: string) => sim.peakOf(id);
  const cl = (id: string, now: number) => sim.clipping(id, now);

  let tick = $state(0);
  $effect(() => {
    // Silent mode matches the neutralised design: the sim never steps.
    if (frozen || silent) return;
    const iv = setInterval(() => {
      sim.step(Date.now() / 1000, Date.now());
      tick++;
    }, 80);
    return () => clearInterval(iv);
  });
  void tick;

  const still = $derived(frozen || silent);

  const nowMs = () => Date.now();

  // Fixture-exact demo values (buildChannels, seed 7) so the idle demos
  // match the design's Ch 1 / Ch 2 / Main LR strips pixel for pixel.
  const IN1_FADER = 0.6433855882551387;
  const IN2_FADER = 0.740589254806367;
  const MAIN_FADER = 0.6310036281179671;
  // Interactive demo state.
  let faderStrip = $state(IN1_FADER);
  let faderFine = $state(0.6);
  let faderAside = $state(IN2_FADER);
  let faderMaster = $state(MAIN_FADER);
  let panStrip = $state(0.28);
  let knobGain = $state(32);
  let knobRatio = $state(3);
  let eqfVal = $state(120);
  let muted = $state(false);
  let mutedOn = $state(true);

  // rAF fps probe (2 s window) for the perf check.
  let fps = $state(0);
  $effect(() => {
    let frames = 0;
    let start = performance.now();
    let raf = 0;
    const loop = () => {
      frames++;
      const t = performance.now();
      if (t - start >= 2000) {
        fps = Math.round((frames * 1000) / (t - start));
        frames = 0;
        start = t;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  const well: Record<string, string> = {};
  void well;
</script>

<AppFrame>
  <div style="flex:1;min-height:0;overflow-y:auto;padding:12px;background:var(--u4-page-bg)">
    <div style="max-width:1100px;margin:0 auto;display:flex;flex-direction:column;gap:16px;color:var(--u4-text-1)">
      <div style="display:flex;align-items:center;gap:12px">
        <div style="font-size:17px;font-weight:600">Signal primitives gallery</div>
        <button
          aria-label="Freeze meters"
          aria-pressed={frozen ? 'true' : 'false'}
          onclick={() => {
            frozen = !frozen;
            if (frozen) {
              sim.freeze();
              sim.level[clipSlot] = -0.1;
              sim.peak[clipSlot] = -0.1;
              sim.clipUntil[clipSlot] = Number.POSITIVE_INFINITY;
            }
          }}
          style="height:40px;padding:0 16px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-btn-fg);font-size:14px;cursor:pointer"
        >{frozen ? 'Unfreeze meters' : 'Freeze meters'}</button>
        <div id="sig-fps" style="font-size:13px;color:var(--u4-text-5)">fps {fps}</div>
      </div>

      <section id="sig-fader" aria-label="Strip faders" style="display:flex;gap:16px;align-items:flex-end;flex-wrap:wrap">
        <div id="sig-fader-idle" style="width:79px;height:340px;background:var(--u4-panel-bg)">
          <Fader value={IN1_FADER} dbText={fmtDb(IN1_FADER)} ariaLabel={'Fader Ch 1 ' + fmtDb(IN1_FADER) + ' dB'} oninput={() => {}} meterFrozen={still} meter={{ getLevel: () => lvl('in1'), getPeak: () => pk('in1'), getClip: () => cl('in1', nowMs()) }} />
        </div>
        <div id="sig-fader-live" style="width:79px;height:340px;background:var(--u4-panel-bg)">
          <Fader value={faderStrip} dbText={fmtDb(faderStrip)} ariaLabel={'Fader Ch 1 ' + fmtDb(faderStrip) + ' dB'} oninput={(v) => (faderStrip = v)} meterFrozen={still} meter={{ getLevel: () => lvl('in1'), getPeak: () => pk('in1'), getClip: () => cl('in1', nowMs()) }} />
        </div>
        <div id="sig-fader-unity" style="width:79px;height:340px;background:var(--u4-panel-bg)">
          <Fader value={0.75} dbText={fmtDb(0.75)} ariaLabel={'Fader Ch 3 ' + fmtDb(0.75) + ' dB'} oninput={() => {}} meterFrozen={still} />
        </div>
        <div id="sig-fader-extreme" style="display:flex;gap:16px;align-items:flex-end">
          <div style="width:79px;height:340px;background:var(--u4-panel-bg)">
            <Fader value={1} dbText={fmtDb(1)} ariaLabel={'Fader Ch 4 ' + fmtDb(1) + ' dB'} oninput={() => {}} meterFrozen={still} />
          </div>
          <div style="width:79px;height:340px;background:var(--u4-panel-bg)">
            <Fader value={0.02} dbText={fmtDb(0.02)} ariaLabel={'Fader Ch 5 ' + fmtDb(0.02) + ' dB'} oninput={() => {}} meterFrozen={still} />
          </div>
        </div>
        <div id="sig-fader-sof" style="width:79px;height:340px;background:var(--u4-panel-bg)">
          <Fader value={faderFine} dbText={fmtDb(faderFine)} accent="sof" fine readColor="#22d3e6" ariaLabel={'Send to Mix 1 from Ch 6 ' + fmtDb(faderFine) + ' dB'} oninput={(v) => (faderFine = v)} meterFrozen={still} />
        </div>
        <div id="sig-fader-disabled" style="width:79px;height:340px;background:var(--u4-panel-bg)">
          <Fader value={IN1_FADER} dbText={fmtDb(IN1_FADER)} disabled ariaLabel={'Fader Ch 7 ' + fmtDb(IN1_FADER) + ' dB'} meterFrozen={still} />
        </div>
      </section>

      <section id="sig-fader-aside" aria-label="Channel strip fader" style="display:flex;gap:16px;align-items:flex-end">
        <div id="sig-aside-idle" style="width:136px;height:422px;background:var(--u4-panel-bg)">
          <Fader variant="aside" value={faderAside} dbText={fmtDb(faderAside)} ariaLabel={'Fader Ch 2 ' + fmtDb(faderAside) + ' dB'} oninput={(v) => (faderAside = v)} meterFrozen={still} meter={{ getLevel: () => lvl('in2'), getPeak: () => pk('in2'), getClip: () => cl('in2', nowMs()) }} />
        </div>
        <div id="sig-aside-clip" style="width:136px;height:422px;background:var(--u4-panel-bg)">
          <Fader variant="aside" value={0.8} dbText={fmtDb(0.8)} ariaLabel={'Fader Ch 2 ' + fmtDb(0.8) + ' dB'} oninput={() => {}} meterFrozen={still} meter={{ getLevel: () => -0.1, getPeak: () => -0.1, getClip: () => true, grOn: true }} />
        </div>
      </section>

      <section id="sig-fader-master" aria-label="Master fader" style="display:flex;gap:16px;align-items:flex-end">
        <div id="sig-master-idle" style="width:108px;height:345px;background:#121a27">
          <Fader variant="master" accent="master" value={faderMaster} dbText={fmtDb(faderMaster)} readColor="#e8edf5" ariaLabel={'Fader Main LR ' + fmtDb(faderMaster) + ' dB'} oninput={(v) => (faderMaster = v)} meterFrozen={still} meter={{ getLevel: (ch) => (ch === 0 ? lvl('mainlr') : lvl('mainlr') - 1.5) }} />
        </div>
        <div id="sig-master-sof" style="width:108px;height:345px;background:#121a27">
          <Fader variant="master" accent="sof" value={0.62} dbText={fmtDb(0.62)} readColor="#e8edf5" ariaLabel={'Send to Mix 1 from Mix 1 ' + fmtDb(0.62) + ' dB'} oninput={() => {}} meterFrozen={still} />
        </div>
      </section>

      <section id="sig-scale" aria-label="Scale labels" style="display:flex;gap:16px">
        <div id="sig-scale-strip" style="position:relative;width:60px;height:300px"><ScaleLabels variant="strip" /></div>
        <div id="sig-scale-aside" style="position:relative;width:60px;height:300px"><ScaleLabels variant="aside" /></div>
      </section>

      <section id="sig-meter-strip" aria-label="Strip meters" style="display:flex;gap:16px;align-items:flex-end">
        <div id="sig-meter-normal" style="position:relative;width:14px;height:280px"><Meter variant="strip" frozen={still} source={{ getLevel: () => -4.3, getPeak: () => -2.0 }} /></div>
        <div id="sig-meter-clip" style="position:relative;width:14px;height:280px"><Meter variant="strip" frozen source={{ getLevel: () => -0.1, getPeak: () => -0.1, getClip: () => true, grOn: true }} /></div>
        <div id="sig-meter-quiet" style="position:relative;width:14px;height:280px"><Meter variant="strip" frozen source={{ getLevel: () => -90, getPeak: () => -90, getClip: () => false }} /></div>
      </section>

      <section id="sig-meter-master" aria-label="Master meter" style="display:flex;gap:16px">
        <div id="sig-master-meter" style="position:relative;width:19px;height:280px"><Meter variant="master" frozen={still} source={{ getLevel: (ch) => (ch === 0 ? -6.2 : -7.7), showPeak: false }} /></div>
      </section>

      <section id="sig-meter-hbar" aria-label="Input level bar" style="display:flex;flex-direction:column;gap:6px;max-width:420px">
        <div style="display:flex;justify-content:space-between;font-size:12px;color:var(--u4-text-6)"><span>Input level</span><span id="sig-hbar-text" style="color:var(--u4-text-1)">{inputLevelText(silent ? -90 : -17.5)}</span></div>
        <div id="sig-hbar"><Meter variant="hbar" frozen source={{ getLevel: () => (silent ? -90 : -17.5) }} /></div>
        <div id="sig-hbar-hot"><Meter variant="hbar" frozen source={{ getLevel: () => -1.2 }} /></div>
      </section>

      <section id="sig-mute" aria-label="Mute buttons" style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
        <div id="sig-mute-off" style="width:120px"><MuteButton muted={false} label="Mute Ch 1" onpress={() => {}} /></div>
        <div id="sig-mute-on" style="width:120px"><MuteButton muted={mutedOn} label="Mute Ch 2" onpress={() => { mutedOn = !mutedOn; }} /></div>
        <div id="sig-mute-disabled" style="width:120px"><MuteButton muted={false} enabled={false} label="Mute Ch 3" /></div>
        <div id="sig-mute-tall" style="width:136px"><MuteButton muted={muted} height={44} label="Mute Ch 4" onpress={() => { muted = !muted; }} /></div>
      </section>

      <section id="sig-pan" aria-label="Pan bars" style="display:flex;gap:16px;flex-wrap:wrap">
        <div id="sig-pan-strip" style="width:200px"><PanBar value={panStrip} name="Ch 2" oninput={(v) => (panStrip = v)} /></div>
        <div id="sig-pan-aside" style="width:200px"><PanBar variant="aside" value={0.28} name="Ch 2" oninput={() => {}} /></div>
        <div id="sig-pan-text" style="width:200px"><PanBar variant="aside" value={0.45} name="Ch 2" oninput={() => {}} /></div>
        <div id="sig-pan-null" style="width:200px"><PanBar value={null} name="Main LR" /></div>
        <div id="sig-pan-disabled" style="width:200px"><PanBar value={-0.28} name="Ch 3" disabled oninput={() => {}} /></div>
      </section>

      <section id="sig-name" aria-label="Name bars" style="display:flex;gap:8px;flex-wrap:wrap">
        <div id="sig-name-ch" style="width:120px"><NameBar name="Ch 1" sub="Lcl 01" color="#5cdb5c" text="#10240f" selectAria="Select Ch 1, source Lcl 01" onselect={() => {}} /></div>
        <div id="sig-name-mix" style="width:120px"><NameBar name="Mix 1" sub="Bus" color="#22d3e6" text="#062a30" selectAria="Select Mix 1, source Bus" onselect={() => {}} /></div>
        <div id="sig-name-grp" style="width:120px"><NameBar name="Grp 1" sub="Bus" color="#2563eb" text="#ffffff" selectAria="Select Grp 1, source Bus" onselect={() => {}} /></div>
        <div id="sig-name-mtx" style="width:120px"><NameBar name="Mtx 1" sub="Out" color="#c026d3" text="#ffffff" selectAria="Select Mtx 1, source Out" onselect={() => {}} /></div>
        <div id="sig-name-dca" style="width:120px"><NameBar name="DCA 1" sub="DCA" color="#dc2626" text="#ffffff" selectAria="Select DCA 1, source DCA" onselect={() => {}} /></div>
        <div id="sig-name-plate" style="width:136px"><NameBar variant="plate" name="Ch 2" color="#5cdb5c" text="#10240f" /></div>
      </section>

      <section id="sig-knob" aria-label="Knobs" style="display:flex;gap:24px;align-items:flex-start;flex-wrap:wrap">
        <div id="sig-knob-eq"><Knob specKey="gain" value={knobGain} label="Gain" color="#ff9f43" size={58} defaultValue={30} oninput={(v) => (knobGain = v)} /></div>
        <div id="sig-knob-input"><Knob specKey="trim" value={0} label="Trim" color="#7be27b" size={68} defaultValue={0} oninput={() => {}} /></div>
        <div id="sig-knob-gain"><Knob specKey="gain" value={29} label="Gain" color="#7be27b" size={68} defaultValue={30} oninput={() => {}} /></div>
        <div id="sig-knob-dyn"><Knob specKey="ratio" value={knobRatio} label="Ratio" color="#ff9f43" size={80} defaultValue={3} oninput={(v) => (knobRatio = v)} /></div>
        <div id="sig-knob-fx"><Knob specKey="gain" mode="norm" value={0.42} label="Decay" color="#b28cff" size={92} fx defaultValue={0.5} oninput={() => {}} /></div>
        <div id="sig-knob-min"><Knob specKey="cthr" value={-60} label="Threshold" color="#ff9f43" size={80} defaultValue={-20} oninput={() => {}} /></div>
        <div id="sig-knob-max"><Knob specKey="cthr" value={0} label="Threshold" color="#ff9f43" size={80} defaultValue={-20} oninput={() => {}} /></div>
        <div id="sig-knob-compthr"><Knob specKey="cthr" value={-13} label="Threshold" color="#ff9f43" size={80} defaultValue={-20} oninput={() => {}} /></div>
        <div id="sig-knob-disabled"><Knob specKey="gain" value={32} label="Gain" color="#ff9f43" size={58} defaultValue={30} disabled /></div>
        <div id="sig-knob-eqf"><Knob specKey="gain" mode="eq" eqBand={1} eqParam="f" value={eqfVal} label="Freq" color="#ff9f43" size={58} defaultValue={100} oninput={(v) => (eqfVal = v)} /></div>
        <div id="sig-knob-eqg"><Knob specKey="gain" mode="eq" eqBand={1} eqParam="g" value={0.5} label="Gain" color="#ff9f43" size={58} defaultValue={0} valueSize={12} oninput={() => {}} /></div>
      </section>

      <section id="sig-stress" aria-label="Stress: 16 animated faders" style="display:flex;gap:3px">
        {#each Array.from({ length: 16 }, (_, i) => i) as i (i)}
          <div style="width:60px;height:280px;background:var(--u4-panel-bg)">
            <Fader value={0.6 + (i % 5) * 0.03} dbText={fmtDb(0.6 + (i % 5) * 0.03)} ariaLabel={'Fader Stress ' + (i + 1) + ' ' + fmtDb(0.6 + (i % 5) * 0.03) + ' dB'} oninput={() => {}} meterFrozen={still} meter={{ getLevel: () => lvl('in1') - i, getPeak: () => pk('in1') - i, getClip: () => false }} />
          </div>
        {/each}
      </section>
    </div>
  </div>
</AppFrame>
