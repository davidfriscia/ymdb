import type { ComponentType } from "react";
import type { MachineSummary, SoftwareSummary } from "../../search/searchApi";

/** One search result, independent of how it is displayed. */
export type ResultItem =
  | { kind: "machine"; key: string; machine: MachineSummary }
  | { kind: "software"; key: string; software: SoftwareSummary };

/**
 * Contract shared by every result renderer. Renderers only display the items
 * of the current page; they never search, filter, sort or paginate.
 */
export interface ResultRendererProps {
  items: ResultItem[];
  /** Preferred item size in pixels, for renderers that support sizing. */
  itemSize: number;
  /** Opens the detail of the item at this index of `items`. */
  onOpen: (index: number) => void;
}

export interface ResultRendererDefinition {
  id: string;
  label: string;
  /** Whether the renderer uses `itemSize` (shows the size slider). */
  sizable: boolean;
  Component: ComponentType<ResultRendererProps>;
}
