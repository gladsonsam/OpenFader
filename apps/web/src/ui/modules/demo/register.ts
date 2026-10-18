// L0 demo registration: loads layout/generic manifests, registers the demo
// and box-test screens. Imported by ui/screens/index.ts (one line).
import { registerScreen } from '../../mount.ts';
import { loadManifestModules } from '../registry.ts';
import Demo from './Demo.svelte';
import BoxTest from './BoxTest.svelte';

loadManifestModules();
registerScreen('l0-demo', Demo);
registerScreen('l0-boxtest', BoxTest);
