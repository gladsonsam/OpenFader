import { invoke } from '@tauri-apps/api/core'
import {
  warn
} from '@tauri-apps/plugin-log';

export async function keepScreenOn(enable: boolean):Promise<void> {
  await invoke<{value?: string}>('plugin:keep-screen-on|keep_screen_on', {
      enable: enable
  });
}
