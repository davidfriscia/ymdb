import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/controllers/` and
// `src-tauri/src/commands/controllers.rs`. Keep both sides in sync.

/** Id of the system profile that leaves MAME's own configuration in charge. */
export const MAME_PROFILE_ID = "mame";

export type ControllerKind =
  "arcadeStick" | "gamepad" | "wheel" | "trackball" | "lightgun" | "keyboard" | "mouse" | "other";

/** MAME device class (`JOYCODE`, `GUNCODE`, `MOUSECODE`, `KEYCODE`). */
export type DeviceClass = "joystick" | "lightgun" | "mouse" | "keyboard";

/**
 * Mirrors `ControllerBinding` in `controllers/model.rs`: `device` when the
 * device id follows one specific device, `portOrder` when it follows the USB
 * port or the enumeration order of the provider.
 */
export type ControllerBinding = "device" | "portOrder";

/** Mirrors `JoystickProvider` in `controllers/devices.rs`: MAME's `-joystickprovider`. */
export type JoystickProvider = "winhybrid" | "dinput" | "xinput" | "sdljoy";

/**
 * Mirrors `Controller` in `controllers/model.rs`: a logical identity. A
 * joystick with a device id, a provider and a binding can be mapped to players.
 */
export interface Controller {
  id: string;
  name: string;
  kind: ControllerKind;
  deviceClass: DeviceClass;
  deviceId?: string;
  provider?: string;
  /** Missing for controllers registered before device mapping. */
  binding?: ControllerBinding;
}

/** Mirrors `ControllerInput` in `controllers/model.rs`. */
export interface ControllerInput {
  name: string;
  kind: ControllerKind;
  deviceClass: DeviceClass;
  deviceId?: string | null;
  provider?: string | null;
  binding?: ControllerBinding | null;
}

/** Controller ids by player, Player 1 first; `null` leaves a player without controller. */
export type Players = (string | null)[];

/** Mirrors `DeviceIdentity` in `controllers/devices.rs`. */
export type DeviceIdentity = "xinputSlot" | "directInputInstance" | "sdlModel" | "other";

/** Mirrors `DetectedDevice` in `controllers/discovery.rs`. */
export interface DetectedDevice {
  /** MAME's joystick number in this detection. */
  index: number;
  name: string;
  provider: JoystickProvider;
  deviceId: string;
  deviceClass: DeviceClass;
  identity: DeviceIdentity;
  vendorId: string | null;
  productId: string | null;
  serial: string | null;
  /** Whether no other detected device has an id MAME could confuse with it. */
  distinguishable: boolean;
  allowedBindings: ControllerBinding[];
  /** `null` when the device cannot be told apart from another one. */
  suggestedBinding: ControllerBinding | null;
  /** Controllers registered with this provider and device id. */
  registeredControllers: string[];
  notes: string[];
}

/** Mirrors `DeviceDetection` in `controllers/discovery.rs`. */
export interface DeviceDetection {
  installationId: string;
  provider: JoystickProvider;
  devices: DetectedDevice[];
  warnings: string[];
}

export type ProfileKind = "system" | "user";

/** Mirrors `ControllerProfileView` in `commands/controllers.rs`. */
export interface ControllerProfile {
  id: string;
  kind: ProfileKind;
  name: string;
  hasBase: boolean;
  /** Name of the file the base was imported from, when known. */
  baseFileName: string | null;
  /** Whether launches use a configuration directory of YMDB (`false` for `mame`). */
  managed: boolean;
  /** Why the profile's base file cannot be used; the file is kept as it is. */
  baseError: string | null;
}

/** Mirrors `Assignment`: each field left out is inherited from weaker levels. */
export interface Assignment {
  profileId?: string;
  /** Controller ids by player, Player 1 first; `[]` assigns none. */
  players?: Players;
}

export type AssignmentLevel = "default" | "machine" | "software" | "machineSoftware";

/**
 * Mirrors `AssignmentTarget`. Keys: `<machine>`, `<list>:<software>`,
 * `<machine>/<list>:<software>`; none for the default.
 */
export interface AssignmentTarget {
  level: AssignmentLevel;
  key?: string;
}

/** Mirrors `Assignments` in `controllers/model.rs`. */
export interface Assignments {
  default: Assignment;
  machines: Partial<Record<string, Assignment>>;
  software: Partial<Record<string, Assignment>>;
  machineSoftware: Partial<Record<string, Assignment>>;
}

/** Mirrors `ControllersView` in `commands/controllers.rs`. */
export interface ControllersView {
  controllers: Controller[];
  profiles: ControllerProfile[];
  assignments: Assignments;
  /** The joystick providers of this operating system, the one `auto` selects first. */
  providers: JoystickProvider[];
}

/** Mirrors `ControlsSource` in `controllers/resolve.rs`. */
export type ControlsSource = AssignmentLevel | "request";

/** Mirrors `LevelAssignment` in `controllers/resolve.rs`. */
export interface LevelAssignment extends AssignmentTarget {
  assignment: Assignment | null;
}

/** Mirrors `ControlsResolution` in `commands/controllers.rs`. */
export interface ControlsResolution {
  profileId: string;
  profileSource: ControlsSource;
  players: Players;
  playersSource: ControlsSource | null;
  profileName: string;
  /** The levels that apply, weakest first. */
  levels: LevelAssignment[];
  /** Not created until MAME first starts with the profile. */
  cfgDirectory: string | null;
}

export function machineSoftwareKey(machine: string, list: string, software: string): string {
  return `${machine}/${list}:${software}`;
}

export function getControllers(): Promise<ControllersView> {
  return invokeCommand("get_controllers");
}

export function createController(controller: ControllerInput): Promise<ControllersView> {
  return invokeCommand("create_controller", { controller });
}

export function updateController(
  controllerId: string,
  controller: ControllerInput,
): Promise<ControllersView> {
  return invokeCommand("update_controller", { controllerId, controller });
}

/** Refused (`controllersInUse`) while an assignment uses the controller. */
export function deleteController(controllerId: string): Promise<ControllersView> {
  return invokeCommand("delete_controller", { controllerId });
}

/**
 * Creates a user profile. With `importBase`, a native dialog asks for a MAME
 * controller configuration file first; `null` when the user cancels it.
 */
export function createControllerProfile(
  name: string,
  importBase: boolean,
): Promise<ControllersView | null> {
  return invokeCommand("create_controller_profile", { name, importBase });
}

export function renameControllerProfile(profileId: string, name: string): Promise<ControllersView> {
  return invokeCommand("rename_controller_profile", { profileId, name });
}

/** Refused while an assignment uses the profile; its files are kept. */
export function deleteControllerProfile(profileId: string): Promise<ControllersView> {
  return invokeCommand("delete_controller_profile", { profileId });
}

/** Replaces the profile's `base.cfg`; `null` when the user cancels the dialog. */
export function importControllerProfileBase(profileId: string): Promise<ControllersView | null> {
  return invokeCommand("import_controller_profile_base", { profileId });
}

/** An empty assignment resets a machine or software level. */
export function setControllerAssignment(
  target: AssignmentTarget,
  assignment: Assignment,
): Promise<ControllersView> {
  return invokeCommand("set_controller_assignment", { target, assignment });
}

export function resetControllerAssignment(target: AssignmentTarget): Promise<ControllersView> {
  return invokeCommand("reset_controller_assignment", { target });
}

/** The effective controls of a machine (running a software) and their inheritance. */
export function resolveControls(
  installationId: string,
  machine: string,
  software?: { list: string; name: string },
): Promise<ControlsResolution> {
  return invokeCommand("resolve_controls", { installationId, machine, software: software ?? null });
}

/** Mirrors `FieldUpdate` in `controllers/model.rs`: the change of one field. */
export type FieldUpdate<T> = { kind: "keep" } | { kind: "set"; value: T } | { kind: "inherit" };

/** Mirrors `AssignmentUpdate`: each field left out keeps its stored value. */
export interface AssignmentUpdate {
  profileId?: FieldUpdate<string>;
  players?: FieldUpdate<Players>;
}

/**
 * Changes some fields of a saved assignment and keeps the others; a machine
 * or software assignment left empty is removed.
 */
export function updateControllerAssignment(
  target: AssignmentTarget,
  update: AssignmentUpdate,
): Promise<ControllersView> {
  return invokeCommand("update_controller_assignment", { target, update });
}

/**
 * Removes the profile's `base.cfg`. The profile and the configuration MAME
 * saved in its directories are kept.
 */
/**
 * Detects the joysticks connected now with the installation's MAME and
 * `provider` (the default provider when omitted). Nothing is registered.
 * Rejects (`detectionFailed`) when the detection fails: never an empty list.
 */
export function detectControllerDevices(
  installationId: string,
  provider: JoystickProvider | null,
): Promise<DeviceDetection> {
  return invokeCommand("detect_controller_devices", { installationId, provider });
}

export function removeControllerProfileBase(profileId: string): Promise<ControllersView> {
  return invokeCommand("remove_controller_profile_base", { profileId });
}
