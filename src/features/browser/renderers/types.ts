import type { ComponentType } from "react";
import type { MachineSummary, SoftwareSummary } from "../../search/searchApi";

/** One search result, independent of how it is displayed. */
export type ResultItem =
  | { kind: "machine"; key: string; machine: MachineSummary }
  | { kind: "software"; key: string; software: SoftwareSummary };

/** Values of renderer options, by option key (already normalized). */
export type RendererOptionValues = Readonly<Record<string, number | boolean | string>>;

/** An option a renderer supports; the View Options panel builds its control. */
export type RendererOptionDefinition =
  | {
      key: string;
      type: "range";
      label: string;
      min: number;
      max: number;
      step: number;
      default: number;
      /** Shown after the value, for example "px". */
      unit?: string;
    }
  | { key: string; type: "toggle"; label: string; default: boolean }
  | {
      key: string;
      type: "choice";
      label: string;
      choices: readonly { value: string; label: string }[];
      default: string;
    };

/**
 * Contract shared by every result renderer. Renderers only display the items
 * of the current page; they never search, filter, sort or paginate.
 */
export interface ResultRendererProps {
  items: ResultItem[];
  /** This renderer's options, normalized against its definitions. */
  options: RendererOptionValues;
  /** Opens the detail of the item at this index of `items`. */
  onOpen: (index: number) => void;
}

export interface ResultRendererDefinition {
  /** Stable identifier, saved in the interface preferences. */
  id: string;
  label: string;
  /** SVG path data (24×24 view box, stroked) of the view mode icon. */
  icon: string;
  Component: ComponentType<ResultRendererProps>;
  options: readonly RendererOptionDefinition[];
}
