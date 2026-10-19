// Mobile native bridge: Tauri plugins for edge-to-edge, haptics, wake-lock,
// persistent store, platform detection and logging.
//
// Everything is dynamic-imported and failure-tolerant: outside the Tauri
// runtime (desktop browser, PWA) every helper is a no-op and boot continues.

let tauriBooted = false;

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** One-shot mobile init. Fire-and-forget from main.ts boot. */
export async function initMobile(): Promise<void> {
  if (tauriBooted || !isTauri()) return;
  tauriBooted = true;
  try {
    const { registerNativeTap } = await import('./ui/primitives/feel.ts');
    registerNativeTap(() => {
      void hapticSelection();
    });
  } catch {
    /* feel bridge unavailable; vibrate fallback stays */
  }
  try {
    const { attachConsole } = await import('@tauri-apps/plugin-log');
    await attachConsole().catch(() => undefined);
  } catch {
    /* log plugin unavailable */
  }
  try {
    const { platform } = await import('@tauri-apps/plugin-os');
    document.documentElement.dataset.platform = platform();
  } catch {
    /* os plugin unavailable */
  }
  try {
    const { keepScreenOn } = await import('tauri-plugin-keep-screen-on-api');
    await keepScreenOn(true);
  } catch {
    /* keep-screen-on unavailable (desktop) */
  }
  try {
    const { onSafeAreaChanged } = await import('tauri-plugin-edge-to-edge-api');
    onSafeAreaChanged((state) => {
      const root = document.documentElement;
      root.dataset.edgeReady = '1';
      if (typeof state?.top === 'number') root.style.setProperty('--safe-area-top', `${state.top}px`);
      if (typeof state?.bottom === 'number')
        root.style.setProperty('--safe-area-bottom-computed', `${state.bottomComputed ?? state.bottom}px`);
    });
  } catch {
    /* edge-to-edge api unavailable; plugin still injects CSS vars natively */
  }
}

// ---- Haptics (no-op outside Tauri) ----

export async function hapticSelection(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { selectionFeedback } = await import('@tauri-apps/plugin-haptics');
    await selectionFeedback();
  } catch {
    /* ignore */
  }
}

export async function hapticImpact(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { impactFeedback } = await import('@tauri-apps/plugin-haptics');
    await impactFeedback('medium');
  } catch {
    /* ignore */
  }
}

export async function hapticWarn(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { notificationFeedback } = await import('@tauri-apps/plugin-haptics');
    await notificationFeedback('warning');
  } catch {
    /* ignore */
  }
}

// ---- App-update notice (Tauri updater scaffold) ----

/** Payload of the backend's one-shot `openmix-updater` event (see
 * `src-tauri/src/lib.rs`): a newer signed release exists. The shell
 * never installs automatically — download/install UI is future work. */
export interface UpdaterNotice {
  version: string;
  current_version: string;
}

/**
 * Listen for the backend's update notice. No-op outside the Tauri
 * runtime (browser/PWA); resolves to an unsubscribe function. Never
 * throws: a missing event plugin just yields a dead subscription.
 */
export async function onUpdaterAvailable(cb: (n: UpdaterNotice) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  try {
    const { listen } = await import('@tauri-apps/api/event');
    return await listen<UpdaterNotice>('openmix-updater', (e) => {
      try {
        cb(e.payload);
      } catch {
        /* ignore */
      }
    });
  } catch {
    return () => {};
  }
}

// ---- Persistent store (Tauri) with localStorage fallback ----

export interface AppSettings {
  host?: string;
  [key: string]: unknown;
}

export async function loadSettings(): Promise<AppSettings> {
  if (!isTauri()) {
    try {
      return JSON.parse(localStorage.getItem('openmix-settings') ?? '{}') as AppSettings;
    } catch {
      return {};
    }
  }
  try {
    const { load } = await import('@tauri-apps/plugin-store');
    const store = await load('settings.json', { autoSave: 100 });
    return ((await store.get<AppSettings>('settings')) ?? {}) as AppSettings;
  } catch {
    return {};
  }
}

export async function saveSettings(patch: AppSettings): Promise<void> {
  if (!isTauri()) {
    try {
      const cur = JSON.parse(localStorage.getItem('openmix-settings') ?? '{}') as AppSettings;
      localStorage.setItem('openmix-settings', JSON.stringify({ ...cur, ...patch }));
    } catch {
      /* ignore */
    }
    return;
  }
  try {
    const { load } = await import('@tauri-apps/plugin-store');
    const store = await load('settings.json', { autoSave: 100 });
    const cur = ((await store.get<AppSettings>('settings')) ?? {}) as AppSettings;
    await store.set('settings', { ...cur, ...patch });
    await store.save();
  } catch {
    /* ignore */
  }
}
