import { CommandFailedError } from "../../shared/ipc";
import type { MappingStatus } from "../launcher/launcherApi";
import type {
  Assignment,
  AssignmentLevel,
  AssignmentTarget,
  Assignments,
  Controller,
  ControllerBinding,
  ControllerKind,
  ControllersView,
  ControlsSource,
  DetectedDevice,
  DeviceClass,
  Players,
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

/** Player assignments in words, with the names of the controllers. */
export function describePlayers(view: ControllersView, players: Players): string {
  const assigned = players.flatMap((id, index) => {
    if (id === null) return [];
    const name = view.controllers.find((controller) => controller.id === id)?.name ?? id;
    return [`Player ${String(index + 1)}: ${name}`];
  });
  return assigned.length === 0 ? "No controller" : assigned.join(", ");
}

export const BINDING_LABELS: Record<ControllerBinding, string> = {
  device: "Device binding",
  portOrder: "Port/order binding",
};

export const BINDING_DESCRIPTIONS: Record<ControllerBinding, string> = {
  device:
    "Device binding: the device ID identifies this device wherever it is connected (for example a controller with its own USB serial number).",
  portOrder:
    "Port/order binding: the device ID follows the USB port or the order the provider lists the devices in. It names whatever device is there, not a specific one.",
};

/** Shown for every controller identified by port or enumeration order. */
export const PORT_ORDER_WARNING =
  "This controller is identified by USB port order. Swapping ports may change player assignments.";

/** MAME allows up to ten players. */
export const MAX_PLAYERS = 10;

/** Drops the players without a controller at the end, like the backend. */
export function trimPlayers(players: Players): Players {
  const next = [...players];
  while (next.length > 0 && next[next.length - 1] === null) next.pop();
  return next;
}

/** The controllers registered with the provider and device id of a detected device (exact id, like MAME). */
export function registeredFor(view: ControllersView, device: DetectedDevice): Controller[] {
  const id = device.deviceId.toLowerCase();
  return view.controllers.filter(
    (controller) =>
      controller.provider === device.provider && controller.deviceId?.toLowerCase() === id,
  );
}

/**
 * Why a controller cannot be mapped to a player, mirroring
 * `Controller::device_binding` (the backend checks again at launch).
 */
export function mappingProblem(controller: Controller): string | null {
  if (controller.deviceClass !== "joystick") return "Only joysticks and gamepads can be mapped.";
  if (!controller.deviceId || !controller.provider || !controller.binding)
    return "Device not configured: detect the devices, then set its provider, device ID and binding.";
  return null;
}

export const MAPPING_STATUS_LABELS: Record<MappingStatus, string> = {
  pending: "Waiting for MAME",
  verified: "Applied to this device",
  verifiedPortOrder: "Applied (by port order: the physical device is not guaranteed)",
  missing: "Device not connected",
  notApplied: "Not applied",
  unknown: "Not verifiable",
};

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
