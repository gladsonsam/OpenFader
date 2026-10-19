// W5b Channel shell registration (auto-discovered via ui/screens/index.ts).
// Registers the layout-rendered ChannelScreen for every channel shell id.
// Tab contents are layout JSON now (see ui/layouts/*.default.json); the
// tab registry stays wired as a thin fallback for compatibility.
import { registerShellScreen } from '../../shell/registry.ts';
import { registerChannelTab } from './tabRegistry.ts';
import ChannelScreen from './ChannelScreen.svelte';
import ChannelTabFallback from './ChannelTabFallback.svelte';

for (const id of ['ch-input', 'ch-eq', 'ch-gate', 'ch-comp', 'ch-sends', 'ch-groups']) {
  registerShellScreen(id, ChannelScreen);
}
registerChannelTab('input', ChannelTabFallback);
registerChannelTab('sends', ChannelTabFallback);
registerChannelTab('groups', ChannelTabFallback);
