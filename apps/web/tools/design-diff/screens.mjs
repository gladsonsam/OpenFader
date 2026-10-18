// Screen table for the design-diff harness.
// Each screen: how to reach it in the DESIGN (fresh page + real clicks),
// which fixture URL our page must serve, and which layout boxes to measure.
//
// Box matchers (all resolved against accessible names / stable text):
//   { kind: 'aria', value }          exact aria-label match
//   { kind: 'ariaPrefix', value }    aria-label startsWith
//   { kind: 'text', value }          button/region whose normalized innerText equals value
//   { kind: 'textPrefix', value }    ... startsWith (smallest-area match wins)
//   { kind: 'css', value }           querySelector
//
// Box rects are stored in docs/design/refs/<id>.boxes.json at --update-ref
// time (measured on the live design page) and compared 1px-tolerant.

const EQ = [{ clickAriaPrefix: "Open EQ for Ch 2" }];

const HEADER = [
  { name: "hdr-toolbar", match: { kind: "aria", value: "Console tools" } },
  { name: "hdr-fx", match: { kind: "aria", value: "Effects" } },
  { name: "hdr-routing", match: { kind: "aria", value: "Routing" } },
  { name: "hdr-connection", match: { kind: "ariaPrefix", value: "Connection" } },
  { name: "hdr-settings", match: { kind: "aria", value: "Settings" } },
  { name: "hdr-scene", match: { kind: "ariaPrefix", value: "Scene " } },
];

const MIXER_STRIPS = (ch) => [
  { name: "strip-mute", match: { kind: "aria", value: `Mute ${ch}` } },
  { name: "strip-fader", match: { kind: "ariaPrefix", value: `Fader ${ch} ` } },
  { name: "strip-select", match: { kind: "ariaPrefix", value: `Select ${ch}` } },
];

const MIXER_MASTER = [
  { name: "master-region", match: { kind: "aria", value: "Master strip" } },
  { name: "master-mute", match: { kind: "aria", value: "Mute Main LR" } },
];

const CHANNEL_STRIP = [
  { name: "chstrip-region", match: { kind: "aria", value: "Channel strip" } },
  { name: "chstrip-mute", match: { kind: "aria", value: "Mute Ch 2" } },
  { name: "chstrip-fader", match: { kind: "ariaPrefix", value: "Fader Ch 2 " } },
];

const BACK = [{ name: "hdr-back", match: { kind: "aria", value: "Back" } }];
const PROCESSING = [{ name: "processing-nav", match: { kind: "aria", value: "Processing" } }];

export const SCREENS = {
  mixer: {
    steps: [],
    ours: "screen=mixer",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      ...MIXER_STRIPS("Ch 1"),
      ...MIXER_MASTER,
    ],
  },
  "ch-eq": {
    steps: [...EQ],
    ours: "screen=ch-eq&ch=2",
    boxes: [
      ...HEADER,
      ...BACK,
      ...PROCESSING,
      { name: "tab-eq", match: { kind: "textPrefix", value: "HPF + EQ" } },
      { name: "eq-graph", match: { kind: "css", value: "[data-graph]" } },
      { name: "eq-knob-freq", match: { kind: "ariaPrefix", value: "Freq " } },
      ...CHANNEL_STRIP,
    ],
  },
  "ch-comp": {
    steps: [...EQ, { clickTextPrefix: "Compressor" }],
    ours: "screen=ch-comp&ch=2",
    boxes: [
      ...HEADER,
      ...BACK,
      ...PROCESSING,
      { name: "tab-comp", match: { kind: "textPrefix", value: "Compressor" } },
      { name: "dyn-curve", match: { kind: "aria", value: "Compressor transfer curve" } },
      { name: "dyn-toggle", match: { kind: "textPrefix", value: "Compressor " } },
      ...CHANNEL_STRIP,
    ],
  },
  "ch-gate": {
    steps: [...EQ, { clickTextPrefix: "Gate" }],
    ours: "screen=ch-gate&ch=2",
    boxes: [
      ...HEADER,
      ...BACK,
      ...PROCESSING,
      { name: "tab-gate", match: { kind: "textPrefix", value: "Gate" } },
      { name: "dyn-curve", match: { kind: "aria", value: "Gate transfer curve" } },
      { name: "dyn-toggle", match: { kind: "textPrefix", value: "Gate " } },
      ...CHANNEL_STRIP,
    ],
  },
  "ch-sends": {
    steps: [...EQ, { clickTextPrefix: "Sends" }],
    ours: "screen=ch-sends&ch=2",
    boxes: [
      ...HEADER,
      ...BACK,
      ...PROCESSING,
      { name: "tab-sends", match: { kind: "textPrefix", value: "Sends" } },
      { name: "send-row", match: { kind: "ariaPrefix", value: "Send to Mix 1 " } },
      { name: "send-pre", match: { kind: "aria", value: "Mix 1 pre-fader" } },
      ...CHANNEL_STRIP,
    ],
  },
  "ch-input": {
    steps: [...EQ, { clickTextPrefix: "Input" }],
    ours: "screen=ch-input&ch=2",
    boxes: [
      ...HEADER,
      ...BACK,
      ...PROCESSING,
      { name: "tab-input", match: { kind: "textPrefix", value: "Input" } },
      { name: "sec-preamp", match: { kind: "aria", value: "Preamp" } },
      { name: "sec-source", match: { kind: "aria", value: "Source" } },
      { name: "sec-delay", match: { kind: "aria", value: "Delay" } },
      ...CHANNEL_STRIP,
    ],
  },
  fx: {
    steps: [{ clickAria: "Effects" }],
    ours: "screen=fx",
    boxes: [
      ...HEADER,
      ...BACK,
      { name: "fx-type", match: { kind: "aria", value: "Effect type" } },
      { name: "fx-title", match: { kind: "text", value: "FX 1 · Hall" } },
      { name: "fx-slot1", match: { kind: "textPrefix", value: "FX 1" } },
      { name: "fx-knob-decay", match: { kind: "ariaPrefix", value: "Decay " } },
    ],
  },
  routing: {
    steps: [{ clickAria: "Routing" }],
    ours: "screen=routing",
    boxes: [
      ...HEADER,
      ...BACK,
      { name: "sec-channels", match: { kind: "aria", value: "Channels" } },
      { name: "sec-sources", match: { kind: "aria", value: "Sources" } },
      { name: "route-bank", match: { kind: "text", value: "1–16" } },
    ],
  },
  connection: {
    steps: [{ clickAriaPrefix: "Connection" }],
    ours: "screen=connection",
    boxes: [
      ...HEADER,
      ...BACK,
      { name: "conn-status", match: { kind: "aria", value: "Connection status" } },
      { name: "conn-scan", match: { kind: "text", value: "Scan" } },
    ],
  },
  settings: {
    steps: [{ clickAria: "Settings" }],
    ours: "screen=settings",
    boxes: [
      ...HEADER,
      ...BACK,
      { name: "sec-display", match: { kind: "aria", value: "Display" } },
      { name: "sec-faders", match: { kind: "aria", value: "Faders" } },
      { name: "sec-safety", match: { kind: "aria", value: "Safety" } },
      { name: "sec-about", match: { kind: "aria", value: "About" } },
      { name: "grp-screen-lock", match: { kind: "aria", value: "Screen lock" } },
    ],
  },
  scenes: {
    steps: [{ clickAriaPrefix: "Scene " }],
    ours: "screen=scenes",
    boxes: [
      ...HEADER,
      ...BACK,
      { name: "scenes-recall", match: { kind: "text", value: "Recall" } },
      { name: "scenes-card1", match: { kind: "textPrefix", value: "001" } },
    ],
  },
  "mixer-locked": {
    steps: [
      { clickAria: "Settings" },
      { clickInGroup: { group: "Screen lock", text: "On" } },
      { clickAria: "Back" },
    ],
    ours: "screen=mixer&locked=1",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      { name: "locked-banner", match: { kind: "textPrefix", value: "Locked" } },
      ...MIXER_STRIPS("Ch 1"),
      ...MIXER_MASTER,
    ],
  },
  "mixer-sof": {
    steps: [
      { clickAria: "Settings" },
      { clickInGroup: { group: "Sends on faders", text: "On" } },
      { clickAria: "Back" },
    ],
    ours: "screen=mixer&sof=1",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      { name: "sof-sub", match: { kind: "text", value: "Aux bus" } },
      { name: "strip-mute", match: { kind: "aria", value: "Mute Ch 1" } },
      { name: "strip-fader", match: { kind: "ariaPrefix", value: "Send to Mix 1 from Ch 1" } },
      { name: "strip-select", match: { kind: "ariaPrefix", value: "Select Ch 1" } },
      { name: "master-region", match: { kind: "aria", value: "Master strip" } },
      { name: "master-mute", match: { kind: "aria", value: "Mute Mix 1" } },
    ],
  },
  "mixer-fine": {
    steps: [
      { clickAria: "Settings" },
      { clickInGroup: { group: "Fine control", text: "On" } },
      { clickAria: "Back" },
    ],
    ours: "screen=mixer&fine=1",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      ...MIXER_STRIPS("Ch 1"),
      ...MIXER_MASTER,
    ],
  },
  "mixer-bank-mix": {
    steps: [{ clickText: "Mix" }],
    ours: "screen=mixer&bank=mix",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      ...MIXER_STRIPS("Mix 1"),
      ...MIXER_MASTER,
    ],
  },
  "mixer-bank-dca": {
    steps: [{ clickText: "DCA" }],
    ours: "screen=mixer&bank=dca",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      ...MIXER_STRIPS("DCA 1"),
      ...MIXER_MASTER,
    ],
  },
  "mixer-bank-mtx": {
    steps: [{ clickText: "Mtx/Main" }],
    ours: "screen=mixer&bank=mtx",
    boxes: [
      ...HEADER,
      { name: "layers-nav", match: { kind: "aria", value: "Layers" } },
      ...MIXER_STRIPS("Mtx 1"),
      ...MIXER_MASTER,
    ],
  },
};

export const SCREEN_IDS = Object.keys(SCREENS);
