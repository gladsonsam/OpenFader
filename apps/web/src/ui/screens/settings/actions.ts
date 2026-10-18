// W5a Settings actions: the layout-dispatched store action for the
// data-driven settings rows.
//
// The settings layout repeats over SETTING_DESCRIPTORS-derived rows and each
// `setting-row` emits `change` {key, value}; this registers `ui.setSetting`
// which maps the descriptor key to the matching ui.do store call with the
// same string->value decoding the plain SettingsScreen used in onPick.
// Registered here in the settings package (screens/<x>/actions.ts), never by
// editing ui/modules/actions.ts. Imported for its side effect by
// SettingsScreen.svelte (browser) and by the layout/action tests (node).
//
// Run: node --experimental-strip-types apps/web/src/ui/screens/settings/actions.test.ts

import { registerAction } from '../../modules/actions.ts';

/** Register the W5a settings actions (idempotent: last wins, never throws). */
export function registerSettingsActions(): void {
  registerAction(
    'ui.setSetting',
    (args, ctx) => {
      const key = String(args['key'] ?? '');
      const raw = String(args['value'] ?? '');
      const call = (name: string, ...a: unknown[]): void => {
        const fn = ctx.ui[name];
        if (typeof fn !== 'function') {
          ctx.warn(`store has no "${name}" (ui.setSetting "${key}" skipped)`);
          return;
        }
        (fn as (...x: unknown[]) => unknown)(...a);
      };
      switch (key) {
        case 'spp': call('setSpp', parseInt(raw, 10)); break;
        case 'peakHold': call('setPeakHold', raw === 'on'); break;
        case 'fine': call('setFine', raw === 'on'); break;
        case 'sof': call('setSof', raw === 'on'); break;
        case 'mix': call('setMix', parseInt(raw, 10)); break;
        case 'muteEnable': call('setMuteEnable', raw === 'on'); break;
        case 'locked': call('setLocked', raw === 'on'); break;
        case 'sceneConfirm': call('setSceneConfirm', raw === 'confirm'); break;
        default: ctx.warn(`ui.setSetting: unknown setting "${key}"`); break;
      }
    },
    { args: { key: 'string', value: 'any' }, describe: 'Set a behaviour setting by descriptor key' },
  );
}

registerSettingsActions();
