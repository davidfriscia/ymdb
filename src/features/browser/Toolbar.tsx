import type { Dispatch } from "react";
import { CARD_SIZE, type BrowserPreferences } from "./browserPreferences";
import type { MachineSort, SoftwareSort } from "../search/searchApi";
import { sortOptions, type BrowserAction, type ScopeQuery } from "./browserState";
import type { Scope } from "./filterDefinitions";
import { RENDERERS, isRendererId } from "./renderers/registry";

const SORT_LABELS: Record<MachineSort | SoftwareSort, string> = {
  relevance: "Best match",
  name: "Short name",
  description: "Description",
  year: "Year",
  manufacturer: "Manufacturer",
  publisher: "Publisher",
  list: "Software list",
};

interface ToolbarProps {
  scope: Scope;
  text: string;
  appliedText: string;
  query: ScopeQuery;
  filterCount: number;
  preferences: BrowserPreferences;
  /** Whether the current renderer uses the item size. */
  sizable: boolean;
  dispatch: Dispatch<BrowserAction>;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
}

export function Toolbar({
  scope,
  text,
  appliedText,
  query,
  filterCount,
  preferences,
  sizable,
  dispatch,
  onPreferencesChange,
}: ToolbarProps) {
  const hasText = appliedText.trim() !== "";
  const sort = query.sort === "relevance" && !hasText ? null : query.sort;
  const shownSort = sort ?? (hasText ? "relevance" : "name");

  return (
    <div className="toolbar" role="search">
      <button
        type="button"
        className="button button--secondary toolbar__filters"
        aria-pressed={preferences.filtersVisible}
        title={preferences.filtersVisible ? "Hide filters" : "Show filters"}
        onClick={() => {
          onPreferencesChange({ filtersVisible: !preferences.filtersVisible });
        }}
      >
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
          <path
            d="M1.5 3h13l-5 6v4.5l-3-1.5V9z"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
          />
        </svg>
        <span>Filters</span>
        {filterCount > 0 && <span className="counter">{filterCount}</span>}
      </button>

      <select
        className="input toolbar__scope"
        aria-label="Catalog type"
        value={scope}
        onChange={(event) => {
          dispatch({ type: "setScope", scope: event.target.value as Scope });
        }}
      >
        <option value="machines">Machines</option>
        <option value="software">Software</option>
      </select>

      <form
        className="toolbar__search"
        onSubmit={(event) => {
          event.preventDefault();
          dispatch({ type: "applyText" });
        }}
      >
        <input
          type="search"
          className="input toolbar__text"
          aria-label="Search text"
          placeholder={
            scope === "machines"
              ? "Search name, description, manufacturer, year"
              : "Search name, description, publisher, year, list"
          }
          value={text}
          onChange={(event) => {
            dispatch({ type: "setText", text: event.target.value });
          }}
        />
        <button type="submit" className="button">
          Search
        </button>
      </form>

      <div className="toolbar__group">
        <select
          className="input"
          aria-label="Sort by"
          value={shownSort}
          onChange={(event) => {
            dispatch({ type: "setSort", sort: event.target.value as MachineSort | SoftwareSort });
          }}
        >
          {sortOptions(scope).map((option) => (
            <option key={option} value={option} disabled={option === "relevance" && !hasText}>
              {SORT_LABELS[option]}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="button button--secondary"
          aria-label={`Sort direction: ${query.direction}`}
          title={query.direction === "ascending" ? "Ascending" : "Descending"}
          onClick={() => {
            dispatch({
              type: "setDirection",
              direction: query.direction === "ascending" ? "descending" : "ascending",
            });
          }}
        >
          {query.direction === "ascending" ? "↑ Asc" : "↓ Desc"}
        </button>
      </div>

      {RENDERERS.length > 1 && (
        <select
          className="input"
          aria-label="Result view"
          value={preferences.renderer}
          onChange={(event) => {
            if (isRendererId(event.target.value)) {
              onPreferencesChange({ renderer: event.target.value });
            }
          }}
        >
          {RENDERERS.map((renderer) => (
            <option key={renderer.id} value={renderer.id}>
              {renderer.label}
            </option>
          ))}
        </select>
      )}

      {sizable && (
        <label className="toolbar__size">
          <span>Card size</span>
          <input
            type="range"
            min={CARD_SIZE.min}
            max={CARD_SIZE.max}
            step={CARD_SIZE.step}
            value={preferences.cardSize}
            onChange={(event) => {
              onPreferencesChange({ cardSize: Number(event.target.value) });
            }}
          />
        </label>
      )}
    </div>
  );
}
