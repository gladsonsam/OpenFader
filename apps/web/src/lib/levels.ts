// Shared mutable meter levels. Mutated in place by the backend callback,
// read imperatively by the rAF paint loop — never $state, so 60fps meter
// frames don't trigger reactive re-renders.
export const levels: number[] = [];

export function setLevels(next: Iterable<unknown>): void {
  levels.length = 0;
  for (const v of next) levels.push(Number(v) || 0);
}
