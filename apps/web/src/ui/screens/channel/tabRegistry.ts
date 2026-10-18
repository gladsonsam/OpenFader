// Channel screen tab-content registry (shared contract for packages C1, C2, C3).
// C1 owns the ChannelScreen shell (left tab list, right master strip) and renders the registered tab content in the
// middle region. Tab packages register their CONTENT component only:
//   registerChannelTab('eq', EqTab)   // from ui/screens/channel/eq/register.ts
// Tab ids follow the design: input | eq | gate | comp | sends  (screen ids ch-input, ch-eq, ch-gate, ch-comp, ch-sends).
import type { Component } from 'svelte';

export type ChannelTabId = 'input' | 'eq' | 'gate' | 'comp' | 'sends';

const tabs = new Map<ChannelTabId, Component>();

export function registerChannelTab(id: ChannelTabId, component: Component): void {
  tabs.set(id, component);
}

export function getChannelTab(id: ChannelTabId): Component | null {
  return tabs.get(id) ?? null;
}

export function clearChannelTabs(): void {
  tabs.clear();
}
