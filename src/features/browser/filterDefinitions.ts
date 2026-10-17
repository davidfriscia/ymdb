import type { MachineFacet, SoftwareFacet } from "../search/searchApi";
import type { Dimension } from "../support-files/supportFilesApi";

/** The catalog type being browsed. */
export type Scope = "machines" | "software";

/**
 * How the values of a facet are shown: a list (searched by the backend), a
 * tree of category paths or versions with a range (both loaded completely).
 */
export type FacetPresentation = "list" | "tree" | "version";

export interface FacetFilterDefinition {
  facet: MachineFacet | SoftwareFacet;
  label: string;
  /** Whether the "no value" entry (`null`) can be selected. */
  nullable: boolean;
  /** Label of the `null` entry. */
  emptyLabel: string;
  formatValue?: (value: string) => string;
  /**
   * A classification of the support files: shown only when the catalog has
   * it (an active source), since its values come from the support files.
   */
  classification?: Dimension;
  presentation?: FacetPresentation;
}

export interface FlagOption {
  value: string;
  label: string;
}

/** A single-choice filter; no selection means "any". */
export interface FlagFilterDefinition {
  id: string;
  label: string;
  options: FlagOption[];
}

const capitalize = (value: string) => value.charAt(0).toUpperCase() + value.slice(1);

const YES_NO: FlagOption[] = [
  { value: "yes", label: "Yes" },
  { value: "no", label: "No" },
];

const CLONES: FlagFilterDefinition = {
  id: "clones",
  label: "Parent / clone",
  options: [
    { value: "parents", label: "Parents only" },
    { value: "clones", label: "Clones only" },
  ],
};

export const FACET_FILTERS: Record<Scope, FacetFilterDefinition[]> = {
  machines: [
    { facet: "genre", label: "Genre", nullable: false, emptyLabel: "", classification: "genre" },
    {
      facet: "category",
      label: "Category",
      nullable: false,
      emptyLabel: "",
      classification: "category",
      presentation: "tree",
    },
    { facet: "series", label: "Series", nullable: false, emptyLabel: "", classification: "series" },
    {
      facet: "versionAdded",
      label: "Version Added",
      nullable: false,
      emptyLabel: "",
      classification: "versionAdded",
      presentation: "version",
    },
    { facet: "manufacturer", label: "Manufacturer", nullable: true, emptyLabel: "Unknown" },
    { facet: "year", label: "Year", nullable: true, emptyLabel: "Unknown" },
    {
      facet: "driverStatus",
      label: "Driver status",
      nullable: true,
      emptyLabel: "Not declared",
      formatValue: capitalize,
    },
    {
      facet: "emulationStatus",
      label: "Emulation status",
      nullable: true,
      emptyLabel: "Not declared",
      formatValue: capitalize,
    },
    { facet: "softwareList", label: "Software list", nullable: false, emptyLabel: "" },
  ],
  software: [
    { facet: "list", label: "Software list", nullable: false, emptyLabel: "" },
    { facet: "publisher", label: "Publisher", nullable: true, emptyLabel: "Unknown" },
    { facet: "year", label: "Year", nullable: true, emptyLabel: "Unknown" },
    {
      facet: "supported",
      label: "Support status",
      nullable: false,
      emptyLabel: "",
      formatValue: (value) => supportLabel(value),
    },
    { facet: "interface", label: "Part interface", nullable: false, emptyLabel: "" },
  ],
};

export const FLAG_FILTERS: Record<Scope, FlagFilterDefinition[]> = {
  machines: [
    CLONES,
    { id: "bios", label: "BIOS", options: YES_NO },
    { id: "device", label: "Device", options: YES_NO },
    { id: "mechanical", label: "Mechanical", options: YES_NO },
    { id: "runnable", label: "Runnable", options: YES_NO },
    { id: "hasRoms", label: "Declares ROMs", options: YES_NO },
    { id: "hasDisks", label: "Declares CHDs", options: YES_NO },
  ],
  software: [
    CLONES,
    { id: "hasRoms", label: "Declares ROMs", options: YES_NO },
    { id: "hasDisks", label: "Declares disks", options: YES_NO },
  ],
};

export function supportLabel(value: string): string {
  switch (value) {
    case "yes":
      return "Supported";
    case "partial":
      return "Partially supported";
    case "no":
      return "Unsupported";
    default:
      return value;
  }
}

export function formatFacetValue(definition: FacetFilterDefinition, value: string | null): string {
  if (value === null) return definition.emptyLabel;
  return definition.formatValue ? definition.formatValue(value) : value;
}

/** Whether the facet's values are loaded completely (tree and version facets). */
export function isCompleteFacet(definition: FacetFilterDefinition): boolean {
  return definition.presentation === "tree" || definition.presentation === "version";
}

export function facetDefinition(scope: Scope, facet: string): FacetFilterDefinition | undefined {
  return FACET_FILTERS[scope].find((definition) => definition.facet === facet);
}
