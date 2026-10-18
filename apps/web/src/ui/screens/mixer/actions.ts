// W5c Mixer actions: screen-local action registrations (imported for side
// effect by MixerScreen). The generic ui.* builtins (ui.setFader, ui.setSend,
// ui.setPan, ui.toggleMute, ui.select, ui.openEq, ui.openDyn, ...) already
// cover every other mixer event; only the SOF-aware fader commit needs a
// screen-local dispatcher: in sends-on-fader mode the strip fader shows and
// sets the send to the target mix, else the channel fader. Both paths run
// through the store so lock rules apply exactly as before.
// Run: node --experimental-strip-types apps/web/src/ui/screens/mixer/actions.test.ts

import { registerAction } from '../../modules/actions.ts';

registerAction(
  'ui.setStripFader',
  (args, ctx) => {
    const id = args['id'];
    const value = args['value'];
    if (typeof id !== 'string' || typeof value !== 'number') {
      ctx.warn(`ui.setStripFader: bad args ${JSON.stringify({ id, value })}`);
      return;
    }
    if (args['useSend'] === true) {
      const mix = args['mix'];
      if (typeof mix !== 'number') {
        ctx.warn('ui.setStripFader: send commit needs a numeric mix');
        return;
      }
      const fn = ctx.ui['setSend'] as unknown as ((id: string, k: number, v: number) => void) | undefined;
      if (typeof fn !== 'function') {
        ctx.warn('store has no "setSend" (ui.setStripFader skipped)');
        return;
      }
      fn(id, mix, value);
      return;
    }
    const fn = ctx.ui['setFader'] as unknown as ((id: string, v: number) => void) | undefined;
    if (typeof fn !== 'function') {
      ctx.warn('store has no "setFader" (ui.setStripFader skipped)');
      return;
    }
    fn(id, value);
  },
  {
    args: { id: 'string', value: 'number', useSend: 'any', mix: 'any' },
    describe: 'Set strip fader (send to mix in SOF, else channel fader; lock-checked)',
  },
);
