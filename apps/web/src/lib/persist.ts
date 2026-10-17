// Persisted UI prefs: custom layers, strip order/spacers, mute groups, safety.
// localStorage only — never touches the backend contract or WS protocol.

export interface LayerDef {
  id: string;
  name: string;
  builtin?: boolean;
}

export interface MuteGroup {
  name: string;
  members: number[];
}

/** One slot in a layer bank: a channel strip or a blank spacer. */
export type OrderEntry = { t: 'ch'; ch: number } | { t: 'gap'; gid: string };

export const LS = {
  layers: 'openmix.layers.v1',
  orders: 'openmix.layerOrder.v1',
  groups: 'openmix.mutegroups.v1',
  sends: 'openmix.sends.v1',
  muteEn: 'openmix.muteEn.v1',
  fine: 'openmix.fine.v1',
  view: 'openmix.view.v1',
} as const;

export function loadJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed: unknown = JSON.parse(raw);
    return parsed === null || parsed === undefined ? fallback : (parsed as T);
  } catch {
    return fallback;
  }
}

export function saveJSON(key: string, v: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* storage full/blocked: prefs simply don't persist */
  }
}

export function defaultLayers(): LayerDef[] {
  return [
    { id: 'inputs', name: 'Inputs', builtin: true },
    { id: 'buses', name: 'Buses', builtin: true },
    { id: 'mains', name: 'Mains', builtin: true },
  ];
}

export function defaultGroups(): MuteGroup[] {
  return [1, 2, 3, 4].map((i) => ({ name: `G${i}`, members: [] }));
}

/** Sanitize a loaded order: keep only well-formed entries. */
export function cleanOrder(raw: unknown): OrderEntry[] | null {
  if (!Array.isArray(raw)) return null;
  const out: OrderEntry[] = [];
  for (const e of raw) {
    if (e === null || typeof e !== 'object') return null;
    const rec = e as Record<string, unknown>;
    if (rec['t'] === 'ch' && typeof rec['ch'] === 'number') out.push({ t: 'ch', ch: rec['ch'] });
    else if (rec['t'] === 'gap' && typeof rec['gid'] === 'string') out.push({ t: 'gap', gid: rec['gid'] });
    else return null;
  }
  return out;
}
