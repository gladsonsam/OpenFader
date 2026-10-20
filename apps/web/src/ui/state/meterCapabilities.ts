import type { World } from '../world/types.ts';

/** Verified X-Air taps: ch 1–16 and the single stereo aux family.
 * The backend maps blob 0–15 to inputs and blob 26 to aux. Resolve the
 * advertised topology here rather than duplicating global strip indices.
 * Other drivers retain their existing meter behavior.
 */
export function hasLiveLevel(world: World, ch: number): boolean {
  if (world.driver === 'dl32s') return false; // level wire path not implemented
  if (world.driver !== 'xair') return true;
  if (!Number.isInteger(ch) || ch < 0 || ch >= world.stripCount) return false;
  const kind = world.kindOf(ch);
  if (kind === 'in') return /^ch(?:[1-9]|1[0-6])$/.test(world.strip(ch).id);
  const bank = world.bankFor(ch);
  if (!bank) return false;
  return kind === 'ret' && bank.families.length === 1 && bank.families[0] === 'aux'
    && bank.strips.length === 1 && bank.strips[0] === ch;
}
