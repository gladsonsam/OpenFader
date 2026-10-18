//! Binary entry point for desktop builds. The app itself lives in the
//! library target (`src/lib.rs`) so the Android build can link it as a
//! `cdylib` behind `tauri::mobile_entry_point`. Desktop behavior is
//! unchanged: `main` just calls `openmix_app::run()`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    openmix_app::run();
}
