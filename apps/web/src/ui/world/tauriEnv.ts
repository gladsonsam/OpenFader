// Tauri runtime probe, split out of world/transport.ts so the live-link
// singleton (state/live.svelte.ts) can test for the embedded engine WITHOUT
// pulling the WS/Tauri transport classes + protocol into the boot chunk.
// The engine modules load lazily on first connect() instead.
export function isTauriRuntime(): boolean {
  try {
    const w = globalThis as {
      __TAURI_INTERNALS__?: unknown;
      __TAURI__?: unknown;
    };
    return w.__TAURI_INTERNALS__ !== undefined || w.__TAURI__ !== undefined;
  } catch {
    return false;
  }
}
