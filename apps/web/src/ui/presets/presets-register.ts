// Presets package registration (auto-discovered via screens/index.ts glob —
// RULE: do not edit that file). Registers the isolated `presets` gallery
// route (`?gallery=presets`) used to verify the picker at both sizes.
import { registerGallery } from '../mount.ts';

registerGallery('presets', () => import('./PresetsGallery.svelte'));
