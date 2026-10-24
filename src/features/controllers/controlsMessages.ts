import { CommandFailedError } from "../../shared/ipc";
import type {
  Assignment,
  AssignmentLevel,
  AssignmentTarget,
  Assignments,
  ControllerKind,
  ControllersView,
  ControlsSource,
  DeviceClass,
} from "./controllersApi";

export const KIND_LABELS: Record<ControllerKind, string> = {
  arcadeStick: "Arcade stick",
  gamepad: "Gamepad",
  wheel: "Wheel",
  trackball: "Trackball",
  lightgun: "Light gun",
  keyboard: "Keyboard",
  mouse: "Mouse",
  other: "Other",
};

export const DEVICE_CLASS_LABELS: Record<DeviceClass, string> = {
  joystick: "Joystick (JOYCODE)",
  lightgun: "Light gun (GUNCODE)",
  mouse: "Mouse (MOUSECODE)",
  keyboard: "Keyboard (KEYCODE)",
};

export const LEVEL_LABELS: Record<AssignmentLevel, string> = {
  default: "Default",
  machine: "Machine",
  software: "Software",
  machineSoftware: "Machine + Software",
};

/** Where an effective value comes from, without the context of a page. */
export const SOURCE_LABELS: Record<ControlsSource, string> = {
  ...LEVEL_LABELS,
  request: "Temporary selection",
};

/**
 * Where an effective value comes from, seen from the detail page of a
 * machine (`software` false) or of a software. The source is the one
 * returned by the backend; nothing is resolved here.
 */
export function describeSource(source: ControlsSource, software: boolean): string {
  switch (source) {
    case "default":
      return "inherited from the default";
    case "machine":
      return software ? "inherited from the machine" : "specific to this machine";
    case "software":
      return "specific to this software";
    case "machineSoftware":
      return "specific to this software on this machine";
    case "request":
      return "temporary for this launch";
  }
}

/** A saved assignment in words: "Default", "Machine sf2", "Software nes:smb", … */
export function describeTarget(target: AssignmentTarget): string {
  const key = target.key ?? "";
  switch (target.level) {
    case "default":
      return "Default";
    case "machine":
      return `Machine ${key}`;
    case "software":
      return `Software ${key}`;
    case "machineSoftware": {
      const [machine = "", software = ""] = key.split("/");
      return `Software ${software} on ${machine}`;
    }
  }
}

/** Every saved assignment with its target, the default first. */
export function assignmentEntries(
  assignments: Assignments,
): { target: AssignmentTarget; assignment: Assignment }[] {
  const keyed = (level: AssignmentLevel, map: Partial<Record<string, Assignment>>) =>
    Object.entries(map).flatMap(([key, assignment]) =>
      assignment ? [{ target: { level, key }, assignment }] : [],
    );
  return [
    { target: { level: "default" }, assignment: assignments.default },
    ...keyed("machine", assignments.machines),
    ...keyed("software", assignments.software),
    ...keyed("machineSoftware", assignments.machineSoftware),
  ];
}

/** The saved assignments that select a controller profile. */
export function profileReferences(view: ControllersView, profileId: string): AssignmentTarget[] {
  return assignmentEntries(view.assignments)
    .filter(({ assignment }) => assignment.profileId === profileId)
    .map(({ target }) => target);
}

/** The saved assignments that store a controller as a player preference. */
export function controllerReferences(
  view: ControllersView,
  controllerId: string,
): AssignmentTarget[] {
  return assignmentEntries(view.assignments)
    .filter(({ assignment }) => assignment.players?.includes(controllerId) ?? false)
    .map(({ target }) => target);
}

/** Player preferences in words, with the names of the controllers. */
export function describePlayers(view: ControllersView, players: string[]): string {
  if (players.length === 0) return "No controller";
  return players
    .map((id, index) => {
      const name = view.controllers.find((controller) => controller.id === id)?.name ?? id;
      return `Player ${String(index + 1)}: ${name}`;
    })
    .join(", ");
}

export function profileName(view: ControllersView | null, profileId: string): string {
  return view?.profiles.find((profile) => profile.id === profileId)?.name ?? profileId;
}

/** The assignments that prevent a removal, when the backend refused it for that reason. */
export function inUseReferences(error: unknown): AssignmentTarget[] | null {
  return error instanceof CommandFailedError && error.detail.kind === "controllersInUse"
    ? error.detail.references
    : null;
}

/** The name of a profile or controller (1 to 60 characters), as checked by the backend. */
export function validateName(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return "Enter a name.";
  // Characters as the backend counts them (Unicode scalar values).
  if (Array.from(trimmed).length > 60) return "Use at most 60 characters.";
  return null;
}
