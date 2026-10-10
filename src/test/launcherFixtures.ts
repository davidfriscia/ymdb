import type {
  LaunchConfig,
  LaunchConfigView,
  OptionDefinition,
  SessionInfo,
  SessionState,
} from "../features/launcher/launcherApi";

/** A subset of the backend option definitions (`launcher/options.rs`). */
export const OPTION_DEFINITIONS: OptionDefinition[] = [
  {
    name: "window",
    kind: { type: "boolean" },
    category: "display",
    label: "Window mode",
    description: "Run in a window instead of full screen.",
  },
  {
    name: "speed",
    kind: { type: "float", min: 0.01, max: 100 },
    category: "performance",
    label: "Speed",
    description: "Emulation speed relative to real time (1 = normal).",
  },
  {
    name: "volume",
    kind: { type: "integer", min: -96, max: 12 },
    category: "sound",
    label: "Volume",
    description: "Sound volume in decibels (-96 to 12).",
  },
  {
    name: "rompath",
    kind: { type: "pathList" },
    category: "paths",
    label: "ROM paths",
    description: "Directories searched for ROM sets and CHDs.",
  },
  {
    name: "bios",
    kind: { type: "identifier" },
    category: "machine",
    label: "BIOS",
    description: "BIOS set of the machine, by short name.",
  },
];

export function launchConfig(overrides: Partial<LaunchConfig> = {}): LaunchConfig {
  return {
    schemaVersion: 1,
    globalOptions: {},
    profiles: [{ id: "default", name: "Default", options: {} }],
    defaultProfileId: "default",
    machineOverrides: {},
    softwareOverrides: {},
    ...overrides,
  };
}

export function launchConfigView(config: LaunchConfig = launchConfig()): LaunchConfigView {
  return { config, definitions: OPTION_DEFINITIONS, loadError: null };
}

export function session(
  id: number,
  machine: string,
  state: SessionState = { state: "running" },
  software: { list: string; name: string } | null = null,
): SessionInfo {
  return {
    id,
    target: { machine, software },
    title: `${machine} description`,
    commandLine: [machine],
    startedAt: Date.UTC(2026, 9, 10, 20, 30),
    state,
  };
}
