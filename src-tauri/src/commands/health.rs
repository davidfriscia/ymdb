//! Application health commands.

use crate::health::{HealthReport, build_health_report};

#[tauri::command]
pub fn check_health(app: tauri::AppHandle) -> HealthReport {
    build_health_report(&app.package_info().version.to_string())
}
