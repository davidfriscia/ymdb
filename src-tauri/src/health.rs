//! Application health report used to verify frontend-backend communication.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub status: AppStatus,
    /// Operating system identifier as reported by Rust (`windows`, `macos`, `linux`, ...).
    pub os: String,
    /// CPU architecture identifier (`x86_64`, `aarch64`, ...).
    pub arch: String,
    pub version: String,
}

pub fn build_health_report(version: &str) -> HealthReport {
    HealthReport {
        status: AppStatus::Ok,
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        version: version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_status_platform_and_version() {
        let report = build_health_report("1.2.3");

        assert_eq!(report.status, AppStatus::Ok);
        assert_eq!(report.os, std::env::consts::OS);
        assert_eq!(report.arch, std::env::consts::ARCH);
        assert_eq!(report.version, "1.2.3");
        assert!(!report.os.is_empty());
    }

    #[test]
    fn report_serializes_to_frontend_contract() {
        let report = HealthReport {
            status: AppStatus::Ok,
            os: "linux".to_owned(),
            arch: "aarch64".to_owned(),
            version: "0.1.0".to_owned(),
        };

        let json = serde_json::to_value(&report).expect("report should serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "status": "ok",
                "os": "linux",
                "arch": "aarch64",
                "version": "0.1.0"
            })
        );
    }
}
