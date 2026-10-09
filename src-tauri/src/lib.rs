//! Application core of Your MAME DB (YMDB).

mod commands;
mod health;
mod mame;
mod settings;

use tauri::Manager;

use crate::commands::SettingsState;
use crate::settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // A missing configuration directory is reported by the commands
            // that need it instead of preventing the application from starting.
            let store = app
                .path()
                .app_config_dir()
                .ok()
                .map(|dir| SettingsStore::in_dir(&dir));
            app.manage(SettingsState::new(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health::check_health,
            commands::mame::get_mame_configuration,
            commands::mame::pick_mame_executable,
            commands::mame::set_mame_executable,
            commands::mame::verify_mame_configuration,
            commands::mame::clear_mame_configuration,
            commands::mame::detect_mame_installations,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the YMDB application");
}
