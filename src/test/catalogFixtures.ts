import { NEW_ID } from "./installationFixtures";
import type { CatalogStatus } from "../features/catalog/catalogApi";
import type {
  MachineDetail,
  SoftwareDetail,
  SoftwareListDetail,
} from "../features/browser/detailApi";
import type {
  FacetResult,
  MachineSummary,
  SearchResponse,
  SoftwareSummary,
} from "../features/search/searchApi";

/** Test data shaped like the backend responses. */

export function machine(name: string, overrides: Partial<MachineSummary> = {}): MachineSummary {
  return {
    name,
    description: `${name} description`,
    year: "1980",
    manufacturer: "Namco",
    cloneOf: null,
    isBios: false,
    isDevice: false,
    isMechanical: false,
    isRunnable: true,
    driverStatus: "good",
    emulationStatus: "good",
    ...overrides,
  };
}

export function software(
  listName: string,
  name: string,
  overrides: Partial<SoftwareSummary> = {},
): SoftwareSummary {
  return {
    listName,
    listDescription: `${listName} list`,
    name,
    description: `${name} description`,
    year: "1985",
    publisher: "Nintendo",
    cloneOf: null,
    supported: "yes",
    ...overrides,
  };
}

export function response<T, F>(
  items: T[],
  options: Partial<SearchResponse<T, F>> & { facets?: FacetResult<F>[] } = {},
): SearchResponse<T, F> {
  const pageSize = options.pageSize ?? 50;
  const total = options.total ?? items.length;
  return {
    items,
    total,
    page: options.page ?? 0,
    pageSize,
    pageCount: Math.ceil(total / pageSize),
    facets: options.facets ?? [],
    elapsedMs: options.elapsedMs ?? 1.5,
  };
}

export function catalogStatus(overrides: Partial<CatalogStatus> = {}): CatalogStatus {
  return {
    catalog: {
      schemaVersion: 3,
      mameRelease: "0.280",
      mameRevision: "mame0280",
      mameBuild: "0.280 (mame0280)",
      executablePath: "/opt/mame/mame",
      importedAt: 1_700_000_000_000,
      importDurationMs: 1000,
      databaseSize: 1024,
      statistics: {
        machines: 3,
        devices: 0,
        bios: 0,
        mechanical: 0,
        clones: 1,
        runnableSystems: 3,
        roms: 3,
        disks: 0,
        softwareLists: 1,
      },
      software: {
        mameRelease: "0.280",
        mameRevision: "mame0280",
        importedAt: 1_700_000_000_000,
        importDurationMs: 1000,
        matchesMachineCatalog: true,
        statistics: {
          lists: 1,
          software: 2,
          clones: 0,
          partiallySupported: 0,
          unsupported: 0,
          parts: 2,
          dataAreas: 2,
          roms: 2,
          diskAreas: 0,
          disks: 0,
          missingReferencedLists: 0,
        },
      },
    },
    installationId: NEW_ID,
    catalogError: null,
    freshness: "current",
    import: { state: "idle" },
    ...overrides,
  };
}

export const emptyCatalogStatus: CatalogStatus = {
  installationId: NEW_ID,
  catalog: null,
  catalogError: null,
  freshness: null,
  import: { state: "idle" },
};

export function machineDetail(name: string, overrides: Partial<MachineDetail> = {}): MachineDetail {
  return {
    name,
    description: `${name} description`,
    year: "1980",
    manufacturer: "Namco",
    sourceFile: "pacman/pacman.cpp",
    isBios: false,
    isDevice: false,
    isMechanical: false,
    isRunnable: true,
    parent: null,
    romParent: null,
    sampleOf: null,
    cloneCount: 0,
    driver: {
      status: "good",
      emulation: "good",
      cocktail: null,
      saveState: "supported",
      requiresArtwork: false,
      unofficial: false,
      noSoundHardware: false,
      incomplete: false,
    },
    features: [],
    soundChannels: 1,
    input: { players: 2, coins: 2, service: false, tilt: false },
    controls: [],
    displays: [],
    chips: [],
    biosSets: [],
    roms: [],
    disks: [],
    devices: [],
    mediaDevices: [],
    softwareLists: [],
    ramOptions: [],
    samples: [],
    classifications: { genres: [], categories: [], series: [], versionAdded: null },
    ...overrides,
  };
}

export function softwareDetail(
  listName: string,
  name: string,
  overrides: Partial<SoftwareDetail> = {},
): SoftwareDetail {
  return {
    listName,
    listDescription: `${listName} list`,
    name,
    description: `${name} description`,
    year: "1985",
    publisher: "Nintendo",
    notes: null,
    supported: "yes",
    parent: null,
    cloneCount: 0,
    machineCount: 1,
    info: [],
    sharedFeatures: [],
    parts: [],
    ...overrides,
  };
}

export function softwareListDetail(
  name: string,
  overrides: Partial<SoftwareListDetail> = {},
): SoftwareListDetail {
  return {
    name,
    description: `${name} list`,
    notes: null,
    available: true,
    softwareCount: 2,
    originalMachineCount: 1,
    compatibleMachineCount: 0,
    ...overrides,
  };
}
