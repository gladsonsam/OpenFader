// W2a world-debug registration (auto-discovered via ui/screens/index.ts).
// Debug screen only: ?ui=4&screen=world-debug&server=127.0.0.1:8080
import { registerScreen } from '../../mount.ts';
import WorldDebug from './WorldDebug.svelte';

registerScreen('world-debug', WorldDebug);
