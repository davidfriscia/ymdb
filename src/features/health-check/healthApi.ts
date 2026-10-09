import { invoke } from "@tauri-apps/api/core";

/** Mirrors `HealthReport` in `src-tauri/src/health.rs`. */
export interface HealthReport {
  status: "ok";
  os: string;
  arch: string;
  version: string;
}

export function checkHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("check_health");
}
