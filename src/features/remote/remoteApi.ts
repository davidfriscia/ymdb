import { invokeCommand } from "../../shared/ipc";
import type { SourceStatus } from "../media/mediaApi";

// These types mirror the Rust contracts in `src-tauri/src/remote/` and
// `src-tauri/src/commands/remote.rs`. Keep both sides in sync.

/** Mirrors `ProviderType` in `remote/connections.rs`. Only SFTP exists. */
export type ProviderType = "sftp";

/** Mirrors `HostKeyVerification` in `remote/connections.rs`. */
export type HostKeyVerification = "ymdb" | "knownHosts";

/** Mirrors `ApprovedHostKey` in `remote/connections.rs`. */
export interface ApprovedHostKey {
  algorithm: string;
  /** `SHA256:<base64>`, as printed by OpenSSH. */
  fingerprint: string;
  publicKey: string;
  approvedAt: number;
}

/** Mirrors `SftpSettings` in `remote/connections.rs`: the editable settings. */
export interface SftpSettings {
  host: string;
  port: number;
  username: string;
  /** Path of the private key file; the key itself is never copied. */
  privateKeyPath: string;
  hostKeyVerification: HostKeyVerification;
}

/** Mirrors `SftpConfig` in `remote/connections.rs`. */
export interface SftpConfig extends SftpSettings {
  approvedHostKey: ApprovedHostKey | null;
}

/** Mirrors `ConnectionSummary` in `commands/remote.rs`. */
export interface ConnectionSummary {
  id: string;
  name: string;
  providerType: "sftp";
  providerConfig: SftpConfig;
  privateKeyFound: boolean;
}

/** Mirrors `ConnectionsView` in `commands/remote.rs`. */
export interface ConnectionsView {
  connections: ConnectionSummary[];
  loadError: string | null;
}

/** Mirrors `ConnectionInput` in `remote/connections.rs`; `id` is `null` for a new connection. */
export interface ConnectionInput {
  id: string | null;
  name: string;
  providerType: "sftp";
  providerConfig: SftpSettings;
}

/** Mirrors `RemoteError` in `remote/provider.rs`. */
export type RemoteError =
  | { kind: "hostUnreachable"; message: string }
  | { kind: "timedOut" }
  | {
      kind: "hostKeyUnknown";
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      publicKey: string;
    }
  | {
      kind: "hostKeyChanged";
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      expectedFingerprint: string;
      source: "ymdb" | "knownHosts";
    }
  | { kind: "privateKeyUnavailable"; message: string }
  | { kind: "privateKeyInvalid"; message: string }
  | { kind: "passphraseRequired" }
  | { kind: "passphraseIncorrect" }
  | { kind: "authenticationFailed" }
  | { kind: "permissionDenied"; path: string }
  | { kind: "notFound"; path: string }
  | { kind: "notADirectory"; path: string }
  | { kind: "unsupported"; operation: string }
  | { kind: "connectionLost" }
  | { kind: "cancelled" }
  | { kind: "localIo"; message: string }
  | { kind: "protocol"; message: string };

/** Mirrors `Capabilities` in `remote/provider.rs`. */
export interface Capabilities {
  listDirectory: boolean;
  rangedDownload: boolean;
}

/** Mirrors `ConnectionInfo` in `remote/provider.rs`. */
export interface ConnectionInfo {
  homeDirectory: string | null;
  capabilities: Capabilities;
}

/** Mirrors `ConnectionTestResult` in `commands/remote.rs`. */
export type ConnectionTestResult =
  { status: "success"; info: ConnectionInfo } | { status: "failure"; error: RemoteError };

export type EntryKind = "file" | "directory" | "symlink" | "other";

/** Mirrors `RemoteEntry` in `remote/provider.rs`. */
export interface RemoteEntry {
  name: string;
  kind: EntryKind;
  size: number | null;
  modifiedAt: number | null;
}

/** Mirrors `RemoteListing` in `commands/remote.rs`. */
export interface RemoteListing {
  path: string;
  parent: string | null;
  entries: RemoteEntry[];
  truncated: boolean;
  capabilities: Capabilities;
}

/** Mirrors `SourceCategory` in `remote/sources.rs`. */
export type SourceCategory =
  "machineRoms" | "machineChds" | "softwareListRoms" | "softwareListChds" | "samples";

/** Mirrors `CollectionOrganization` in `remote/sources.rs`. */
export type CollectionOrganization =
  "merged" | "split" | "nonMerged" | "zipArchives" | "directories";

/** Mirrors `LocalDestination` in `remote/sources.rs`. */
export type LocalDestination =
  { mode: "mameDirectory"; directory: string } | { mode: "custom"; directory: string };

/** Mirrors `CategorySource` in `remote/sources.rs`. A disabled category may be incomplete. */
export interface CategorySource {
  enabled: boolean;
  connectionId: string | null;
  remoteBasePath: string | null;
  organization: CollectionOrganization | null;
  destination: LocalDestination | null;
}

/** Mirrors `RemoteSourcesConfig` in `remote/sources.rs`. */
export interface RemoteSourcesConfig {
  schemaVersion: number;
  categories: Partial<Record<SourceCategory, CategorySource>>;
}

export type SearchPath = "rom" | "sample";

/** Mirrors `CategoryInfo` in `commands/remote.rs`. */
export interface CategoryInfo {
  id: SourceCategory;
  label: string;
  searchPath: SearchPath;
  organizations: CollectionOrganization[];
}

/** Mirrors `ConnectionChoice` in `commands/remote.rs`. */
export interface ConnectionChoice {
  id: string;
  name: string;
  providerType: ProviderType;
}

export type DirectoriesOrigin = "launchOptions" | "mameIni" | "mameDefault";

/** Mirrors `SearchPathDirectories` in `commands/remote.rs`. */
export interface SearchPathDirectories {
  searchPath: SearchPath;
  origin: DirectoriesOrigin;
  directories: { entry: string; path: string; status: SourceStatus }[];
}

/** Mirrors `RemoteSourcesView` in `commands/remote.rs`. */
export interface RemoteSourcesView {
  installationId: string;
  config: RemoteSourcesConfig;
  loadError: string | null;
  categories: CategoryInfo[];
  connections: ConnectionChoice[];
  mameDirectories: SearchPathDirectories[];
  iniFile: string | null;
  warnings: { category: SourceCategory; message: string }[];
}

/** Mirrors `ConnectionReference` in `remote/sources.rs`. */
export interface ConnectionReference {
  installationId: string;
  installationLabel: string;
  category: SourceCategory;
}

const call = invokeCommand;

export function getRemoteConnections(): Promise<ConnectionsView> {
  return call("get_remote_connections");
}

/** Creates (`id: null`) or updates a connection. */
export function saveRemoteConnection(connection: ConnectionInput): Promise<ConnectionsView> {
  return call("save_remote_connection", { connection });
}

/** Refused with `connectionInUse` while remote sources reference it. */
export function removeRemoteConnection(connectionId: string): Promise<ConnectionsView> {
  return call("remove_remote_connection", { connectionId });
}

/** Connects for real; the passphrase is used once and never stored. */
export function testRemoteConnection(
  connectionId: string,
  passphrase: string | null,
): Promise<ConnectionTestResult> {
  return call("test_remote_connection", { connectionId, passphrase });
}

/** Approves the server identity presented by the last test. */
export function approveRemoteHostKey(
  connectionId: string,
  fingerprint: string,
): Promise<ConnectionsView> {
  return call("approve_remote_host_key", { connectionId, fingerprint });
}

export function forgetRemoteHostKey(connectionId: string): Promise<ConnectionsView> {
  return call("forget_remote_host_key", { connectionId });
}

/** Lists a remote directory; an empty path lists the server's start directory. */
export function browseRemoteDirectory(
  connectionId: string,
  path: string,
  passphrase: string | null,
): Promise<RemoteListing> {
  return call("browse_remote_directory", { connectionId, path, passphrase });
}

/** Native file picker; `null` when cancelled. */
export function pickPrivateKeyFile(): Promise<string | null> {
  return call("pick_private_key_file");
}

/** Native directory picker; `null` when cancelled. */
export function pickDestinationDirectory(): Promise<string | null> {
  return call("pick_destination_directory");
}

export function getRemoteSources(installationId: string): Promise<RemoteSourcesView> {
  return call("get_remote_sources", { installationId });
}

export function saveRemoteSources(
  installationId: string,
  config: RemoteSourcesConfig,
): Promise<RemoteSourcesView> {
  return call("save_remote_sources", { installationId, config });
}
