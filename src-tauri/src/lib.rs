//! Application core of Your MAME DB (YMDB).

mod commands;
mod health;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::check_health])
        .run(tauri::generate_context!())
        .expect("failed to run the YMDB application");
}
