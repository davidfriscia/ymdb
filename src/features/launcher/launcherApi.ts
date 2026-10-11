import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/launcher/` and
// `src-tauri/src/commands/launcher.rs`. Keep both sides in sync.

/** Mirrors `OptionKind` in `options.rs`. */
export type OptionKind =
  | { type: "boolean" }
  | { type: "integer"; min: number; max: number }
  | { type: "float"; min: number; max: number }
  | { type: "identifier" }
  | { type: "path" }
  | { type: "pathList" };

export type OptionCategory = "display" | "performance" | "sound" | "paths" | "machine" | "session";

/** Mirrors `OptionDefinition` in `options.rs`. */
export interface OptionDefinition {
  name: string;
  kind: OptionKind;
  category: OptionCategory;
  label: string;
  description: string;
}

/** Mirrors `OptionValue`: boolean, number, text (identifier or path) or path list. */
export type OptionValue = boolean | number | string | string[];

/** Options explicitly set at one level, by MAME option name. */
export type OptionSet = Partial<Record<string, OptionValue>>;

/** Mirrors `Profile` in `config.rs`. */
export interface Profile {
  id: string;
  name: string;
  options: OptionSet;
}

/** Mirrors `LaunchConfig` in `config.rs`. */
export interface LaunchConfig {
  schemaVersion: number;
  globalOptions: OptionSet;
  profiles: Profile[];
  defaultProfileId: string;
  /** By machine short name. */
  machineOverrides: Partial<Record<string, OptionSet>>;
  /** By `<list>:<software>`. */
  softwareOverrides: Partial<Record<string, OptionSet>>;
}

/** Mirrors `LaunchConfigView` in `commands/launcher.rs`. */
export interface LaunchConfigView {
  installationId: string;
  config: LaunchConfig;
  definitions: OptionDefinition[];
  /** Set when `launch.json` could not be read; `config` is then the default. */
  loadError: string | null;
}

export const DEFAULT_PROFILE_ID = "default";

export function softwareKey(list: string, name: string): string {
  return `${list}:${name}`;
}

/** Mirrors `MediaAssignment` in `resolve.rs`. */
export interface MediaAssignment {
  /** Media device brief name, for example `cart` or `flop1`. */
  device: string;
  part: string;
}

/** Mirrors `LaunchRequest` in `launcher/mod.rs`. */
export interface LaunchRequest {
  /** The installation whose executable, catalog and launch options are used. */
  installationId: string;
  machine: string;
  software?: { list: string; name: string; media: MediaAssignment[] };
  /** Omitted for the default profile. */
  profileId?: string;
}

export type OptionSource = "global" | "profile" | "machine" | "software" | "romManager";

/** Mirrors `PlanArgument` in `command.rs`. */
export type PlanArgument =
  | { role: "machine"; tokens: string[] }
  | { role: "media"; device: string; part: string; tokens: string[] }
  | { role: "option"; name: string; source: OptionSource; tokens: string[] };

/** Mirrors `LaunchPreview` in `launcher/mod.rs`. */
export interface LaunchPreview {
  plan: {
    installationId: string;
    /** The launch profile applied. */
    profileId: string;
    executable: string;
    workingDirectory: string;
    arguments: PlanArgument[];
  };
  commandLine: string[];
  warnings: string[];
}

/** Mirrors `MediaDevice` in `resolve.rs`. */
export interface MediaDevice {
  briefName: string;
  instanceName: string | null;
  deviceType: string;
  interfaces: string[];
}

/** Mirrors `SoftwarePart` in `resolve.rs`. */
export interface SoftwarePart {
  name: string;
  interface: string;
  label: string | null;
  compatibility: string | null;
}

/** Mirrors `MachineCandidate` in `resolve.rs`. */
export interface MachineCandidate {
  name: string;
  description: string;
  year: string | null;
  manufacturer: string | null;
  cloneOf: string | null;
  relation: "original" | "compatible";
  filters: string[];
  filterMatch: boolean;
  devices: MediaDevice[];
  defaultMedia: MediaAssignment[];
  launchable: boolean;
}

/** Mirrors `SoftwareLaunchInfo` in `resolve.rs`. */
export interface SoftwareLaunchInfo {
  listName: string;
  name: string;
  description: string;
  parts: SoftwarePart[];
  candidates: MachineCandidate[];
  recommended: string | null;
}

/** Mirrors `LaunchTarget` in `session.rs`. */
export interface LaunchTarget {
  machine: string;
  software: { list: string; name: string } | null;
}

/** Mirrors `SessionState` in `session.rs`. */
export type SessionState =
  | { state: "running" }
  | {
      state: "exited";
      exitCode: number | null;
      success: boolean;
      message: string;
      logTail: string;
      finishedAt: number;
    }
  | { state: "stopped"; finishedAt: number };

/** Mirrors `SessionInfo` in `session.rs`. */
export interface SessionInfo {
  id: number;
  /** The installation the session was started from. */
  installationId: string;
  /** Its name when the session started. */
  installationLabel: string;
  target: LaunchTarget;
  title: string;
  commandLine: string[];
  startedAt: number;
  state: SessionState;
}

/** The launch configuration of one installation. */
export function getLaunchConfig(installationId: string): Promise<LaunchConfigView> {
  return invokeCommand("get_launch_config", { installationId });
}

export function saveLaunchConfig(
  installationId: string,
  config: LaunchConfig,
): Promise<LaunchConfigView> {
  return invokeCommand("save_launch_config", { installationId, config });
}

/** Resolves to `null` when the software is not in the installation's catalog. */
export function getSoftwareLaunchInfo(
  installationId: string,
  list: string,
  name: string,
): Promise<SoftwareLaunchInfo | null> {
  return invokeCommand("get_software_launch_info", { installationId, list, name });
}

export function previewLaunch(request: LaunchRequest): Promise<LaunchPreview> {
  return invokeCommand("preview_launch", { request });
}

export function startLaunch(request: LaunchRequest): Promise<SessionInfo> {
  return invokeCommand("start_launch", { request });
}

/** In-memory session list: cheap, suitable for polling. */
export function getLaunchSessions(): Promise<SessionInfo[]> {
  return invokeCommand("get_launch_sessions");
}

export function stopLaunchSession(id: number): Promise<SessionInfo> {
  return invokeCommand("stop_launch_session", { id });
}

export function sameTarget(a: LaunchTarget, b: LaunchTarget): boolean {
  return (
    a.machine === b.machine &&
    a.software?.list === b.software?.list &&
    a.software?.name === b.software?.name
  );
}

/** A profile id derived from its name, unique among `profiles`. */
export function profileId(name: string, profiles: Profile[]): string {
  const base =
    name
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 32) || "profile";
  let id = base;
  for (let suffix = 2; profiles.some((profile) => profile.id === id); suffix++) {
    id = `${base}-${String(suffix)}`;
  }
  return id;
}
