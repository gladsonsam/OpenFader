// S910 Scenes shell registration (auto-discovered via ui/screens/index.ts).
// Never edit screens/index.ts; this file loads in isolation.
import { registerShellScreen } from '../../shell/registry.ts';
import ScenesScreen from './ScenesScreen.svelte';

registerShellScreen('scenes', ScenesScreen);
