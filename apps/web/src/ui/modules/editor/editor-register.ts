// Layout-editor registration: loads manifests (idempotent — Demo's register
// already ran it; duplicates are rejected) and exposes the visual editor at
// `?ui=4&screen=layout-editor`.
import { registerScreen } from '../../mount.ts';
import { loadManifestModules } from '../registry.ts';
import EditorPage from './EditorPage.svelte';

loadManifestModules();
registerScreen('layout-editor', EditorPage);
