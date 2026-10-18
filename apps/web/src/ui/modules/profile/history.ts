// L0 undo/redo: pure bounded history stack. Never throws.

export interface History<T> {
  past: T[];
  present: T;
  future: T[];
  limit: number;
}

export function createHistory<T>(initial: T, limit = 50): History<T> {
  return { past: [], present: initial, future: [], limit: Math.max(1, limit) };
}

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

/** Push a new state (clears redo). Bounded: oldest past entries are dropped. */
export function pushHistory<T>(h: History<T>, state: T): History<T> {
  const past = [...h.past, clone(h.present)];
  while (past.length > h.limit) past.shift();
  return { past, present: clone(state), future: [], limit: h.limit };
}

/** Step back; no-op at the oldest state. */
export function undoHistory<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  const past = [...h.past];
  const present = past.pop() as T;
  return { past, present, future: [clone(h.present), ...h.future], limit: h.limit };
}

/** Step forward; no-op at the newest state. */
export function redoHistory<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  const [next, ...future] = h.future;
  return { past: [...h.past, clone(h.present)], present: clone(next), future, limit: h.limit };
}

export function canUndo<T>(h: History<T>): boolean {
  return h.past.length > 0;
}

export function canRedo<T>(h: History<T>): boolean {
  return h.future.length > 0;
}
