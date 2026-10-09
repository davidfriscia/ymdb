//! Application core of Your MAME DB (YMDB).

mod catalog;
mod commands;
mod health;
mod mame;
mod settings;

use tauri::Manager;

use crate::catalog::store::CatalogStore;
use crate::commands::SettingsState;
use crate::commands::catalog::CatalogState;
use crate::settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Missing directories are reported by the commands that need them
            // instead of preventing the application from starting.
            let paths = app.path();
            let settings = paths
                .app_config_dir()
                .ok()
                .map(|dir| SettingsStore::in_dir(&dir));
            // The catalog is large, machine-specific derived data: it belongs
            // in the local (non-roaming) data directory.
            let catalog = paths
                .app_local_data_dir()
                .ok()
                .map(|dir| CatalogStore::in_dir(&dir));
            app.manage(SettingsState::new(settings));
            app.manage(CatalogState::new(catalog));
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
            commands::catalog::get_catalog_status,
            commands::catalog::get_catalog_import_status,
            commands::catalog::start_catalog_import,
            commands::catalog::cancel_catalog_import,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the YMDB application");
}
