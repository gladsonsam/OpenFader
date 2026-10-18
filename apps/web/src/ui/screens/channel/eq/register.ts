// W5b EQ tab registration (auto-discovered via ui/screens/index.ts).
// The EQ tab content is layout JSON (ui/layouts/eq.default.json), rendered
// by C1's ChannelScreen; the tab registry keeps a layout-backed fallback
// for compatibility. Never touches tabRegistry.ts itself or C1/C3 files.
import { registerChannelTab } from '../tabRegistry.ts';
import ChannelTabFallback from '../ChannelTabFallback.svelte';

registerChannelTab('eq', ChannelTabFallback);
