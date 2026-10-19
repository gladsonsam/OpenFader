import { invoke } from '@tauri-apps/api/core';

async function keepScreenOn(enable) {
    await invoke('plugin:keep-screen-on|keep_screen_on', {
        enable: enable
    });
}

export { keepScreenOn };
