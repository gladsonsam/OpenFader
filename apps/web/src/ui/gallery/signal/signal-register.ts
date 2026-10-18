// P1 signal gallery registration (imported once from screens/index.ts).
import { registerGallery } from '../../mount.ts';

registerGallery('signal', () => import('./SignalGallery.svelte'));
