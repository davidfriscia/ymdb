//! Application core of Your MAME DB (YMDB).

mod atomic_file;
mod catalog;
mod commands;
mod health;
mod launcher;
mod mame;
mod media;
mod settings;

use std::env;
use std::path::PathBuf;

use tauri::Manager;

use crate::catalog::store::CatalogStore;
use crate::commands::SettingsState;
use crate::commands::catalog::CatalogState;
use crate::commands::launcher::LauncherState;
use crate::commands::media::MediaState;
use crate::launcher::store::LaunchConfigStore;
use crate::media::config::MediaConfigStore;
use crate::media::protocol::SCHEME;
use crate::settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(SCHEME, |context, request, responder| {
            let app = context.app_handle().clone();
            let path = request.uri().path().to_owned();
            // File I/O, possibly on a network share: off the webview thread.
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(commands::media::protocol_response(&app, &path));
            });
        })
        .setup(|app| {
            // Missing directories are reported by the commands that need them
            // instead of preventing the application from starting.
            let (config_dir, data_dir) = app_dirs(app.path());
            let settings = config_dir.as_deref().map(SettingsStore::in_dir);
            // The catalog is large, machine-specific derived data: it belongs
            // in the local (non-roaming) data directory.
            let catalog = data_dir.as_deref().map(CatalogStore::in_dir);
            let launch_config = config_dir.as_deref().map(LaunchConfigStore::in_dir);
            let launch_logs = data_dir.as_ref().map(|dir| dir.join("launch-logs"));
            let media = config_dir.as_deref().map(MediaConfigStore::in_dir);
            app.manage(SettingsState::new(settings));
            app.manage(CatalogState::new(catalog));
            app.manage(LauncherState::new(launch_config, launch_logs));
            app.manage(MediaState::new(media));
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
            commands::media::get_media_sources,
            commands::media::save_media_sources,
            commands::media::pick_media_directory,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the YMDB application");
}

/// Configuration and local data directories. `YMDB_DATA_DIR`, when set, puts
/// both under one directory (`config/` and `data/`): it isolates test runs and
/// portable setups from the user's data.
fn app_dirs(paths: &tauri::path::PathResolver<tauri::Wry>) -> (Option<PathBuf>, Option<PathBuf>) {
    match env::var_os("YMDB_DATA_DIR").filter(|value| !value.is_empty()) {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            (Some(dir.join("config")), Some(dir.join("data")))
        }
        None => (paths.app_config_dir().ok(), paths.app_local_data_dir().ok()),
    }
}
