import type { MachineFacet, SoftwareFacet } from "../search/searchApi";

/** The catalog type being browsed. */
export type Scope = "machines" | "software";

export interface FacetFilterDefinition {
  facet: MachineFacet | SoftwareFacet;
  label: string;
  /** Whether the "no value" entry (`null`) can be selected. */
  nullable: boolean;
  /** Label of the `null` entry. */
  emptyLabel: string;
  formatValue?: (value: string) => string;
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
