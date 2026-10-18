// S910 Settings shell registration (auto-discovered via ui/screens/index.ts).
// Never edit screens/index.ts; this file loads in isolation.
import { registerShellScreen } from '../../shell/registry.ts';
import SettingsScreen from './SettingsScreen.svelte';

registerShellScreen('settings', SettingsScreen);
