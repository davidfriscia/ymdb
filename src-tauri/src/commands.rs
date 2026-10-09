//! Tauri commands exposed to the frontend.
//!
//! Commands are thin adapters: they extract what they need from the Tauri
//! runtime and delegate to plain, testable functions.

use crate::health::{HealthReport, build_health_report};

#[tauri::command]
pub fn check_health(app: tauri::AppHandle) -> HealthReport {
    build_health_report(&app.package_info().version.to_string())
}
