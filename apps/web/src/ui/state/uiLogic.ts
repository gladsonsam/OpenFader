// D2 State package: pure (rune-free, DOM-free) port of the design's state
// machine — public import path for the logic split across state/logic/.
// One version of each transition lives in its feature module below; this
// file only re-exports so existing screens/tests keep their imports.
//
// Modules: types (state + update helpers), nav, mix (fader/send/pan/mute),
// eq, inputDyn, fx, routing, connection, settings, scenes, selectors.
//
// Lock semantics mirror the design exactly: move()/mute/patch/dyn-toggle and
// the input toggles check `locked`; navigation, EQ edits, FX edits, scene
// picks and settings never do.

export * from './logic/types.ts';
export * from './logic/nav.ts';
export * from './logic/mix.ts';
export * from './logic/eq.ts';
export * from './logic/inputDyn.ts';
export * from './logic/fx.ts';
export * from './logic/routing.ts';
export * from './logic/connection.ts';
export * from './logic/settings.ts';
export * from './logic/scenes.ts';
export * from './logic/selectors.ts';

export { clamp as clampValue, toN as specToN, fromN as specFromN, fmtMs as fmtMsValue } from '../fixtures/designData.ts';
