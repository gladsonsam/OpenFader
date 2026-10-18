// uiLogic/scenes: scene confirm flow + scene view.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { scenes } from '../../fixtures/designData.ts';
import { backTo } from './nav.ts';
import { type World, withState } from './types.ts';

// ---------- scenes (confirm flow) ----------

export function pickScene(w: World, i: number): World {
  if (i < 0 || i >= scenes().length) return w;
  if (w.state.sceneConfirm) return withState(w, { scenePv: i });
  return withState(w, { scenePv: i, scene: i });
}

/** RECALL: enabled only when a preview differs; loads and returns to backTo. */
export function recallScene(w: World): World {
  if (w.state.scenePv === w.state.scene) return w;
  return withState(w, { scene: w.state.scenePv, mode: backTo(w) });
}

export interface SceneView {
  names: string[];
  curNum: string;
  curName: string;
  pvNum: string;
  pvName: string;
  pending: boolean;
  recallEnabled: boolean;
  recallTag: string;
}

export function sceneView(w: World): SceneView {
  const names = scenes();
  const pending = w.state.scenePv !== w.state.scene;
  return {
    names,
    curNum: String(w.state.scene + 1).padStart(3, '0'),
    curName: names[w.state.scene],
    pvNum: String(w.state.scenePv + 1).padStart(3, '0'),
    pvName: names[w.state.scenePv],
    pending,
    recallEnabled: pending,
    recallTag: pending ? 'Selected · not recalled yet' : 'Current scene',
  };
}
