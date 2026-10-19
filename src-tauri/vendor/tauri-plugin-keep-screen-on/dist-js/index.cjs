'use strict';

var core = require('@tauri-apps/api/core');

async function keepScreenOn(enable) {
    await core.invoke('plugin:keep-screen-on|keep_screen_on', {
        enable: enable
    });
}

exports.keepScreenOn = keepScreenOn;
