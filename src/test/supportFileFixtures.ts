import type {
  ClassificationStatus,
  Dimension,
  SupportFileFormat,
  SupportFileImportPreview,
  SupportFilesView,
  SupportSource,
} from "../features/support-files/supportFilesApi";

/** Test data shaped like the support file commands' responses. */

export const NO_CLASSIFICATIONS: ClassificationStatus = { dimensions: [], syncError: null };

const SOURCES: Record<Dimension, SupportFileFormat> = {
  genre: "genre",
  category: "catver",
  series: "series",
  versionAdded: "catver",
};

export function classificationStatus(
  dimensions: Dimension[],
  syncError: string | null = null,
): ClassificationStatus {
  return {
    dimensions: dimensions.map((dimension) => ({
      dimension,
      source: SOURCES[dimension],
      machineCount: 100,
      valueCount: 10,
    })),
    syncError,
  };
}

export function supportSource(
  format: SupportFileFormat,
  overrides: Partial<SupportSource> = {},
): SupportSource {
  const dimensions: Dimension[] =
    format === "catver"
      ? ["category", "versionAdded"]
      : format === "catlist"
        ? ["category"]
        : format === "version"
          ? ["versionAdded"]
          : [format];
  return {
    format,
    fileName: `${format}.ini`,
    declaredVersion: "0.288",
    importedAt: 1_780_000_000_000,
    machineCount: 50_096,
    dimensions: dimensions.map((dimension) => ({
      dimension,
      machineCount: 50_096,
      valueCount: 676,
      active: true,
    })),
    warnings: [],
    ...overrides,
  };
}

/** The view of the given sources; each dimension is active for its first source. */
export function supportFilesView(sources: SupportSource[]): SupportFilesView {
  const dimensions: Dimension[] = ["genre", "category", "series", "versionAdded"];
  return {
    sources,
    dimensions: dimensions.map((dimension) => {
      const available = sources.filter((source) =>
        source.dimensions.some((entry) => entry.dimension === dimension),
      );
      const active = available.find((source) =>
        source.dimensions.some((entry) => entry.dimension === dimension && entry.active),
      );
      return {
        dimension,
        activeSource: active?.format ?? null,
        availableSources: available.map((source) => source.format),
        revision: available.length,
      };
    }),
  };
}

export function importPreview(
  overrides: Partial<SupportFileImportPreview> = {},
): SupportFileImportPreview {
  return {
    importId: 1,
    format: "series",
    fileName: "series.ini",
    declaredVersion: "0.288",
    machineCount: 12_399,
    dimensions: [
      { dimension: "series", machineCount: 12_399, valueCount: 720, activeAfterImport: true },
    ],
    warnings: [],
    replaces: null,
    ...overrides,
  };
}
