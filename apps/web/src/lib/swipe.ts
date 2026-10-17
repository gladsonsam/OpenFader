// Swipe-step gesture (phone channel-at-a-time): a horizontal swipe over a
// strip bank steps the channel selection. Wired via addEventListener (not
// inline ontouch) so static scroll containers stay a11y-quiet.
export function swipe(node: HTMLElement, fn: (dir: -1 | 1) => void): { destroy(): void } {
  let x: number | null = null;
  const start = (e: TouchEvent): void => {
    x = e.touches[0]?.clientX ?? null;
  };
  const end = (e: TouchEvent): void => {
    if (x === null) return;
    const dx = (e.changedTouches[0]?.clientX ?? x) - x;
    x = null;
    if (Math.abs(dx) > 48) fn(dx < 0 ? 1 : -1);
  };
  node.addEventListener('touchstart', start, { passive: true });
  node.addEventListener('touchend', end, { passive: true });
  return {
    destroy(): void {
      node.removeEventListener('touchstart', start);
      node.removeEventListener('touchend', end);
    },
  };
}
