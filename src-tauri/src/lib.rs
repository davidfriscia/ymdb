//! Application core of Your MAME DB (YMDB).

mod atomic_file;
mod catalog;
mod commands;
mod health;
mod installations;
mod launcher;
mod mame;
mod media;
mod remote;
mod settings;

use std::env;
use std::path::PathBuf;

use tauri::Manager;

use crate::commands::catalog::CatalogState;
use crate::commands::installations::InstallationsState;
use crate::commands::launcher::LauncherState;
use crate::commands::media::MediaState;
use crate::commands::remote::RemoteState;
use crate::installations::AppLayout;
use crate::media::config::MediaConfigStore;
use crate::media::protocol::SCHEME;

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
            // Catalogs are large, machine-specific derived data: they belong
            // in the local (non-roaming) data directory. Loading the registry
            // upgrades the single-installation layout if needed; MAME is not run.
            let layout = config_dir
                .as_deref()
                .zip(data_dir.as_deref())
                .map(|(config, data)| AppLayout::new(config, data));
            let launch_logs = data_dir.as_ref().map(|dir| dir.join("launch-logs"));
            let media = config_dir.as_deref().map(MediaConfigStore::in_dir);
            app.manage(InstallationsState::new(layout));
            app.manage(CatalogState::default());
            app.manage(LauncherState::new(launch_logs));
            app.manage(MediaState::new(media));
            app.manage(RemoteState::new(config_dir.as_deref()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health::check_health,
            commands::mame::pick_mame_executable,
            commands::mame::detect_mame_installations,
            commands::installations::list_installations,
            commands::installations::add_installation,
            commands::installations::verify_installation,
            commands::installations::rename_installation,
            commands::installations::set_active_installation,
            commands::installations::remove_installation,
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
            commands::remote::get_remote_connections,
            commands::remote::save_remote_connection,
            commands::remote::remove_remote_connection,
            commands::remote::test_remote_connection,
            commands::remote::approve_remote_host_key,
            commands::remote::forget_remote_host_key,
            commands::remote::browse_remote_directory,
            commands::remote::pick_private_key_file,
            commands::remote::pick_destination_directory,
            commands::remote::get_remote_sources,
            commands::remote::save_remote_sources,
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
