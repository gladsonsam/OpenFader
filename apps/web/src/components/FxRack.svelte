<script lang="ts">
  // FxRack: 4 X-Air FX slot cards with per-effect labeled params.
  //
  // Driver mapping (crates/proto-xair/src/fx.rs): slot N 1..4, param MM 01..64
  // at `/fx/N/par/MM`, values 0.0..=1.0 bit-exact (hardware-verified read of
  // `/fx/1/par/01`; see docs/xr18-baseline.md §6 row 5). UI field naming goes
  // through the EXISTING `param` command — no protocol change:
  //   backend.setParam(slot - 1, `fx${slot}_par${MM}`, value01)
  // e.g. slot 1 par 1 -> setParam(0, 'fx1_par01', 0.5). `ch` carries the
  // 0-based slot so a future relay can route without parsing; the field is
  // authoritative. A relay/shell without an fx route rejects the field and
  // the card disables itself (unsupported-gracefully, same rule as Detail).
  //
  // Labeled params: the per-slot "fx" picker selects which effect's par set
  // to render, from the research table
  // (`research_notes/FX param labels/fx-labels.md`, mirrored in
  // `crates/proto-xair/src/fx.rs`). Continuous pars render sliders, enum
  // pars render selects (discrete steps over 0..1; OFF/ON exactly 0/1 —
  // the option-index mapping is assumed, the notes only promise discrete
  // steps), padding pars are hidden. Rows the notes flag low-confidence
  // render with a "?" suffix plus a header note, never asserted. Unknown
  // effect index falls back to the generic P01–P08 sliders.
  //
  // The picker is LABELS ONLY: it never writes `/fx/N/type`, so picking an
  // entry never retunes the desk — it just relabels the same fxN_parMM
  // addresses. "Generic" is today's 8 live sliders + the par 09–64 TODO row.
  import type { Backend } from '../lib/backend';

  interface Props {
    backend: Backend;
    say?: (t: string) => void;
  }

  let { backend, say }: Props = $props();

  const KNOWN_PARS = 8; // generic fallback: par 01..08 live; rest is a TODO row
  const PARAM_COUNT = 64; // cf. core fx::FX_PARAM_COUNT

  interface FxParDef {
    par: number;
    label: string;
    unit: string;
    min: string;
    max: string;
    options?: string[];
    inferred?: boolean;
    bank?: string;
  }

  interface FxEffectDef {
    index: number;
    name: string;
    desc: string;
    pars: FxParDef[];
  }

  // Builders mirror crates/proto-xair/src/fx.rs p/q/e/ie (+dual/geq).
  type FxExtra = Partial<FxParDef>;
  const P = (
    par: number,
    label: string,
    unit: string,
    min: string,
    max: string,
    extra: FxExtra = {},
  ): FxParDef => ({ par, label, unit, min, max, ...extra });
  const E = (par: number, label: string, options: string[], extra: FxExtra = {}): FxParDef => ({
    par,
    label,
    unit: '',
    min: '',
    max: '',
    options,
    ...extra,
  });
  const Q = (par: number, label: string, unit: string, min: string, max: string): FxParDef =>
    P(par, label, unit, min, max, { inferred: true });
  const IE = (par: number, label: string, options: string[]): FxParDef =>
    E(par, label, options, { inferred: true });
  /** Dual-block effect: A half pars 1..h, B half h+1..2h (cf. fx.rs dual). */
  const DUAL = (base: FxParDef[]): FxParDef[] => [
    ...base.map((d) => ({ ...d, bank: 'A' })),
    ...base.map((d) => ({ ...d, par: d.par + base.length, bank: 'B' })),
  ];
  const ALL_INFERRED = (base: FxParDef[]): FxParDef[] =>
    base.map((d) => ({ ...d, inferred: true }));

  const OFF_ON = ['OFF', 'ON'];
  const ZERO_ONE = ['0', '1'];
  const NOTE_DIV9 = ['1/4', '3/8', '1/2', '2/3', '1', '4/3', '3/2', '2', '3'];
  const PATTERN14 = [
    '1/4', '1/3', '3/8', '1/2', '2/3', '3/4', '1',
    '1/4X', '1/3X', '3/8X', '1/2X', '2/3X', '3/4X', '1X',
  ];
  const MODD_FACTOR = ['1', '1/2', '2/3', '3/2'];
  const MODD_SETUP = ['PAR', 'SER'];
  const MODD_RTYPE = ['AMB', 'CLUB', 'HALL'];
  const VINTAGE_OUT = ['FRONT', 'REAR'];
  const DLY_MODE = ['ST', 'X', 'M'];
  const FILT_MODE = ['LP', 'HP', 'BP', 'NO'];
  const FILT_WAVE = ['TRI', 'SIN', 'SAW', 'SAW-', 'RMP', 'SQU', 'RND'];
  const SUB_RANGE = ['LO', 'MID', 'HI'];
  const P1A_LOFREQ = ['0', '30', '60', '100'];
  const P1A_HIFREQ = ['3k', '4k', '5k', '8k', '10k', '12k', '16k'];
  const P1A_ATTFR = ['5k', '10k', '20k'];
  const PQ5_LOFREQ = ['200', '300', '500', '700', '1000'];
  const PQ5_MIDFREQ = ['200', '300', '500', '700', '1k', '1k5', '2k', '3k', '4k', '5k', '7k'];
  const PQ5_HIFREQ = ['1k5', '2k', '3k', '4k', '5k'];
  const ULC_RATIO = ['4', '8', '12', '20', 'ALL'];
  const CMB_SOLO = ['OFF', 'Bd1', 'Bd2', 'Bd3', 'Bd4', 'Bd5'];
  const CMB_SLOPE = ['12', '48'];
  const CMB_RATIO = [
    '1.1', '1.2', '1.3', '1.5', '1.7', '2', '2.5', '3', '3.5', '4', '5', '7', '10', 'LIM',
  ];
  const CMB_METER = ['GR', 'SBC', 'PEAK'];
  const LEC_MODE = ['COMP', 'LIM'];
  const DES_VOICE = ['FEM', 'MALE'];
  const DES_MODE = ['ST', 'M-S'];
  const DIMC_MODE = ['M', 'ST'];

  const GEQ_BANDS = [
    '20', '25', '31.5', '40', '50', '63', '80', '100', '125', '160', '200', '250', '315',
    '400', '500', '630', '800', '1k', '1k25', '1k6', '2k', '2k5', '3k15', '4k', '5k',
    '6k3', '8k', '10k', '12k5', '16k', '20k',
  ];
  const geq = (bank?: string): FxParDef[] => [
    ...GEQ_BANDS.map((f, i) => P(i + 1, f, 'Hz', '-15', '+15', bank ? { bank } : {})),
    P(32, 'Lvl', 'dB', '-15', '+15', bank ? { bank } : {}),
  ];
  const geq2 = (): FxParDef[] => [
    ...geq('A'),
    ...geq('B').map((d) => ({ ...d, par: d.par + 32 })),
  ];

  const HALL: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.2', '5'),
    P(3, 'Size', '', '2', '100'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '', '1', '30'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'BassMult', '', '0.5', '2'), P(10, 'Spread', '', '0', '50'),
    P(11, 'Shape', '', '0', '250'), P(12, 'ModSpeed', '', '0', '100'),
  ];
  const AMBI: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.2', '7.3'),
    P(3, 'Size', '', '2', '100'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '', '1', '30'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'Modulate', '%', '0', '100'), P(10, 'TailGain', '%', '0', '100'),
  ];
  const RPLT: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.3', '29'),
    P(3, 'Size', 'm', '4', '39'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '%', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'BassMult', '', '0.25', '4'), P(10, 'Spread', '', '0', '50'),
    P(11, 'Attack', '', '0', '100'), P(12, 'ModSpin', '%', '0', '100'),
    P(13, 'EchoLeft', 'ms', '0', '1200'), P(14, 'EchoRight', 'ms', '0', '1200'),
    P(15, 'EchoFbL', '%', '-100', '+100'), P(16, 'EchoFbR', '%', '-100', '+100'),
  ];
  const ROOM: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.3', '29'),
    P(3, 'Size', 'm', '4', '76'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '%', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'BassMult', '', '0.25', '4'), P(10, 'Spread', '', '0', '50'),
    P(11, 'Shape', '', '0', '250'), P(12, 'ModSpin', '%', '0', '100'),
    P(13, 'EchoLeft', 'ms', '0', '1200'), P(14, 'EchoRight', 'ms', '0', '1200'),
    P(15, 'EchoFbL', '%', '-100', '+100'), P(16, 'EchoFbR', '%', '-100', '+100'),
  ];
  const CHAM: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.3', '29'),
    P(3, 'Size', 'm', '4', '76'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '%', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'BassMult', '', '0.25', '4'), P(10, 'Spread', '', '0', '50'),
    P(11, 'Shape', '', '0', '250'), P(12, 'ModSpin', '%', '0', '100'),
    P(13, 'ERLeft', 'ms', '0', '500'), P(14, 'ERRight', 'ms', '0', '500'),
    P(15, 'ERLevelL', '%', '0', '100'), Q(16, 'ERLevelR', '%', '0', '100'),
  ];
  const PLAT: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.5', '10'),
    P(3, 'Size', '', '2', '100'), P(4, 'Damping', 'Hz', '1k', '20k'),
    P(5, 'Diffusion', '', '1', '30'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'BassMult', '', '0.5', '2'), P(10, 'Xover', 'Hz', '10', '500'),
    P(11, 'ModDepth', '', '1', '50'), P(12, 'ModSpeed', '', '0', '100'),
  ];
  const VREV: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '120'), P(2, 'Decay', 's', '0.4', '4.5'),
    P(3, 'Modulate', '%', '0', '100'), E(4, 'Vintage', OFF_ON), E(5, 'Output', VINTAGE_OUT),
    P(6, 'Level', 'dB', '-12', '+12'), P(7, 'LoCut', 'Hz', '10', '500'),
    P(8, 'HiCut', 'Hz', '10k', '20k'), P(9, 'LoMult', '', '0.5', '2'),
    P(10, 'HiMult', '', '0.25', '1'),
  ];
  const VRM: FxParDef[] = [
    P(1, 'RevDelay', 'ms', '0', '200'), P(2, 'Decay', 's', '0.1', '20'),
    P(3, 'Size', '', '2', '100'), P(4, 'Density', '', '1', '30'),
    P(5, 'ERLevel', '%', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoMult', '', '0.1', '10'), P(8, 'HiMult', '', '0.1', '10'),
    P(9, 'LoCut', 'Hz', '10', '500'), P(10, 'HiCut', 'Hz', '200', '20k'),
    P(11, 'ERLeft', 'ms', '0', '200'), P(12, 'ERRight', 'ms', '0', '200'),
    E(13, 'Freeze', OFF_ON),
  ];
  const GATE: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 'ms', '140', '1000'),
    P(3, 'Attack', '', '0', '30'), Q(4, 'Density', '', '1', '50'),
    P(5, 'Spread', '', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiSvFreq', 'Hz', '200', '20k'),
    P(9, 'HiSvGain', 'dB', '-30', '0'), P(10, 'Diffusion', '', '1', '30'),
  ];
  const RVRS: FxParDef[] = [
    P(1, 'PreDelay', 'ms', '0', '200'), P(2, 'Decay', 'ms', '140', '1000'),
    P(3, 'Rise', '', '0', '50'), P(4, 'Diffusion', '', '1', '30'),
    P(5, 'Spread', '', '0', '100'), P(6, 'Level', 'dB', '-12', '+12'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiSvFreq', 'Hz', '200', '20k'),
    P(9, 'HiSvGain', 'dB', '-30', '0'),
  ];
  const DLY: FxParDef[] = [
    P(1, 'Mix', '%', '0', '100'), P(2, 'Time', 'ms', '1', '3000'),
    E(3, 'Mode', DLY_MODE), E(4, 'FactorL', NOTE_DIV9), E(5, 'FactorR', NOTE_DIV9),
    P(6, 'Offset', 'ms', '-100', '+100'), P(7, 'LC', 'Hz', '10', '500'),
    P(8, 'HC', 'Hz', '200', '20k'), P(9, 'FeedLC', 'Hz', '10', '500'),
    P(10, 'FeedL', '%', '0', '100'), P(11, 'FeedR', '%', '0', '100'),
    P(12, 'FeedHC', 'Hz', '200', '20k'),
  ];
  const TAP3: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), P(2, 'GainT', '%', '0', '100'),
    P(3, 'PanT', '%', '-100', '+100'), P(4, 'Feedback', '%', '0', '100'),
    P(5, 'LC', 'Hz', '10', '500'), P(6, 'HC', 'Hz', '200', '20k'),
    E(7, 'FactorA', NOTE_DIV9), P(8, 'GainA', '%', '0', '100'),
    P(9, 'PanA', '%', '-100', '+100'), E(10, 'FactorB', NOTE_DIV9),
    P(11, 'GainB', '%', '0', '100'), P(12, 'PanB', '%', '-100', '+100'),
    E(13, 'X-Feed', OFF_ON), E(14, 'Mono', OFF_ON), E(15, 'Dry', OFF_ON),
  ];
  const TAP4: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), P(2, 'Gain', '%', '0', '100'),
    P(3, 'Feedback', '%', '0', '100'), P(4, 'LC', 'Hz', '10', '500'),
    P(5, 'HC', 'Hz', '200', '20k'), P(6, 'Spread', '', '0', '6'),
    E(7, 'FactorA', NOTE_DIV9), P(8, 'GainA', '%', '0', '100'),
    E(9, 'FactorB', NOTE_DIV9), P(10, 'GainB', '%', '0', '100'),
    E(11, 'FactorC', NOTE_DIV9), P(12, 'GainC', '%', '0', '100'),
    E(13, 'X-Feed', OFF_ON), E(14, 'Mono', OFF_ON), E(15, 'Dry', OFF_ON),
  ];
  const CRS: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '5'), P(2, 'DepthL', '%', '0', '100'),
    P(3, 'DepthR', '%', '0', '100'), P(4, 'DelayL', 'ms', '0.5', '50'),
    P(5, 'DelayR', 'ms', '0.5', '50'), P(6, 'Mix', '%', '0', '100'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'Phase', '', '0', '180'), P(10, 'Wave', '%', '0', '100'),
    P(11, 'Spread', '%', '0', '100'),
  ];
  const FLNG: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '5'), P(2, 'DepthL', '%', '0', '100'),
    P(3, 'DepthR', '%', '0', '100'), P(4, 'DelayL', 'ms', '0.5', '20'),
    P(5, 'DelayR', 'ms', '0.5', '20'), P(6, 'Mix', '%', '0', '100'),
    P(7, 'LoCut', 'Hz', '10', '500'), P(8, 'HiCut', 'Hz', '200', '20k'),
    P(9, 'Phase', '', '0', '180'), P(10, 'FeedLC', 'Hz', '10', '500'),
    P(11, 'FeedHC', 'Hz', '200', '20k'), P(12, 'Feed', '%', '-90', '+90'),
  ];
  const PHAS: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '5'), P(2, 'Depth', '%', '0', '100'),
    P(3, 'Reso', '%', '0', '80'), P(4, 'Base', '', '0', '50'),
    P(5, 'Stages', '', '2', '12'), P(6, 'Mix', '%', '0', '70'),
    P(7, 'Wave', '', '-50', '+50'), P(8, 'Phase', '', '0', '180'),
    P(9, 'EnvMod', '%', '-100', '+100'), P(10, 'Attack', 'ms', '10', '1000'),
    P(11, 'Hold', 'ms', '1', '2000'), P(12, 'Release', 'ms', '10', '1000'),
  ];
  const DIMC: FxParDef[] = [
    E(1, 'On', OFF_ON), E(2, 'M/St', DIMC_MODE), E(3, 'Dry', OFF_ON),
    E(4, 'Sw1', OFF_ON), E(5, 'Sw2', OFF_ON), E(6, 'Sw3', OFF_ON), E(7, 'Sw4', OFF_ON),
  ];
  const FILT: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '20'), P(2, 'Depth', '%', '0', '100'),
    P(3, 'Reso', '%', '0', '100'), P(4, 'Base', 'Hz', '20', '15k'),
    E(5, 'Mode', FILT_MODE), P(6, 'Mix', '%', '0', '100'),
    E(7, 'Wave', FILT_WAVE), P(8, 'Phase', '', '0', '180'),
    P(9, 'EnvMod', '%', '-100', '+100'), P(10, 'Attack', 'ms', '10', '250'),
    P(11, 'Release', 'ms', '10', '500'), P(12, 'Drive', '%', '0', '100'),
    E(13, '4pole', OFF_ON), E(14, 'SideChain', OFF_ON),
  ];
  const ROTA: FxParDef[] = [
    P(1, 'LoSpeed', 'Hz', '0.1', '4'), P(2, 'HiSpeed', 'Hz', '2', '10'),
    P(3, 'Accel', '%', '0', '100'), P(4, 'Distance', '%', '0', '100'),
    P(5, 'Balance', '%', '-100', '+100'), P(6, 'Mix', '%', '0', '100'),
    E(7, 'StopSW', OFF_ON), E(8, 'SpeedSW', OFF_ON),
  ];
  const PANFX: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '5'), P(2, 'Phase', '', '0', '180'),
    P(3, 'Wave', '', '-50', '+50'), P(4, 'Depth', '%', '0', '100'),
    P(5, 'EnvSpd', '%', '0', '100'), P(6, 'EnvDepth', '%', '0', '100'),
    P(7, 'Attack', 'ms', '10', '1000'), P(8, 'Hold', 'ms', '1', '2000'),
    P(9, 'Release', 'ms', '10', '1000'),
  ];
  const SUB5: FxParDef[] = [
    E(1, 'On', OFF_ON), E(2, 'Range', SUB_RANGE),
    P(3, 'Dry', '%', '0', '100'), P(4, 'Oct1', '%', '0', '100'), P(5, 'Oct2', '%', '0', '100'),
  ];
  const DRV_COMBO: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), E(2, 'Pattern', PATTERN14),
    P(3, 'FeedHC', 'Hz', '1k', '20k'), P(4, 'Feedback', '%', '0', '100'),
    P(5, 'X-Feed', '%', '0', '100'), P(6, 'Balance', '', '-100', '+100'),
    P(7, 'PreDelay', 'ms', '0', '200'), P(8, 'Decay', 's', '0.1', '5'),
    P(9, 'Size', '', '2', '100'), P(10, 'Damp', 'Hz', '1k', '20k'),
    P(11, 'LoCut', 'Hz', '10', '500'), P(12, 'Mix', '%', '0', '100'),
  ];
  const CRR: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '4'), P(2, 'Depth', '%', '0', '100'),
    P(3, 'Delay', 'ms', '0.5', '50'), P(4, 'Phase', '', '0', '180'),
    P(5, 'Wave', '%', '0', '100'), P(6, 'Balance', '', '-100', '+100'),
    P(7, 'PreDelay', 'ms', '0', '200'), P(8, 'Decay', 's', '0.1', '5'),
    P(9, 'Size', '', '2', '100'), P(10, 'Damp', 'Hz', '1k', '20k'),
    P(11, 'LoCut', 'Hz', '10', '500'), P(12, 'Mix', '%', '0', '100'),
  ];
  const FLR: FxParDef[] = [
    P(1, 'Speed', 'Hz', '0.05', '4'), P(2, 'Depth', '%', '0', '100'),
    P(3, 'Delay', 'ms', '0.5', '20'), P(4, 'Phase', '', '0', '180'),
    P(5, 'Feed', '%', '-90', '+90'), P(6, 'Balance', '', '-100', '+100'),
    P(7, 'PreDelay', 'ms', '0', '200'), P(8, 'Decay', 's', '0.1', '5'),
    P(9, 'Size', '', '2', '100'), P(10, 'Damp', 'Hz', '1k', '20k'),
    P(11, 'LoCut', 'Hz', '10', '500'), P(12, 'Mix', '%', '0', '100'),
  ];
  const DCR: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), E(2, 'Pattern', PATTERN14),
    P(3, 'FeedHC', 'Hz', '1k', '20k'), P(4, 'Feedback', '%', '0', '100'),
    P(5, 'X-Feed', '%', '0', '100'), P(6, 'Balance', '', '-100', '+100'),
    P(7, 'Speed', 'Hz', '0.05', '4'), P(8, 'Depth', '%', '0', '100'),
    P(9, 'Delay', 'ms', '0.5', '50'), P(10, 'Phase', '', '0', '180'),
    P(11, 'Wave', '%', '0', '100'), P(12, 'Mix', '%', '0', '100'),
  ];
  const DFL: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), E(2, 'Pattern', PATTERN14),
    P(3, 'FeedHC', 'Hz', '1k', '20k'), P(4, 'Feedback', '%', '0', '100'),
    P(5, 'X-Feed', '%', '0', '100'), P(6, 'Balance', '', '-100', '+100'),
    P(7, 'Speed', 'Hz', '0.05', '4'), P(8, 'Depth', '%', '0', '100'),
    P(9, 'Delay', 'ms', '0.5', '20'), P(10, 'Phase', '', '0', '180'),
    P(11, 'Feed', '%', '-90', '+90'), P(12, 'Mix', '%', '0', '100'),
  ];
  const MODD: FxParDef[] = [
    P(1, 'Time', 'ms', '1', '3000'), E(2, 'Factor', MODD_FACTOR),
    P(3, 'Feedback', '%', '0', '100'), P(4, 'FeedLC', 'Hz', '10', '500'),
    P(5, 'FeedHC', 'Hz', '1k', '20k'), P(6, 'Depth', '%', '0', '100'),
    P(7, 'Speed', 'Hz', '0.05', '10'), E(8, 'Serial', MODD_SETUP), E(9, 'Rtype', MODD_RTYPE),
    P(10, 'Decay', '', '1', '10'), P(11, 'Damp', 'Hz', '1k', '20k'),
    P(12, 'Balance', '', '-100', '+100'), P(13, 'Mix', '%', '0', '100'),
  ];
  const DES: FxParDef[] = [
    P(1, 'Low(M)', '', '0', '50'), P(2, 'High(M)', '', '0', '50'),
    P(3, 'Low S', '', '0', '50'), P(4, 'High S', '', '0', '50'),
    E(5, 'Male', DES_VOICE), E(6, 'M-S', DES_MODE),
  ];
  const DES2: FxParDef[] = [
    P(1, 'Low A', '', '0', '50'), P(2, 'High A', '', '0', '50'),
    P(3, 'Low B', '', '0', '50'), P(4, 'High B', '', '0', '50'),
    E(5, 'Male A', DES_VOICE), E(6, 'Male B', DES_VOICE),
  ];
  const P1A: FxParDef[] = [
    IE(1, 'On', OFF_ON), Q(2, 'Gain', 'dB', '-12', '+12'),
    Q(3, 'Lo Boost', '', '0', '10'), IE(4, 'Lo Freq', P1A_LOFREQ),
    Q(5, 'Lo Atten', '', '0', '10'), Q(6, 'Hi Width', '', '0', '10'),
    Q(7, 'Hi Boost', '', '0', '10'), IE(8, 'Hi Freq', P1A_HIFREQ),
    Q(9, 'Hi Atten', '', '0', '10'), IE(10, 'Atten Fr', P1A_ATTFR), IE(11, 'Transf', OFF_ON),
  ];
  const PQ5: FxParDef[] = [
    E(1, 'On', OFF_ON), P(2, 'Gain', 'dB', '-12', '+12'), E(3, 'Lo Freq', PQ5_LOFREQ),
    P(4, 'Lo Boost', '', '0', '10'), E(5, 'Mid Freq', PQ5_MIDFREQ),
    P(6, 'Mid Cut', '', '0', '10'), E(7, 'Hi Freq', PQ5_HIFREQ),
    P(8, 'Hi Boost', '', '0', '10'), E(9, 'Transf', OFF_ON),
  ];
  const WAVD: FxParDef[] = [
    P(1, 'AttackA', '%', '-100', '+100'), P(2, 'SustainA', '%', '-100', '+100'),
    P(3, 'OutGainA', 'dB', '-24', '+24'), P(4, 'AttackB', '%', '-100', '+100'),
    P(5, 'SustainB', '%', '-100', '+100'), P(6, 'OutGainB', 'dB', '-24', '+24'),
  ];
  const LIM: FxParDef[] = [
    P(1, 'InGain', 'dB', '0', '18'), P(2, 'OutGain', 'dB', '-18', '0'),
    P(3, 'Squeeze', '%', '0', '100'), P(4, 'Knee', 'dB', '0', '10'),
    P(5, 'Attack', 'ms', '0.05', '1'), P(6, 'Release', 'ms', '20', '2000'),
    E(7, 'STLink', OFF_ON), E(8, 'AutoGain', OFF_ON),
  ];
  const CMB: FxParDef[] = [
    E(1, 'On', OFF_ON), E(2, 'SoloBd', CMB_SOLO),
    P(3, 'Mix', '%', '0', '100'), P(4, 'Attack', '', '0', '19'),
    P(5, 'Release', 'ms', '20', '3000'), E(6, 'AutoRel', OFF_ON),
    P(7, 'SBC', '', '1', '10'), E(8, 'SBCOn', OFF_ON),
    P(9, 'XOSpc', '', '-50', '+50'), E(10, 'Slope', CMB_SLOPE), E(11, 'Ratio', CMB_RATIO),
    P(12, 'Thresh', 'dB', '-40', '0'), P(13, 'Gain', 'dB', '-10', '+10'),
    P(14, 'Bd1 Thr', 'dB', '-10', '+10'), P(15, 'Bd1 Gain', 'dB', '-10', '+10'),
    E(16, 'Bd1 Lock', ZERO_ONE),
    P(17, 'Bd2 Thr', 'dB', '-10', '+10'), P(18, 'Bd2 Gain', 'dB', '-10', '+10'),
    E(19, 'Bd2 Lock', ZERO_ONE),
    P(20, 'Bd3 Thr', 'dB', '-10', '+10'), P(21, 'Bd3 Gain', 'dB', '-10', '+10'),
    E(22, 'Bd3 Lock', ZERO_ONE),
    P(23, 'Bd4 Thr', 'dB', '-10', '+10'), P(24, 'Bd4 Gain', 'dB', '-10', '+10'),
    E(25, 'Bd4 Lock', ZERO_ONE),
    P(26, 'Bd5 Thr', 'dB', '-10', '+10'), P(27, 'Bd5 Gain', 'dB', '-10', '+10'),
    E(28, 'Bd5 Lock', ZERO_ONE), E(29, 'MeterSel', CMB_METER),
  ];
  const FAC: FxParDef[] = [
    E(1, 'On', OFF_ON), Q(2, 'InGain', 'dB', '-20', '0'),
    P(3, 'Threshold', '', '0', '10'), P(4, 'Time', '', '1', '6'),
    P(5, 'DCBias', '%', '0', '100'), P(6, 'OutGain', 'dB', '-18', '+6'),
    P(7, 'Balance', '%', '-100', '+100'),
  ];
  const LEC: FxParDef[] = [
    E(1, 'On', OFF_ON), P(2, 'Gain', '', '0', '100'), P(3, 'Peak Red', '', '0', '100'),
    E(4, 'Mode', LEC_MODE), P(5, 'OutGain', 'dB', '-18', '+6'),
  ];
  const ULC: FxParDef[] = [
    E(1, 'On', OFF_ON), P(2, 'InGain', 'dB', '-48', '0'), P(3, 'OutGain', 'dB', '-48', '0'),
    P(4, 'Attack', '', '1', '7'), P(5, 'Release', '', '1', '7'), E(6, 'Ratio', ULC_RATIO),
  ];
  const IMG: FxParDef[] = [
    P(1, 'Balance', '%', '-100', '+100'), P(2, 'MonoPan', '%', '-100', '+100'),
    P(3, 'StereoPan', '%', '-100', '+100'), P(4, 'ShvGain', 'dB', '0', '12'),
    P(5, 'ShvFreq', 'Hz', '100', '1000'), P(6, 'ShvQ', '', '1', '10'),
    P(7, 'OutGain', 'dB', '-12', '+12'),
  ];
  const EDI: FxParDef[] = [
    E(1, 'On', OFF_ON), E(2, 'In ST-MS', DES_MODE), E(3, 'Out ST-MS', DES_MODE),
    P(4, 'StSpread', '', '-50', '+50'), P(5, 'LMFSpread', '', '-50', '+50'),
    P(6, 'Balance', '', '-50', '+50'), P(7, 'CntrDist', '', '-50', '+50'),
    P(8, 'Gain', 'dB', '-12', '+12'),
  ];
  const SON4: FxParDef[] = [
    E(1, 'On', OFF_ON), P(2, 'LoContour', '', '0', '10'),
    Q(3, 'Process', '', '0', '10'), P(4, 'Gain', 'dB', '-12', '+12'),
  ];
  const EXC: FxParDef[] = [
    P(1, 'Tune', 'Hz', '1k', '10k'), P(2, 'Peak', '%', '0', '100'),
    P(3, 'ZeroFill', '%', '0', '100'), P(4, 'Timbre', '', '-50', '+50'),
    P(5, 'Harmonics', '%', '0', '100'), P(6, 'Mix', '%', '0', '100'),
    E(7, 'Solo', OFF_ON),
  ];
  const ENH: FxParDef[] = [
    P(1, 'OutGain', 'dB', '-12', '+12'), Q(2, 'Spread', '%', '0', '100'),
    P(3, 'BassGain', '%', '0', '100'), P(4, 'BassFreq', '', '1', '50'),
    P(5, 'MidGain', '%', '0', '100'), Q(6, 'MidQ', '', '1', '50'),
    P(7, 'HiGain', '%', '0', '100'), P(8, 'HiFreq', '', '1', '50'),
    E(9, 'Solo', OFF_ON),
  ];
  const ENH2: FxParDef[] = [
    Q(1, 'OutGain', 'dB', '-12', '+12'), Q(2, 'BassGain', '%', '0', '100'),
    Q(3, 'BassFreq', '', '1', '50'), Q(4, 'MidGain', '%', '0', '100'),
    Q(5, 'MidQ', '', '1', '50'), Q(6, 'HiGain', '%', '0', '100'),
    Q(7, 'HiFreq', '', '1', '50'), IE(8, 'Solo', OFF_ON),
    Q(9, 'OutGain', 'dB', '-12', '+12'), Q(10, 'BassGain', '%', '0', '100'),
    Q(11, 'BassFreq', '', '1', '50'), Q(12, 'MidGain', '%', '0', '100'),
    Q(13, 'MidQ', '', '1', '50'), Q(14, 'HiGain', '%', '0', '100'),
    Q(15, 'HiFreq', '', '1', '50'), IE(16, 'Solo', OFF_ON),
  ];
  const AMP: FxParDef[] = [
    P(1, 'PreAmp', '', '0', '10'), P(2, 'Buzz', '', '0', '10'),
    P(3, 'Punch', '', '0', '10'), P(4, 'Crunch', '', '0', '10'),
    P(5, 'Drive', '', '0', '10'), Q(6, 'Level', '', '0', '10'),
    Q(7, 'Low', '', '0', '10'), Q(8, 'High', '', '0', '10'),
    E(9, 'Cabinet', OFF_ON),
  ];
  const DRV: FxParDef[] = [
    P(1, 'Drive', '%', '0', '100'), P(2, 'Even', '%', '0', '50'),
    P(3, 'Odd', '%', '0', '50'), P(4, 'Gain', 'dB', '-12', '+12'),
    P(5, 'LoCut', 'Hz', '20', '200'), P(6, 'HiCut', 'Hz', '4k', '20k'),
    P(7, 'LoGain', 'dB', '-12', '+12'), P(8, 'LoFreq', 'Hz', '50', '400'),
    P(9, 'HiGain', 'dB', '-12', '+12'), P(10, 'HiFreq', 'Hz', '1k', '10k'),
  ];
  const PIT2: FxParDef[] = [
    Q(1, 'Semi1', '', '-12', '+12'), Q(2, 'Cent1', '', '-50', '+50'),
    Q(3, 'Delay1', 'ms', '1', '500'), Q(4, 'Gain1', '%', '0', '100'),
    Q(5, 'Pan1', '%', '-100', '+100'), Q(6, 'Mix', '%', '0', '100'),
    Q(7, 'Semi2', '', '-12', '+12'), Q(8, 'Cent2', '', '-50', '+50'),
    Q(9, 'Delay2', 'ms', '1', '500'), Q(10, 'Gain2', '%', '0', '100'),
    Q(11, 'Pan2', '%', '-100', '+100'), Q(12, 'HiCut', 'Hz', '2k', '20k'),
  ];
  const PIT: FxParDef[] = [
    P(1, 'Semi', '', '-12', '+12'), P(2, 'Cent', '', '-50', '+50'),
    Q(3, 'Delay', 'ms', '1', '500'), P(4, 'LoCut', 'Hz', '10', '500'),
    P(5, 'HiCut', 'Hz', '2k', '20k'), P(6, 'Mix', '%', '0', '100'),
  ];

  // Wire order (cf. fx.rs fx_effect_meta); index 15 is an undocumented gap.
  const EFFECTS: FxEffectDef[] = [
    { index: 0, name: 'HALL', desc: 'hall reverb', pars: HALL },
    { index: 1, name: 'AMBI', desc: 'ambience', pars: AMBI },
    { index: 2, name: 'RPLT', desc: 'rich plate', pars: RPLT },
    { index: 3, name: 'ROOM', desc: 'room', pars: ROOM },
    { index: 4, name: 'CHAM', desc: 'chamber', pars: CHAM },
    { index: 5, name: 'PLAT', desc: 'plate', pars: PLAT },
    { index: 6, name: 'VREV', desc: 'vintage reverb', pars: VREV },
    { index: 7, name: 'VRM', desc: 'vintage room', pars: VRM },
    { index: 8, name: 'GATE', desc: 'gated reverb', pars: GATE },
    { index: 9, name: 'RVRS', desc: 'reverse reverb', pars: RVRS },
    { index: 10, name: 'DLY', desc: 'stereo delay', pars: DLY },
    { index: 11, name: '3TAP', desc: '3-tap delay', pars: TAP3 },
    { index: 12, name: '4TAP', desc: '4-tap delay', pars: TAP4 },
    { index: 13, name: 'CRS', desc: 'stereo chorus', pars: CRS },
    { index: 14, name: 'FLNG', desc: 'stereo flanger', pars: FLNG },
    { index: 16, name: 'PHAS', desc: 'stereo phaser', pars: PHAS },
    { index: 17, name: 'DIMC', desc: 'dimensional chorus', pars: DIMC },
    { index: 18, name: 'FILT', desc: 'mood filter', pars: FILT },
    { index: 19, name: 'ROTA', desc: 'rotary speaker', pars: ROTA },
    { index: 20, name: 'PAN', desc: 'tremolo/panner', pars: PANFX },
    { index: 21, name: 'SUB', desc: 'sub-octaver', pars: DUAL(SUB5) },
    { index: 22, name: 'D/RV', desc: 'delay+chamber', pars: DRV_COMBO },
    { index: 23, name: 'CR/R', desc: 'chorus+chamber', pars: CRR },
    { index: 24, name: 'FL/R', desc: 'flanger+chamber', pars: FLR },
    { index: 25, name: 'D/CR', desc: 'delay+chorus', pars: DCR },
    { index: 26, name: 'D/FL', desc: 'delay+flanger', pars: DFL },
    { index: 27, name: 'MODD', desc: 'modulation delay', pars: MODD },
    { index: 28, name: 'GEQ2', desc: 'dual graphic EQ', pars: geq2() },
    { index: 29, name: 'GEQ', desc: 'stereo graphic EQ', pars: geq() },
    { index: 30, name: 'TEQ2', desc: 'true dual graphic EQ', pars: geq2() },
    { index: 31, name: 'TEQ', desc: 'true stereo graphic EQ', pars: geq() },
    { index: 32, name: 'DES2', desc: 'dual de-esser', pars: DES2 },
    { index: 33, name: 'DES', desc: 'stereo de-esser', pars: DES },
    { index: 34, name: 'P1A', desc: 'program EQ', pars: P1A },
    { index: 35, name: 'P1A2', desc: 'dual program EQ', pars: DUAL(P1A) },
    { index: 36, name: 'PQ5', desc: 'midrange EQ', pars: PQ5 },
    { index: 37, name: 'PQ5S', desc: 'dual midrange EQ', pars: DUAL(PQ5) },
    { index: 38, name: 'WAVD', desc: 'wave designer', pars: WAVD },
    { index: 39, name: 'LIM', desc: 'precision limiter', pars: LIM },
    { index: 40, name: 'CMB', desc: 'combinator', pars: CMB },
    { index: 41, name: 'CMB2', desc: 'dual combinator', pars: DUAL(CMB) },
    { index: 42, name: 'FAC', desc: 'fair compressor', pars: FAC },
    { index: 43, name: 'FAC1M', desc: 'M/S fair compressor', pars: ALL_INFERRED(DUAL(FAC)) },
    { index: 44, name: 'FAC2', desc: 'dual fair compressor', pars: DUAL(FAC) },
    { index: 45, name: 'LEC', desc: 'leisure compressor', pars: LEC },
    { index: 46, name: 'LEC2', desc: 'dual leisure compressor', pars: DUAL(LEC) },
    { index: 47, name: 'ULC2', desc: 'dual ultimo compressor', pars: ALL_INFERRED(DUAL(ULC)) },
    { index: 48, name: 'ENH2', desc: 'dual enhancer', pars: ENH2 },
    { index: 49, name: 'ENH', desc: 'enhancer', pars: ENH },
    { index: 50, name: 'EXC2', desc: 'dual exciter', pars: DUAL(EXC) },
    { index: 51, name: 'EXC', desc: 'exciter', pars: EXC },
    { index: 52, name: 'IMG', desc: 'stereo imager', pars: IMG },
    { index: 53, name: 'EDI', desc: 'Edison EX1', pars: EDI },
    { index: 54, name: 'SON', desc: 'sound maxer', pars: DUAL(SON4) },
    { index: 55, name: 'AMP2', desc: 'dual guitar amp', pars: DUAL(AMP) },
    { index: 56, name: 'AMP', desc: 'guitar amp', pars: AMP },
    { index: 57, name: 'DRV2', desc: 'dual tube stage', pars: DUAL(DRV) },
    { index: 58, name: 'DRV', desc: 'tube stage', pars: DRV },
    { index: 59, name: 'PIT2', desc: 'dual pitch shifter', pars: PIT2 },
    { index: 60, name: 'PIT', desc: 'stereo pitch shifter', pars: PIT },
  ];

  /** Labeled effect for a `/fx/N/type` index, or null (generic fallback). */
  function effectByIndex(index: number): FxEffectDef | null {
    return EFFECTS.find((e) => e.index === index) ?? null;
  }

  const tell = (t: string): void => {
    say?.(t);
  };
  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

  /** UI field for (slot, par), or null when the mapping is unknown. */
  function fieldFor(slot: number, par: number): string | null {
    if (!Number.isInteger(slot) || slot < 1 || slot > 4) return null;
    if (!Number.isInteger(par) || par < 1 || par > PARAM_COUNT) return null;
    return `fx${slot}_par${String(par).padStart(2, '0')}`;
  }

  /** OSC address the field drives (title hints only). */
  function addrFor(slot: number, par: number): string {
    return `/fx/${slot}/par/${String(par).padStart(2, '0')}`;
  }

  // Which desk slot each card edits (default: card i -> slot i+1).
  let cardSlot: number[] = $state([1, 2, 3, 4]);
  // Per-card effect picker: null = generic P01–P08; otherwise the picker
  // only relabels sliders (it never writes /fx/N/type — no desk retune).
  let cardFx: (number | null)[] = $state([null, null, null, null]);
  // Local slider positions (the backend never echoes FX state, so these are
  // authoritative only after a touch). Key `${slot}:${par}`.
  let vals: Record<string, number> = $state({});
  // Slots the backend rejected: reason string disables the card's sliders.
  let dead: Record<number, string> = $state({});

  const key = (slot: number, par: number): string => `${slot}:${par}`;
  const pos = (slot: number, par: number): number => vals[key(slot, par)] ?? 0;

  function pickSlot(card: number, slot: number): void {
    cardSlot[card] = slot;
  }

  function pickFx(card: number, raw: string): void {
    cardFx[card] = raw === 'generic' ? null : Number(raw);
  }

  async function push(card: number, par: number, v01: number): Promise<void> {
    const slot = cardSlot[card] ?? card + 1;
    const field = fieldFor(slot, par);
    if (field === null) return; // unknown mapping — slider renders disabled
    vals[key(slot, par)] = v01;
    try {
      await backend.setParam(slot - 1, field, v01);
    } catch (e) {
      dead[slot] = errMsg(e);
      tell(`rack FX${slot} ${field}: ${errMsg(e)} (unsupported on this backend)`);
    }
  }

  type SliderEvent = Event & { currentTarget: EventTarget & HTMLInputElement };
  type SelectEvent = Event & { currentTarget: EventTarget & HTMLSelectElement };
</script>

<div class="rack">
  <div class="hint">Positions are local until touched — the backend never echoes FX state. First move pushes <code>fxN_parMM</code> (0..1) via the <code>param</code> command. The fx picker only relabels sliders — it never writes <code>/fx/N/type</code>, so it cannot retune the desk.</div>
  {#each [0, 1, 2, 3] as card (card)}
    {@const slot = cardSlot[card] ?? card + 1}
    {@const why = dead[slot]}
    {@const fxRaw = cardFx[card]}
    {@const fx = fxRaw === null || fxRaw === undefined ? null : effectByIndex(fxRaw)}
    {@const fxInferred = fx !== null && fx.pars.some((p) => p.inferred)}
    <section class="card" class:dead={why !== undefined} aria-label={`FX slot card ${card + 1}`}>
      <header>
        <b>FX{slot}</b>
        <label>slot
          <select value={slot} onchange={(e) => pickSlot(card, Number(e.currentTarget.value))} disabled={why !== undefined}>
            {#each [1, 2, 3, 4] as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
      </header>
      <label class="fxpick" title="Labels only — picking an effect never writes /fx/N/type (no desk retune)">fx
        <select value={fxRaw ?? 'generic'} onchange={(e: SelectEvent) => pickFx(card, e.currentTarget.value)} disabled={why !== undefined}>
          <option value="generic">generic · P01–P08</option>
          {#each EFFECTS as fxo (fxo.index)}<option value={fxo.index} title={fxo.desc}>{fxo.index} · {fxo.name}</option>{/each}
        </select>
      </label>
      {#if why !== undefined}
        <div class="deadnote" title={why}>offline — backend rejected FX ({why})</div>
      {/if}
      {#if fx === null}
        {#each Array.from({ length: KNOWN_PARS }, (_, i) => i + 1) as par (par)}
          {@const field = fieldFor(slot, par)}
          {@const fxId = `fx-${slot}-${par}`}
          <div class="prow">
            <label
              for={fxId}
              title={field === null ? 'TODO: unmapped FX param (no guessing)' : `${field} -> ${addrFor(slot, par)}`}
            >
              {field === null ? `P${String(par).padStart(2, '0')} TODO` : `P${String(par).padStart(2, '0')}`}
            </label>
            <input
              id={fxId}
              type="range"
              min="0"
              max="1000"
              step="1"
              value={Math.round(pos(slot, par) * 1000)}
              disabled={field === null || why !== undefined}
              title={field === null ? 'TODO: driver mapping unknown — disabled, no guessing' : `${field} -> ${addrFor(slot, par)}`}
              oninput={(e: SliderEvent) => void push(card, par, Number(e.currentTarget.value) / 1000)}
            />
            <output>{Math.round(pos(slot, par) * 100)}%</output>
          </div>
        {/each}
        <div class="prow more">
          <label for="fx-{slot}-rest" title="par 09..64 share the fxN_parMM pattern but have no effect-type labels yet — no guessing">par 09–{PARAM_COUNT} TODO</label>
          <input id="fx-{slot}-rest" type="range" min="0" max="1000" value="0" disabled title="TODO: effect-type labels unknown — same fxN_parMM pattern, not yet mapped (no guessing)" />
          <output>—</output>
        </div>
      {:else}
        <div class="fxname" title={fx.desc}>{fx.index} · {fx.name} — {fx.pars.length} pars</div>
        {#if fxInferred}
          <div class="infnote" title="Some rows are inferred from docs (see research_notes/FX param labels/fx-labels.md) — unverified on hardware">labels with ? are inferred — unverified, never asserted</div>
        {/if}
        {#each fx.pars as p (p.par)}
          {@const field = fieldFor(slot, p.par)}
          {@const fxId = `fx-${slot}-${p.par}`}
          {@const shown = `${p.label}${p.bank ? ' ' + p.bank : ''}${p.inferred ? '?' : ''}`}
          {@const range = p.options ? p.options.join('/') : `${p.min}–${p.max}${p.unit ? ' ' + p.unit : ''}`}
          <div class="prow">
            <label for={fxId} title={`${field} -> ${addrFor(slot, p.par)} · ${range}`}>{shown}</label>
            {#if p.options}
              {@const n = p.options.length}
              {@const sel = n <= 1 ? 0 : Math.min(n - 1, Math.max(0, Math.round(pos(slot, p.par) * (n - 1))))}
              {@const step = n <= 1 ? 0 : 1 / (n - 1)}
              <select
                id={fxId}
                value={sel}
                disabled={field === null || why !== undefined}
                title={`${field} -> ${addrFor(slot, p.par)} · ${range} (discrete steps over 0..1 — mapping assumed)`}
                onchange={(e: SelectEvent) => void push(card, p.par, Number(e.currentTarget.value) * step)}
              >
                {#each p.options as opt, i (opt)}<option value={i}>{opt}</option>{/each}
              </select>
              <output>{p.options[sel] ?? '—'}</output>
            {:else}
              <input
                id={fxId}
                type="range"
                min="0"
                max="1000"
                step="1"
                value={Math.round(pos(slot, p.par) * 1000)}
                disabled={field === null || why !== undefined}
                title={`${field} -> ${addrFor(slot, p.par)} · ${range}`}
                oninput={(e: SliderEvent) => void push(card, p.par, Number(e.currentTarget.value) / 1000)}
              />
              <output>{Math.round(pos(slot, p.par) * 100)}%</output>
            {/if}
          </div>
        {/each}
      {/if}
    </section>
  {/each}
</div>

<style>
  .rack {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
    padding: 10px;
  }
  .hint {
    grid-column: 1 / -1;
    font-size: 12px;
    color: #9aa4b2;
  }
  .hint code {
    color: #e2e8f0;
    background: #1a2230;
    padding: 0 4px;
    border-radius: 3px;
  }
  .card {
    background: #0f141c;
    border: 1px solid #1e293b;
    border-radius: 8px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .card.dead {
    opacity: 0.75;
    border-style: dashed;
  }
  .card header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 14px;
  }
  .card header b {
    font-size: 16px;
  }
  .card header label {
    font-size: 12px;
    color: #9aa4b2;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .fxpick {
    font-size: 12px;
    color: #9aa4b2;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .fxpick select {
    flex: 1;
    min-width: 0;
  }
  .fxname {
    font-size: 12px;
    color: #e2e8f0;
  }
  .infnote {
    font-size: 11px;
    color: #fbbf24;
  }
  .deadnote {
    font-size: 11px;
    color: #f87171;
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .prow label {
    width: 72px;
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #9aa4b2;
  }
  .prow input[type='range'] {
    flex: 1;
    min-width: 0;
  }
  .prow select {
    flex: 1;
    min-width: 0;
  }
  .prow output {
    min-width: 36px;
    text-align: right;
    color: #e2e8f0;
  }
  .prow.more {
    opacity: 0.6;
  }
  .prow.more label {
    width: auto;
    flex: 1;
  }
</style>
