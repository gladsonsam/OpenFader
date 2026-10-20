// W2a write coalescer: SPEC 2a leading-edge send + latest-wins coalescing.
//
// A fader drag fires dozens of writes per second; the engine accepts <= 60/s
// per control. First write of a burst goes out immediately (local-first
// latency); writes inside the 1/60 s window collapse to the latest value,
// flushed by one timer per key. Timer + clock are injected for node tests.

export const MIN_INTERVAL_MS = 1000 / 60;

export interface CoalescerClock {
  now(): number;
  setTimer(fn: () => void, ms: number): unknown;
  clearTimer(t: unknown): void;
}

export class WriteCoalescer {
  private lastSentAt = new Map<string, number>();
  private queued = new Map<string, () => void>();
  private timers = new Map<string, unknown>();
  private readonly clock: CoalescerClock;

  constructor(clock: CoalescerClock) {
    this.clock = clock;
  }

  /**
   * Offer one write. `send` runs immediately on the leading edge, else the
   * write is held and only the latest per key is sent when the window lapses.
   * Returns 'sent' | 'coalesced'.
   */
  offer(key: string, send: () => void): 'sent' | 'coalesced' {
    const now = this.clock.now();
    const last = this.lastSentAt.get(key);
    if (last === undefined || now - last >= MIN_INTERVAL_MS) {
      this.cancel(key);
      this.lastSentAt.set(key, now);
      send();
      return 'sent';
    }
    this.queued.set(key, send);
    if (!this.timers.has(key)) {
      const wait = Math.max(0, MIN_INTERVAL_MS - (now - last));
      const t = this.clock.setTimer(() => {
        this.timers.delete(key);
        const fn = this.queued.get(key);
        this.queued.delete(key);
        if (fn) {
          this.lastSentAt.set(key, this.clock.now());
          fn();
        }
      }, wait);
      this.timers.set(key, t);
    }
    return 'coalesced';
  }

  flush(key: string): void {
    const send = this.queued.get(key);
    this.cancel(key);
    send?.();
  }

  cancel(key: string): void {
    const t = this.timers.get(key);
    if (t !== undefined) this.clock.clearTimer(t);
    this.timers.delete(key);
    this.queued.delete(key);
    this.lastSentAt.delete(key);
  }

  /** Drop held writes (e.g. on disconnect). */
  clear(): void {
    for (const t of this.timers.values()) this.clock.clearTimer(t);
    this.timers.clear();
    this.queued.clear();
    this.lastSentAt.clear();
  }
}
