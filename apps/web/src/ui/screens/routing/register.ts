// S78 Routing shell registration (auto-discovered via ui/screens/index.ts).
// Never edit screens/index.ts; this file loads in isolation.
import { registerShellScreen } from '../../shell/registry.ts';
import RoutingScreen from './RoutingScreen.svelte';

registerShellScreen('routing', RoutingScreen);
