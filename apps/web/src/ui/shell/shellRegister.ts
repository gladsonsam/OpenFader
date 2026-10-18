// H1 shell package: route registration (the single import line in
// ui/screens/index.ts points here). The Shell renders for `?ui=4&screen=shell`
// and for every design screen id in the fixture URL contract, so the diff
// harness (`screen=<design screen id>`) always sees header + rail even before
// the content screens land (they show a neutral placeholder until then).
import { registerScreen } from '../mount.ts';
import { SCREEN_IDS } from '../state/urlBridge.ts';
import Shell from './Shell.svelte';

registerScreen('shell', Shell);
for (const id of SCREEN_IDS) registerScreen(id, Shell);
