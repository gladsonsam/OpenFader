// F1 FX shell registration (auto-discovered via ui/screens/index.ts).
// Never edit screens/index.ts; this file loads in isolation.
import { registerShellScreen } from '../../shell/registry.ts';
import FxScreen from './FxScreen.svelte';

registerShellScreen('fx', FxScreen);
