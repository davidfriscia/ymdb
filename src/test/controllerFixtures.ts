import type {
  Assignment,
  AssignmentTarget,
  AssignmentUpdate,
  Controller,
  ControllerInput,
  ControllerProfile,
  ControllersView,
  ControlsResolution,
  ControlsSource,
  FieldUpdate,
  LevelAssignment,
} from "../features/controllers/controllersApi";

export const MAME_PROFILE: ControllerProfile = {
  id: "mame",
  kind: "system",
  name: "MAME configuration",
  hasBase: false,
  baseFileName: null,
  managed: false,
};

export function userProfile(
  id: string,
  name: string,
  overrides: Partial<ControllerProfile> = {},
): ControllerProfile {
  return {
    id,
    kind: "user",
    name,
    hasBase: false,
    baseFileName: null,
    managed: true,
    ...overrides,
  };
}

export function controller(
  id: string,
  name: string,
  overrides: Partial<Controller> = {},
): Controller {
  return { id, name, kind: "arcadeStick", deviceClass: "joystick", ...overrides };
}

/** The initial configuration: the `mame` profile, used by default. */
export function controllersView(overrides: Partial<ControllersView> = {}): ControllersView {
  return {
    controllers: [],
    profiles: [MAME_PROFILE],
    assignments: {
      default: { profileId: "mame" },
      machines: {},
      software: {},
      machineSoftware: {},
    },
    ...overrides,
  };
}

/** The resolution of the `mame` profile, inherited from the default. */
export function mameResolution(levels: LevelAssignment[] = []): ControlsResolution {
  return {
    profileId: "mame",
    profileSource: "default",
    players: [],
    playersSource: null,
    profileName: "MAME configuration",
    levels,
    cfgDirectory: null,
  };
}

type Args = Record<string, unknown>;

function mapOf(view: ControllersView, target: AssignmentTarget) {
  switch (target.level) {
    case "machine":
      return view.assignments.machines;
    case "software":
      return view.assignments.software;
    case "machineSoftware":
      return view.assignments.machineSoftware;
    case "default":
      return null;
  }
}

function stored(view: ControllersView, target: AssignmentTarget): Assignment | undefined {
  const map = mapOf(view, target);
  return map ? map[target.key ?? ""] : view.assignments.default;
}

function apply<T>(update: FieldUpdate<T> | undefined, value: T | undefined): T | undefined {
  if (!update || update.kind === "keep") return value;
  return update.kind === "set" ? update.value : undefined;
}

function store(view: ControllersView, target: AssignmentTarget, assignment: Assignment) {
  const clean: Assignment = {};
  if (assignment.profileId !== undefined) clean.profileId = assignment.profileId;
  if (assignment.players !== undefined) clean.players = assignment.players;
  const map = mapOf(view, target);
  if (!map) {
    view.assignments.default = clean;
  } else if (clean.profileId === undefined && clean.players === undefined) {
    // Left out of the view, like a removed key (`toEqual` ignores it).
    map[target.key ?? ""] = undefined;
  } else {
    map[target.key ?? ""] = clean;
  }
}

/** A Tauri `CommandError` rejection. */
export function commandError(kind: string, fields: Record<string, unknown> = {}): Error {
  return Object.assign(new Error(kind), { kind, ...fields });
}

/**
 * A stateful fake of the controller commands. Its resolution follows the
 * backend's documented precedence (default < machine < software <
 * machine + software), field by field, so that the UI can be tested against
 * realistic answers; the UI itself never resolves.
 */
export function controllersBackend(initial: ControllersView = controllersView()) {
  let view: ControllersView = structuredClone(initial);
  let nextBase: string | null = "xarcade.cfg";
  const references = (uses: (assignment: Assignment) => boolean) => {
    const targets: AssignmentTarget[] = [];
    if (uses(view.assignments.default)) targets.push({ level: "default" });
    for (const [level, map] of [
      ["machine", view.assignments.machines],
      ["software", view.assignments.software],
      ["machineSoftware", view.assignments.machineSoftware],
    ] as const) {
      for (const [key, assignment] of Object.entries(map)) {
        if (assignment && uses(assignment)) targets.push({ level, key });
      }
    }
    return targets;
  };
  const snapshot = () => structuredClone(view);
  const profile = (id: string) => view.profiles.find((entry) => entry.id === id);
  const userProfileOf = (id: string) => {
    const found = profile(id);
    if (!found)
      throw commandError("notFound", { message: `the controller profile '${id}' does not exist` });
    if (found.kind === "system")
      throw commandError("invalidRequest", {
        message: `the system profile '${id}' cannot be changed`,
      });
    return found;
  };
  const slug = (name: string) => name.toLowerCase().replace(/[^a-z0-9]+/g, "-");

  const handlers = {
    get_controllers: () => snapshot(),
    create_controller: (args: Args) => {
      const input = args.controller as ControllerInput;
      view.controllers.push({
        id: slug(input.name),
        name: input.name,
        kind: input.kind,
        deviceClass: input.deviceClass,
        ...(input.deviceId ? { deviceId: input.deviceId } : {}),
        ...(input.provider ? { provider: input.provider } : {}),
      });
      return snapshot();
    },
    update_controller: (args: Args) => {
      const input = args.controller as ControllerInput;
      view.controllers = view.controllers.map((entry) =>
        entry.id === args.controllerId
          ? {
              id: entry.id,
              name: input.name,
              kind: input.kind,
              deviceClass: input.deviceClass,
              ...(input.deviceId ? { deviceId: input.deviceId } : {}),
              ...(input.provider ? { provider: input.provider } : {}),
            }
          : entry,
      );
      return snapshot();
    },
    delete_controller: (args: Args) => {
      const id = args.controllerId as string;
      const used = references((assignment) => assignment.players?.includes(id) ?? false);
      if (used.length > 0) {
        throw commandError("controllersInUse", {
          message: `the controller '${id}' is assigned in ${String(used.length)} assignment(s); remove it from them first`,
          references: used,
        });
      }
      view.controllers = view.controllers.filter((entry) => entry.id !== id);
      return snapshot();
    },
    create_controller_profile: (args: Args) => {
      if (args.importBase && nextBase === null) return null;
      view.profiles.push(
        userProfile(slug(args.name as string), args.name as string, {
          hasBase: Boolean(args.importBase),
          baseFileName: args.importBase ? nextBase : null,
        }),
      );
      return snapshot();
    },
    rename_controller_profile: (args: Args) => {
      userProfileOf(args.profileId as string).name = args.name as string;
      return snapshot();
    },
    delete_controller_profile: (args: Args) => {
      const id = args.profileId as string;
      userProfileOf(id);
      const used = references((assignment) => assignment.profileId === id);
      if (used.length > 0) {
        throw commandError("controllersInUse", {
          message: `the profile '${id}' is used by ${String(used.length)} assignment(s); change them first`,
          references: used,
        });
      }
      view.profiles = view.profiles.filter((entry) => entry.id !== id);
      return snapshot();
    },
    import_controller_profile_base: (args: Args) => {
      const found = userProfileOf(args.profileId as string);
      if (nextBase === null) return null;
      found.hasBase = true;
      found.baseFileName = nextBase;
      return snapshot();
    },
    remove_controller_profile_base: (args: Args) => {
      const found = userProfileOf(args.profileId as string);
      found.hasBase = false;
      found.baseFileName = null;
      return snapshot();
    },
    set_controller_assignment: (args: Args) => {
      store(view, args.target as AssignmentTarget, args.assignment as Assignment);
      return snapshot();
    },
    update_controller_assignment: (args: Args) => {
      const target = args.target as AssignmentTarget;
      const update = args.update as AssignmentUpdate;
      const current = stored(view, target) ?? {};
      store(view, target, {
        profileId: apply(update.profileId, current.profileId),
        players: apply(update.players, current.players),
      });
      return snapshot();
    },
    reset_controller_assignment: (args: Args) => {
      const target = args.target as AssignmentTarget;
      store(view, target, target.level === "default" ? { profileId: "mame" } : {});
      return snapshot();
    },
    resolve_controls: (args: Args) => {
      const machine = args.machine as string;
      const software = args.software as { list: string; name: string } | null;
      const targets: AssignmentTarget[] = [
        { level: "default" },
        { level: "machine", key: machine },
      ];
      if (software) {
        targets.push({ level: "software", key: `${software.list}:${software.name}` });
        targets.push({
          level: "machineSoftware",
          key: `${machine}/${software.list}:${software.name}`,
        });
      }
      const levels: LevelAssignment[] = targets.map((target) => ({
        ...target,
        assignment: stored(view, target) ?? null,
      }));
      let profileId = "mame";
      let profileSource: ControlsSource = "default";
      let players: string[] = [];
      let playersSource: ControlsSource | null = null;
      for (const level of levels) {
        if (level.assignment?.profileId !== undefined) {
          profileId = level.assignment.profileId;
          profileSource = level.level;
        }
        if (level.assignment?.players !== undefined) {
          players = level.assignment.players;
          playersSource = level.level;
        }
      }
      const effective = profile(profileId);
      return {
        profileId,
        profileSource,
        players,
        playersSource,
        profileName: effective?.name ?? profileId,
        levels,
        cfgDirectory: effective?.managed
          ? `C:\\YMDB ✓\\installations\\${String(args.installationId)}\\controller-profiles\\${profileId}\\cfg`
          : null,
      } satisfies ControlsResolution;
    },
  };
  return {
    handlers,
    view: () => view,
    /** The file the next import dialog returns; `null` cancels it. */
    setNextBase: (name: string | null) => {
      nextBase = name;
    },
    replace: (next: ControllersView) => {
      view = structuredClone(next);
    },
  };
}
