import { invokeCommand } from "../../shared/ipc";
import type { SupportStatus } from "../search/searchApi";

// These types mirror the Rust contracts in `src-tauri/src/catalog/detail/`.
// Keep both sides in sync.

/** Reference to another record; `description` is `null` when it cannot be resolved. */
export interface RecordLink {
  name: string;
  description: string | null;
}

export interface NameValue {
  name: string;
  value: string | null;
}

export interface DriverDetail {
  status: string | null;
  emulation: string | null;
  cocktail: string | null;
  saveState: string | null;
  requiresArtwork: boolean;
  unofficial: boolean;
  noSoundHardware: boolean;
  incomplete: boolean;
}

export interface FeatureDetail {
  featureType: string;
  status: string | null;
  overall: string | null;
}

export interface InputDetail {
  players: number | null;
  coins: number | null;
  service: boolean;
  tilt: boolean;
}

export interface ControlDetail {
  controlType: string;
  player: number | null;
  buttons: number | null;
  requiredButtons: number | null;
  ways: string | null;
  ways2: string | null;
  ways3: string | null;
}

export interface DisplayDetail {
  tag: string | null;
  displayType: string;
  rotate: number | null;
  flipX: boolean;
  width: number | null;
  height: number | null;
  refresh: number | null;
}

export interface ChipDetail {
  chipType: string;
  name: string;
  tag: string | null;
  clock: number | null;
}

export interface BiosSetDetail {
  name: string;
  description: string;
  isDefault: boolean;
}

export interface RomDetail {
  name: string;
  bios: string | null;
  size: number;
  crc: string | null;
  sha1: string | null;
  mergeName: string | null;
  region: string | null;
  offset: string | null;
  status: string;
  isOptional: boolean;
}

export interface DiskDetail {
  name: string;
  sha1: string | null;
  mergeName: string | null;
  region: string | null;
  index: number | null;
  isWritable: boolean;
  status: string;
  isOptional: boolean;
}

export interface DeviceDetail {
  device: RecordLink;
  instanceCount: number;
}

export interface MediaDeviceDetail {
  deviceType: string;
  tag: string | null;
  interface: string | null;
  mandatory: string | null;
  fixedImage: string | null;
  instanceName: string | null;
  instanceBriefName: string | null;
  extensions: string[];
}

/** A software list declared by a machine; not a compatibility guarantee. */
export interface MachineSoftwareList {
  name: string;
  tag: string | null;
  status: string;
  filter: string | null;
  description: string | null;
  /** Whether the list exists in the imported software lists. */
  available: boolean;
}

export interface RamOptionDetail {
  name: string | null;
  size: number;
  isDefault: boolean;
}

/** Mirrors `MachineDetail` in `detail/machine.rs`. */
/** Mirrors `MachineClassifications` in `catalog/classification.rs`. */
export interface MachineClassifications {
  genres: string[];
  /** Category paths, levels joined with ` / `. */
  categories: string[];
  series: string[];
  versionAdded: string | null;
}

export interface MachineDetail {
  name: string;
  description: string;
  year: string | null;
  manufacturer: string | null;
  sourceFile: string | null;
  isBios: boolean;
  isDevice: boolean;
  isMechanical: boolean;
  isRunnable: boolean;
  parent: RecordLink | null;
  romParent: RecordLink | null;
  sampleOf: string | null;
  cloneCount: number;
  driver: DriverDetail | null;
  features: FeatureDetail[];
  soundChannels: number | null;
  input: InputDetail | null;
  controls: ControlDetail[];
  displays: DisplayDetail[];
  chips: ChipDetail[];
  biosSets: BiosSetDetail[];
  roms: RomDetail[];
  disks: DiskDetail[];
  devices: DeviceDetail[];
  mediaDevices: MediaDeviceDetail[];
  softwareLists: MachineSoftwareList[];
  ramOptions: RamOptionDetail[];
  samples: string[];
  /** From the active support file sources, not from MAME. */
  classifications: MachineClassifications;
}

export interface SoftwareRomDetail {
  name: string | null;
  size: number | null;
  length: number | null;
  crc: string | null;
  sha1: string | null;
  offset: string | null;
  value: string | null;
  status: string;
  loadFlag: string | null;
}

export interface DataAreaDetail {
  name: string;
  size: number;
  databits: number;
  endian: string;
  roms: SoftwareRomDetail[];
}

export interface SoftwareDiskDetail {
  name: string;
  sha1: string | null;
  status: string;
  isWriteable: boolean;
}

export interface DiskAreaDetail {
  name: string;
  disks: SoftwareDiskDetail[];
}

export interface PartDetail {
  name: string;
  interface: string;
  features: NameValue[];
  dataAreas: DataAreaDetail[];
  diskAreas: DiskAreaDetail[];
}

/** Mirrors `SoftwareDetail` in `detail/software.rs`. */
export interface SoftwareDetail {
  listName: string;
  listDescription: string | null;
  name: string;
  description: string;
  year: string | null;
  publisher: string | null;
  notes: string | null;
  supported: SupportStatus;
  parent: RecordLink | null;
  cloneCount: number;
  machineCount: number;
  info: NameValue[];
  sharedFeatures: NameValue[];
  parts: PartDetail[];
}

/** Mirrors `SoftwareListDetail` in `detail/software.rs`. */
export interface SoftwareListDetail {
  name: string;
  description: string | null;
  notes: string | null;
  /** False for lists referenced by machines but not in the imported software lists. */
  available: boolean;
  softwareCount: number;
  originalMachineCount: number;
  compatibleMachineCount: number;
}

/** Resolves to `null` when the machine is not in the installation's catalog. */
export function getMachineDetail(
  installationId: string,
  name: string,
): Promise<MachineDetail | null> {
  return invokeCommand("get_machine_detail", { installationId, name });
}

export function getSoftwareDetail(
  installationId: string,
  list: string,
  name: string,
): Promise<SoftwareDetail | null> {
  return invokeCommand("get_software_detail", { installationId, list, name });
}

export function getSoftwareListDetail(
  installationId: string,
  name: string,
): Promise<SoftwareListDetail | null> {
  return invokeCommand("get_software_list_detail", { installationId, name });
}
