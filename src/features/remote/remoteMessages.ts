import type {
  CollectionOrganization,
  ConnectionReference,
  ProviderType,
  RemoteError,
  SourceCategory,
} from "./remoteApi";

export const PROVIDER_LABELS: Record<ProviderType, string> = { sftp: "SFTP" };

export const ORGANIZATION_LABELS: Record<CollectionOrganization, string> = {
  merged: "Merged",
  split: "Split",
  nonMerged: "Non-merged",
  zipArchives: "ZIP archives",
  directories: "Directories",
};

export const CATEGORY_LABELS: Record<SourceCategory, string> = {
  machineRoms: "Machine ROMs",
  machineChds: "Machine CHDs",
  softwareListRoms: "Software List ROMs",
  softwareListChds: "Software List CHDs",
  samples: "Samples",
};

/** A user-facing description of a remote failure (never credentials). */
export function describeRemoteError(error: RemoteError): string {
  switch (error.kind) {
    case "hostUnreachable":
      return `The server cannot be reached: ${error.message}.`;
    case "timedOut":
      return "The server did not answer in time.";
    case "hostKeyUnknown":
      return `The identity of ${error.host}:${String(error.port)} has not been approved yet.`;
    case "hostKeyChanged":
      return `The identity of ${error.host}:${String(error.port)} changed. The connection was refused: this can mean that someone is intercepting the connection.`;
    case "privateKeyUnavailable":
      return `The private key file cannot be read: ${error.message}.`;
    case "privateKeyInvalid":
      return `The private key file is not valid: ${error.message}.`;
    case "passphraseRequired":
      return "The private key is protected by a passphrase.";
    case "passphraseIncorrect":
      return "The passphrase is not correct.";
    case "authenticationFailed":
      return "The server refused the private key for this user.";
    case "permissionDenied":
      return `Access to ${error.path} was denied by the server.`;
    case "notFound":
      return `${error.path} does not exist on the server.`;
    case "notADirectory":
      return `${error.path} is not a directory.`;
    case "unsupported":
      return `This repository does not support ${error.operation}.`;
    case "connectionLost":
      return "The connection was interrupted.";
    case "cancelled":
      return "The operation was cancelled.";
    case "localIo":
      return `A local file could not be written: ${error.message}.`;
    case "protocol":
      return `The server reported an error: ${error.message}.`;
  }
}

export function describeReferences(references: ConnectionReference[]): string {
  return references
    .map((reference) => `${reference.installationLabel} (${CATEGORY_LABELS[reference.category]})`)
    .join(", ");
}
