// H1 shell package public surface.
export { default as HeaderBar } from './HeaderBar.svelte';
export { default as BankRail } from './BankRail.svelte';
export { default as SceneButton } from './SceneButton.svelte';
export { default as Shell } from './Shell.svelte';
export {
  registerShellScreen,
  registerDesignScreens,
  getShellScreen,
  clearShellScreens,
} from './registry.ts';
export { BANK_TINT, bankBtnStyle, tileStyle, ariaCurrent } from './shellStyle.ts';
export type { BankBtnStyle, TileStyle } from './shellStyle.ts';
