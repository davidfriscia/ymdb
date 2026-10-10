//! Application core of Your MAME DB (YMDB).

mod atomic_file;
mod catalog;
mod commands;
mod health;
mod launcher;
mod mame;
mod settings;

use tauri::Manager;

use crate::catalog::store::CatalogStore;
use crate::commands::SettingsState;
use crate::commands::catalog::CatalogState;
use crate::commands::launcher::LauncherState;
use crate::launcher::store::LaunchConfigStore;
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
            let launch_config = paths
                .app_config_dir()
                .ok()
                .map(|dir| LaunchConfigStore::in_dir(&dir));
            let launch_logs = paths
                .app_local_data_dir()
                .ok()
                .map(|dir| dir.join("launch-logs"));
            app.manage(CatalogState::new(catalog));
            app.manage(LauncherState::new(launch_config, launch_logs));
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
            commands::catalog::start_software_list_import,
            commands::catalog::cancel_catalog_import,
            commands::search::search_catalog_machines,
            commands::search::search_catalog_software,
            commands::detail::get_machine_detail,
            commands::detail::get_software_detail,
            commands::detail::get_software_list_detail,
            commands::launcher::get_launch_config,
            commands::launcher::save_launch_config,
            commands::launcher::get_software_launch_info,
            commands::launcher::preview_launch,
            commands::launcher::start_launch,
            commands::launcher::get_launch_sessions,
            commands::launcher::stop_launch_session,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the YMDB application");
}
