import { formatCount } from "../../catalog/catalogMessages";
import { formatFacetValue, type FacetFilterDefinition } from "../filterDefinitions";
import type { FacetRow } from "./facetValues";

interface FacetValueListProps {
  definition: FacetFilterDefinition;
  rows: FacetRow[];
  selected: (string | null)[];
  onToggle: (value: string | null) => void;
  /** Scrolls on its own instead of growing (side panel and dialog). */
  scroll?: boolean;
}

/** Checkboxes of facet values, with their counts. */
export function FacetValueList({
  definition,
  rows,
  selected,
  onToggle,
  scroll = true,
}: FacetValueListProps) {
  return (
    <ul
      className={scroll ? "facet__values facet__values--scroll" : "facet__values"}
      aria-label={`${definition.label} values`}
    >
      {rows.map(({ value, count }) => {
        const text = formatFacetValue(definition, value);
        return (
          <li key={value ?? "\u0000"}>
            <label className="facet__value" title={text}>
              <input
                type="checkbox"
                checked={selected.includes(value)}
                onChange={() => {
                  onToggle(value);
                }}
              />
              <span
                className={value === null ? "facet__label facet__label--empty" : "facet__label"}
              >
                {text}
              </span>
              <span className="facet__count">{count === null ? "—" : formatCount(count)}</span>
            </label>
          </li>
        );
      })}
    </ul>
  );
}
