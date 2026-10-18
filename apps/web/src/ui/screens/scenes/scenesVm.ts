// S910 Scenes screen: pure view-model builder.
//
// Exact port of the `sceneCards` + recall-bar branch of the design's
// renderVals (docs/design/src/design-logic.js.txt): card numbers/names,
// Current tag, pressed/border/bg colours, pending recall-bar texts and
// colours. Components stay presentational: the screen binds this to uiStore
// state and wires ui.do actions (pickScene / recallScene / goBack).
// Run: node --experimental-strip-types apps/web/src/ui/screens/scenes/scenesVm.test.ts

import { scenes } from '../../fixtures/designData.ts';
import type { UiState } from '../../state/uiLogic.ts';

export interface SceneCardVm {
  index: number;
  num: string;
  name: string;
  tag: string;
  pressed: boolean;
  border: string;
  bg: string;
}

export interface SceneRecallVm {
  pending: boolean;
  tag: string;
  tagColor: string;
  num: string;
  name: string;
  barBorder: string;
  border: string;
  bg: string;
  text: string;
  disabled: boolean;
}

export interface ScenesVm {
  cards: SceneCardVm[];
  recall: SceneRecallVm;
}

export function buildScenesVm(state: UiState): ScenesVm {
  const names = scenes();
  const pending = state.scenePv !== state.scene;
  const cards: SceneCardVm[] = names.map((name, i) => {
    const pv = i === state.scenePv;
    return {
      index: i,
      num: String(i + 1).padStart(3, '0'),
      name,
      tag: i === state.scene ? 'Current' : '',
      pressed: pv,
      border: pv ? (i === state.scene ? '#7be27b' : '#f0b44a') : '#222a36',
      bg: pv ? '#1b2330' : '#0e1520',
    };
  });
  return {
    cards,
    recall: {
      pending,
      tag: pending ? 'Selected · not recalled yet' : 'Current scene',
      tagColor: pending ? '#f0b44a' : '#7be27b',
      num: String(state.scenePv + 1).padStart(3, '0'),
      name: names[state.scenePv],
      barBorder: pending ? '#a87622' : '#222a36',
      border: pending ? '#f0b44a' : '#2a323f',
      bg: pending ? '#f0b44a' : '#161c26',
      text: pending ? '#241703' : '#5d6878',
      disabled: !pending,
    },
  };
}
