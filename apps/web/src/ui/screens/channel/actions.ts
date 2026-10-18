// W5b channel actions: layout-JSON entry points for the Channel screens.
//
// The generic store actions the channel layouts need (ui.setTab,
// ui.stepChannel, ui.setFader, ui.setPan, ui.toggleMute, ui.setSend,
// ui.toggleSendPre, ui.setEqParam, ui.toggleEqOn, ui.toggleEqBand,
// ui.flatEq, ui.setHpfSlope, ui.setEqBandType, ui.setDynParam, ui.toggleDyn,
// ui.toggleDelayOn, ui.toggleInvert, ui.togglePhantom, ui.goPatch, ...)
// are L1 builtins in ui/modules/actions.ts — the default layouts call them
// directly. This file registers ONLY what builtins cannot express:
//
//   ui.setInputKnob   input gain/trim/delay knobs (no builtin; the store's
//                     setInputKnob maps the raw knob value through the spec)
//   channel.eqNode    the EQ graph node gesture: one drag/keyboard edit maps
//                     to one (HPF: freq only) or two (band: freq + gain)
//                     setEqParam calls, exactly like EqTab.node did. `locked`
//                     drops value edits (selection is a separate ui.setSelBand)
//   channel.dynKnob   the dyn knob gesture: DynTab.onKnob mapped the raw
//                     knob value through dynSpecKey + toN before setDynParam;
//                     `locked` drops the edit (the store's setDynParam is not
//                     lock-checked; the old handler checked it)
//
// All handlers run through the live store (bound per screen with
// bindUiActions), so lock/muteEnable/arm-timer rules apply exactly as in
// the plain screens. Registered on import (like the builtins); never throw.
//
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/actions.test.ts

import { registerAction, type ActionCtx } from '../../modules/actions.ts';
import { dynSpecKey } from '../../state/uiLogic.ts';
import { spec, toN } from '../../fixtures/designData.ts';

function uiFn(ctx: ActionCtx, name: string): ((...a: never[]) => unknown) | null {
  const fn = ctx.ui[name];
  if (typeof fn !== 'function') {
    ctx.warn(`store has no "${name}" (channel action skipped)`);
    return null;
  }
  return fn as (...a: never[]) => unknown;
}

/** Register all W5b channel actions (idempotent; call after clearActions in tests). */
export function registerChannelActions(): void {
  registerAction(
    'ui.setInputKnob',
    (args, ctx) => {
      const set = uiFn(ctx, 'setInputKnob');
      if (!set) return;
      set(args['id'] as never, args['key'] as never, args['value'] as never);
    },
    {
      args: { id: 'string', key: 'string', value: 'number' },
      describe: 'Set input gain/trim/delay knob (raw knob value)',
    },
  );

  registerAction(
    'channel.eqNode',
    (args, ctx) => {
      if (args['locked'] === true) return;
      const setEq = uiFn(ctx, 'setEqParam');
      if (!setEq) return;
      const id = args['id'] as string;
      const band = args['band'] as number;
      const f = args['f'] as number;
      if (band === 0) {
        setEq(id as never, band as never, 'f' as never, f as never);
        return;
      }
      const g = args['g'] as number;
      setEq(id as never, band as never, 'f' as never, f as never);
      setEq(id as never, band as never, 'g' as never, g as never);
    },
    {
      args: { id: 'string', band: 'number', f: 'number', g: 'number', locked: 'boolean' },
      describe: 'EQ graph node edit (HPF: freq only; band: freq + gain)',
    },
  );

  registerAction(
    'channel.dynKnob',
    (args, ctx) => {
      if (args['locked'] === true) return;
      const which = args['which'] as string;
      const prop = args['prop'] as string;
      let key: string;
      try {
        key = dynSpecKey(which as 'gate' | 'comp', prop);
      } catch (e) {
        ctx.warn(`channel.dynKnob: ${e instanceof Error ? e.message : String(e)}`);
        return;
      }
      const setDyn = uiFn(ctx, 'setDynParam');
      if (!setDyn) return;
      const n = toN(spec(key), args['value'] as number);
      setDyn(args['id'] as never, which as never, prop as never, n as never);
    },
    {
      args: { id: 'string', which: 'string', prop: 'string', value: 'number', locked: 'boolean' },
      describe: 'Dynamics knob edit (raw value mapped through the spec)',
    },
  );
}

registerChannelActions();

export function channelActionIds(): string[] {
  return ['ui.setInputKnob', 'channel.eqNode', 'channel.dynKnob'];
}
