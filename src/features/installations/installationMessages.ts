import type { ExecutableStatus, InstallationSummary } from "./installationsApi";

export function describeExecutableStatus(status: ExecutableStatus): string {
  switch (status) {
    case "verified":
      return "Verified";
    case "notVerified":
      return "Not verified yet: verify it to read its version";
    case "changed":
      return "The executable changed since it was verified: verify it again";
    case "missing":
      return "The executable no longer exists";
  }
}

export function describeCatalog(installation: InstallationSummary): string {
  return installation.catalogImported ? "Catalog imported" : "No catalog";
}
