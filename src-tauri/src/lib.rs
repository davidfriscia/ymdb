//! Application core of Your MAME DB (YMDB).

mod app_data;
mod atomic_file;
mod catalog;
mod commands;
mod controllers;
mod health;
mod installations;
mod launcher;
mod lists;
mod mame;
mod media;
mod remote;
mod roms;
mod saved_filters;
mod settings;
mod support_files;
mod window_state;

use std::env;
use std::path::PathBuf;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::app_data::AppDirs;
use crate::commands::app_data::AppDataState;
use crate::commands::catalog::CatalogState;
use crate::commands::controllers::ControllersState;
use crate::commands::installations::InstallationsState;
use crate::commands::launcher::LauncherState;
use crate::commands::lists::ListsState;
use crate::commands::media::MediaState;
use crate::commands::remote::RemoteState;
use crate::commands::roms::RomManagerState;
use crate::commands::saved_filters::SavedFiltersState;
use crate::commands::support_files::SupportFilesState;
use crate::installations::AppLayout;
use crate::media::config::MediaConfigStore;
use crate::media::protocol::SCHEME;
use crate::window_state::WindowStateTracker;

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
            // A confirmed restore replaces the data before anything opens it.
            let dirs = config_dir
                .as_deref()
                .zip(data_dir.as_deref())
                .map(|(config, data)| AppDirs::new(config, data));
            let restore_outcome = dirs.as_ref().and_then(app_data::restore::recover);
            if let Some(dirs) = &dirs {
                app_data::export::remove_work_dirs(dirs);
            }
            let webview_dir = commands::app_data::webview_data_dir(
                app.path().app_local_data_dir().ok().as_deref(),
            );
            app.manage(AppDataState::new(dirs, webview_dir, restore_outcome));
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
            app.manage(ListsState::new(config_dir.as_deref()));
            app.manage(RemoteState::new(config_dir.as_deref()));
            app.manage(RomManagerState::default());
            // Imported support files can be large: local data, not roaming.
            app.manage(SupportFilesState::new(data_dir.as_deref()));
            app.manage(SavedFiltersState::new(config_dir.as_deref()));
            app.manage(
                ControllersState::new(config_dir.as_deref()).with_data_dir(data_dir.as_deref()),
            );
            restore_main_window(app, config_dir.as_deref());
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
            commands::media::list_media,
            commands::lists::get_list,
            commands::lists::set_list_membership,
            commands::lists::export_lists,
            commands::lists::preview_lists_import,
            commands::lists::apply_lists_import,
            commands::lists::list_user_lists,
            commands::lists::create_list,
            commands::lists::rename_list,
            commands::lists::delete_list,
            commands::lists::get_list_entries,
            commands::lists::get_entry_lists,
            commands::lists::set_entry_memberships,
            commands::lists::choose_folder_import_file,
            commands::lists::preview_folder_import,
            commands::lists::apply_folder_import,
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
            commands::roms::prepare_download_plan,
            commands::roms::start_download,
            commands::roms::get_download_status,
            commands::roms::cancel_download,
            commands::support_files::get_support_files,
            commands::support_files::preview_support_file_import,
            commands::support_files::apply_support_file_import,
            commands::support_files::remove_support_file,
            commands::support_files::set_active_support_source,
            commands::support_files::get_classification_status,
            commands::support_files::find_missing_classification_values,
            commands::saved_filters::get_saved_filters,
            commands::saved_filters::create_saved_filter,
            commands::saved_filters::update_saved_filter,
            commands::saved_filters::rename_saved_filter,
            commands::saved_filters::delete_saved_filter,
            commands::controllers::get_controllers,
            commands::controllers::create_controller,
            commands::controllers::update_controller,
            commands::controllers::delete_controller,
            commands::controllers::create_controller_profile,
            commands::controllers::rename_controller_profile,
            commands::controllers::delete_controller_profile,
            commands::controllers::import_controller_profile_base,
            commands::controllers::remove_controller_profile_base,
            commands::controllers::set_controller_assignment,
            commands::controllers::update_controller_assignment,
            commands::controllers::reset_controller_assignment,
            commands::controllers::resolve_controls,
            commands::controllers::detect_controller_devices,
            commands::app_data::get_application_data,
            commands::app_data::open_application_data_location,
            commands::app_data::start_backup_export,
            commands::app_data::start_backup_import_check,
            commands::app_data::get_backup_status,
            commands::app_data::cancel_backup_job,
            commands::app_data::discard_backup_job,
            commands::app_data::apply_backup_import,
            commands::app_data::take_restore_outcome,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the YMDB application")
        .run(|app, event| {
            // Also covers exits without a close request (restart after a restore).
            if let RunEvent::Exit = event
                && let Some(tracker) = app.try_state::<WindowStateTracker>()
            {
                tracker.save();
            }
        });
}

/// Applies the saved placement to the main window (created hidden), shows
/// it, and tracks its placement until it closes.
fn restore_main_window(app: &tauri::App, config_dir: Option<&std::path::Path>) {
    app.manage(WindowStateTracker::new(config_dir));
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let tracker = app.state::<WindowStateTracker>();
    tracker.restore(&window);
    tracker.track(&window);
    let handle = window.clone();
    window.on_window_event(move |event| {
        let tracker = handle.state::<WindowStateTracker>();
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => tracker.schedule_track(&handle),
            WindowEvent::CloseRequested { .. } => {
                tracker.track(&handle);
                tracker.save();
            }
            _ => {}
        }
    });
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
