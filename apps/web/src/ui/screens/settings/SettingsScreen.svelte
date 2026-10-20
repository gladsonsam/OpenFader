<!-- S910 Settings screen: rendered 100% from ui/layouts/settings.default.json
      through LayoutRenderer + actions + bindings. The JSON owns the
      composition (centred 1100px grid, four section panels, descriptor-driven
      row repeats); row view-models flow in through `bind` expressions; row
      clicks dispatch ui.setSetting (screens/settings/actions.ts) through the
      store. This file only registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { buildSettingsVm } from './settingsVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/settings.default.json';

  const store = createProfileStore('settings');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): the phone settings grid is a
  // single column; desktop renders the design-exact grid.
  const device = useDeviceClass();

  const vm = $derived(buildSettingsVm(ui.state));
  const rows = (title: string) => vm.find(s => s.title === title)?.rows ?? [];
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    lists: {
      displayRows: rows('Display').map(row => row.key === 'spp' && device.now !== 'desktop'
        ? { ...row, label: 'Strip layout', desc: 'Touch screens use swipeable strips sized for fingers. The 8 / 12 / 16 setting applies on desktop.', control: 'none', options: [] }
        : row),
      faderRows: rows('Faders'),
      safetyRows: rows('Safety'),
      aboutRows: rows('About'),
    },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />
