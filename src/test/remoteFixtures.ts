import type {
  CategoryInfo,
  ConnectionSummary,
  RemoteSourcesConfig,
  RemoteSourcesView,
} from "../features/remote/remoteApi";

export const NAS_ID = "11111111111111111111111111111111";
export const BACKUP_ID = "22222222222222222222222222222222";

export const FINGERPRINT = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";
export const OTHER_FINGERPRINT = "SHA256:uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s";

export function connection(
  id: string,
  name: string,
  overrides: Partial<ConnectionSummary["providerConfig"]> = {},
  privateKeyFound = true,
): ConnectionSummary {
  return {
    id,
    name,
    providerType: "sftp",
    providerConfig: {
      host: "nas.local",
      port: 22,
      username: "mame",
      privateKeyPath: "C:\\Users\\me\\.ssh\\id_ed25519",
      hostKeyVerification: "ymdb",
      approvedHostKey: null,
      ...overrides,
    },
    privateKeyFound,
  };
}

export const CATEGORIES: CategoryInfo[] = [
  {
    id: "machineRoms",
    label: "Machine ROMs",
    searchPath: "rom",
    organizations: ["merged", "split", "nonMerged"],
  },
  {
    id: "machineChds",
    label: "Machine CHDs",
    searchPath: "rom",
    organizations: ["merged", "split", "nonMerged"],
  },
  {
    id: "softwareListRoms",
    label: "Software List ROMs",
    searchPath: "rom",
    organizations: ["merged", "split", "nonMerged"],
  },
  {
    id: "softwareListChds",
    label: "Software List CHDs",
    searchPath: "rom",
    organizations: ["merged", "split", "nonMerged"],
  },
  {
    id: "samples",
    label: "Samples",
    searchPath: "sample",
    organizations: ["zipArchives", "directories"],
  },
];

/** The remote sources view of an installation with the given configuration. */
export function remoteSourcesView(
  installationId: string,
  config: RemoteSourcesConfig = { schemaVersion: 1, categories: {} },
  connections: ConnectionSummary[] = [connection(NAS_ID, "Home NAS")],
): RemoteSourcesView {
  return {
    installationId,
    config,
    loadError: null,
    categories: CATEGORIES,
    connections: connections.map((entry) => ({
      id: entry.id,
      name: entry.name,
      providerType: entry.providerType,
    })),
    mameDirectories: [
      {
        searchPath: "rom",
        origin: "mameIni",
        directories: [
          { entry: "roms", path: "C:\\MAME\\roms", status: "available" },
          { entry: "D:\\Shared ROMs", path: "D:\\Shared ROMs", status: "missing" },
        ],
      },
      {
        searchPath: "sample",
        origin: "mameIni",
        directories: [{ entry: "samples", path: "C:\\MAME\\samples", status: "available" }],
      },
    ],
    iniFile: "C:\\MAME\\mame.ini",
    warnings: [],
  };
}
