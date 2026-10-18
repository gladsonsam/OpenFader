// uiLogic/connection: fixture connection selection (live link owns the real socket).
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { devices } from '../../fixtures/designData.ts';
import { type World, withState } from './types.ts';

// ---------- connection ----------

export function scanDevices(w: World, clock: string): World {
  return withState(w, { lastScan: 'Scanned at ' + clock });
}

export function disconnect(w: World): World {
  return withState(w, { connected: null });
}

export function connectDevice(w: World, key: string): World {
  if (!devices().some(d => d.key === key)) return w;
  if (w.state.connected === key) return w;
  return withState(w, { connected: key });
}
