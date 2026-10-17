// Fader <-> dB readout. Same curve as the vanilla UI.
export function faderDb(v) {
  if (v <= 0.0005) return '-inf';
  const db = 20 * Math.log10(v);
  return `${db >= 0 ? '+' : ''}${db.toFixed(1)}`;
}
