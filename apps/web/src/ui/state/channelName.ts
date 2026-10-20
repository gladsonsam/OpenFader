/** Physical strip labels are UI fallbacks, never desk names to write back. */
export function physicalChannelName(id: string): string {
  if (id === 'aux1') return 'Aux';
  if (id === 'mainlr') return 'Main LR';
  if (id === 'mainc') return 'Main C';
  const match = /^(in|fx|fxs|mix|mtx|dca|grp)(\d+)$/.exec(id);
  if (!match) return 'Channel';
  const family: Record<string, string> = { in: 'Channel', fx: 'FX Return', fxs: 'FX Send', mix: 'Mix', mtx: 'Matrix', dca: 'DCA', grp: 'Group' };
  return `${family[match[1]!]} ${match[2]}`;
}
