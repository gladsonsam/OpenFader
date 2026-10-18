// W5b Gate+Compressor tab registration (auto-discovered via ui/screens/index.ts).
// Both tabs are layout JSON (ui/layouts/gate.default.json,
// ui/layouts/comp.default.json), rendered by C1's ChannelScreen; the tab
// registry keeps a layout-backed fallback for compatibility.
import { registerChannelTab } from '../tabRegistry.ts';
import ChannelTabFallback from '../ChannelTabFallback.svelte';

registerChannelTab('gate', ChannelTabFallback);
registerChannelTab('comp', ChannelTabFallback);
