import { clearNativeSettings, resumeSettingsAfterFailedReset } from './mobile.ts';

/** Persistence inventory: openmix-offline, openmix-settings (browser fallback),
 * openmix.relay-host, openmix.relay-auto, openmix.recent-desks,
 * openmix.profile.v3.* / legacy v2.*, openmix.channel-presets.v1;
 * native settings.json is cleared by clearNativeSettings, and PWA caches
 * use openmix-shell-*. Session storage currently has no writers.
 * Only app-owned namespaces. Never use Storage.clear() on a shared origin.
 * Includes legacy profiles and any future app-owned preference keys. */
function clearOwnedStorage(storage: Storage): void {
  for (const key of Object.keys(storage)) {
    if (key.startsWith('openmix.') || key.startsWith('openmix-')) storage.removeItem(key);
  }
}

/** Caller must first stop engine writes and await disconnect. A page reload
 * afterwards drops in-memory profiles, presets, navigation and model caches.
 * No IndexedDB, cookies or filesystem scene library is used by this client.
 * The external relay owns its own data and must never be reset here. */
export async function clearAppStorage(): Promise<void> {
  try {
    await clearNativeSettings();
    clearOwnedStorage(window.localStorage);
    clearOwnedStorage(window.sessionStorage);
    if ('serviceWorker' in navigator) {
      const script = new URL('sw.js', document.baseURI).href;
      const registrations = await navigator.serviceWorker.getRegistrations();
      for (const registration of registrations) {
        if ([registration.active, registration.waiting, registration.installing].some(worker => worker?.scriptURL === script)) {
          await registration.unregister();
        }
      }
    }
    if ('caches' in window) {
      for (const name of await caches.keys()) {
        if (name.startsWith('openmix-') || name.startsWith('openmix.')) await caches.delete(name);
      }
    }
  } catch (error) {
    resumeSettingsAfterFailedReset();
    throw error;
  }
}
