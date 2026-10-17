import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/support_files/`,
// `src-tauri/src/catalog/classification.rs` and
// `src-tauri/src/commands/support_files.rs`. Keep both sides in sync.

/** Mirrors `SupportFileFormat`: one imported source per format. */
export type SupportFileFormat = "catver" | "genre" | "catlist" | "version" | "series";

/** Mirrors `Dimension`: the classifications shown to the user. */
export type Dimension = "genre" | "category" | "series" | "versionAdded";

export const DIMENSIONS: readonly Dimension[] = ["genre", "category", "series", "versionAdded"];

export type WarningKind =
  | "notUtf8"
  | "invalidLine"
  | "invalidMachineName"
  | "duplicateEntry"
  | "conflictingValue"
  | "unrecognizedVersion"
  | "missingSection"
  | "ignoredSection"
  | "entriesOutsideSection";

/** Mirrors `ParseWarning` in `support_files/model.rs`. */
export interface ParseWarning {
  kind: WarningKind;
  count: number;
  examples: string[];
}

/** Mirrors `SourceDimensionView` in `support_files/repository.rs`. */
export interface SourceDimension {
  dimension: Dimension;
  machineCount: number;
  valueCount: number;
  active: boolean;
}

/** Mirrors `SourceView`. */
export interface SupportSource {
  format: SupportFileFormat;
  fileName: string;
  declaredVersion: string | null;
  /** Milliseconds since the Unix epoch. */
  importedAt: number;
  machineCount: number;
  dimensions: SourceDimension[];
  warnings: ParseWarning[];
}

/** Mirrors `DimensionView`. */
export interface DimensionSources {
  dimension: Dimension;
  activeSource: SupportFileFormat | null;
  availableSources: SupportFileFormat[];
  revision: number;
}

/** Mirrors `SupportFilesView`. */
export interface SupportFilesView {
  sources: SupportSource[];
  dimensions: DimensionSources[];
}

/** Mirrors `PreviewDimension` in `commands/support_files.rs`. */
export interface PreviewDimension {
  dimension: Dimension;
  machineCount: number;
  valueCount: number;
  activeAfterImport: boolean;
}

/** Mirrors `SupportFileImportPreview`. */
export interface SupportFileImportPreview {
  importId: number;
  format: SupportFileFormat;
  fileName: string;
  declaredVersion: string | null;
  machineCount: number;
  dimensions: PreviewDimension[];
  warnings: ParseWarning[];
  replaces: SupportSource | null;
}

/** Mirrors `ClassifiedDimension` in `catalog/classification.rs`. */
export interface ClassifiedDimension {
  dimension: Dimension;
  source: SupportFileFormat;
  machineCount: number;
  valueCount: number;
}

/** Mirrors `ClassificationStatus`. */
export interface ClassificationStatus {
  dimensions: ClassifiedDimension[];
  syncError: string | null;
}

/** Mirrors `DimensionValues`. */
export interface DimensionValues {
  dimension: Dimension;
  values: string[];
}

export const FORMAT_LABELS: Record<SupportFileFormat, string> = {
  catver: "CatVer",
  genre: "Genre",
  catlist: "CatList",
  version: "Version",
  series: "Series",
};

export const FORMAT_FILE_NAMES: Record<SupportFileFormat, string> = {
  catver: "catver.ini",
  genre: "genre.ini",
  catlist: "catlist.ini",
  version: "version.ini",
  series: "series.ini",
};

export const DIMENSION_LABELS: Record<Dimension, string> = {
  genre: "Genre",
  category: "Category",
  series: "Series",
  versionAdded: "Version Added",
};

export const WARNING_LABELS: Record<WarningKind, string> = {
  notUtf8: "Not UTF-8: decoded as Windows-1252",
  invalidLine: "Lines that are not valid entries",
  invalidMachineName: "Invalid machine names",
  duplicateEntry: "Duplicate entries (kept once)",
  conflictingValue: "Machines with two values (the first is kept)",
  unrecognizedVersion: "Versions that cannot be ordered",
  missingSection: "Missing sections",
  ignoredSection: "Ignored sections",
  entriesOutsideSection: "Lines outside any section",
};

/** The separator of category levels in projected category paths. */
export const CATEGORY_SEPARATOR = " / ";

export function getSupportFiles(): Promise<SupportFilesView> {
  return invokeCommand("get_support_files");
}

/**
 * Opens the native file picker and checks the file; `null` when cancelled.
 * With `expectedFormat` (reimport), a file of another format is refused.
 */
export function previewSupportFileImport(
  expectedFormat: SupportFileFormat | null = null,
): Promise<SupportFileImportPreview | null> {
  return invokeCommand("preview_support_file_import", { expectedFormat });
}

export function applySupportFileImport(importId: number): Promise<SupportFilesView> {
  return invokeCommand("apply_support_file_import", { importId });
}

/** Removes a source from YMDB; the file it was imported from is not touched. */
export function removeSupportFile(format: SupportFileFormat): Promise<SupportFilesView> {
  return invokeCommand("remove_support_file", { format });
}

export function setActiveSupportSource(
  dimension: Dimension,
  format: SupportFileFormat | null,
): Promise<SupportFilesView> {
  return invokeCommand("set_active_support_source", { dimension, format });
}

/** The classifications of an installation's catalog (brought up to date first). */
export function getClassificationStatus(installationId: string): Promise<ClassificationStatus> {
  return invokeCommand("get_classification_status", { installationId });
}

/** The values that no machine of the catalog has; they match nothing in a search. */
export function findMissingClassificationValues(
  installationId: string,
  values: DimensionValues[],
): Promise<DimensionValues[]> {
  return invokeCommand("find_missing_classification_values", { installationId, values });
}
