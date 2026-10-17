import { describe, expect, it } from "vitest";
import type { Dimension } from "../support-files/supportFilesApi";
import {
  activeFilters,
  browserReducer,
  buildFacetRequest,
  buildResultsRequest,
  countActiveFilters,
  initialBrowserState,
  type BrowserAction,
  type BrowserState,
} from "./browserState";
import { buildCategoryTree } from "./filters/facetValues";

function apply(...actions: BrowserAction[]): BrowserState {
  return actions.reduce(browserReducer, initialBrowserState);
}

describe("classification filters", () => {
  it("map classifications and version ranges to the machine filters", () => {
    const state = apply(
      { type: "toggleValue", facet: "genre", value: "Shooter" },
      { type: "toggleValue", facet: "category", value: "Shooter / Flying Vertical" },
      { type: "toggleValue", facet: "series", value: "194X" },
      { type: "toggleValue", facet: "series", value: "Pac-Man" },
      { type: "toggleValue", facet: "versionAdded", value: "0.37b5" },
      { type: "setRange", facet: "versionAdded", range: { from: "0.100" } },
    );
    const { request } = buildResultsRequest("machines", "", state.queries.machines);
    expect(request.filters).toEqual({
      genres: ["Shooter"],
      categories: ["Shooter / Flying Vertical"],
      series: ["194X", "Pac-Man"],
      versionsAdded: ["0.37b5"],
      versionAddedRange: { from: "0.100" },
    });
    expect(countActiveFilters(state.queries.machines.filters)).toBe(6);
    const active = activeFilters("machines", state.queries.machines.filters);
    expect(active.map((filter) => filter.label)).toEqual([
      "Genre: Shooter",
      "Category: Shooter / Flying Vertical",
      "Series: 194X",
      "Series: Pac-Man",
      "Version Added: 0.37b5",
      "Version Added: from 0.100",
    ]);
    // Removing the range chip keeps the exact versions.
    const range = active.at(-1);
    expect(range).toBeDefined();
    if (!range) return;
    const removed = browserReducer(state, range.remove);
    expect(removed.queries.machines.filters.ranges).toEqual({});
    expect(removed.queries.machines.filters.values.versionAdded).toEqual(["0.37b5"]);
  });

  it("format and clear version ranges", () => {
    const both = apply({
      type: "setRange",
      facet: "versionAdded",
      range: { from: "0.1", to: "0.2" },
    });
    expect(activeFilters("machines", both.queries.machines.filters)[0]?.label).toBe(
      "Version Added: 0.1 – 0.2",
    );
    const upTo = apply({ type: "setRange", facet: "versionAdded", range: { to: "0.2" } });
    expect(activeFilters("machines", upTo.queries.machines.filters)[0]?.label).toBe(
      "Version Added: up to 0.2",
    );
    const cleared = browserReducer(both, {
      type: "setRange",
      facet: "versionAdded",
      range: { from: undefined, to: undefined },
    });
    expect(cleared.queries.machines.filters.ranges).toEqual({});
    const facetCleared = browserReducer(both, { type: "clearFacet", facet: "versionAdded" });
    expect(facetCleared.queries.machines.filters.ranges).toEqual({});
  });

  it("are requested as facets only for available dimensions", () => {
    const query = initialBrowserState.queries.machines;
    const facets = (dimensions: Dimension[]) =>
      buildFacetRequest("machines", "", query.filters, query.facetOptions, undefined, dimensions)
        .request.facets;
    expect(facets([])?.map((facet) => facet.facet)).toEqual([
      "manufacturer",
      "year",
      "driverStatus",
      "emulationStatus",
      "softwareList",
    ]);
    expect(facets(["category", "series", "versionAdded"])?.slice(0, 3)).toEqual([
      // Trees and version ranges need every value.
      { facet: "category", limit: 1000 },
      { facet: "series", limit: 50 },
      { facet: "versionAdded", limit: 1000 },
    ]);
  });

  it("replace a facet's selection at once", () => {
    const state = apply(
      { type: "toggleValue", facet: "series", value: "194X" },
      { type: "setPage", page: 3 },
      { type: "setFacetValues", facet: "series", values: ["Pac-Man", "Galaga", "Pac-Man"] },
    );
    expect(state.queries.machines.filters.values.series).toEqual(["Pac-Man", "Galaga"]);
    expect(state.queries.machines.page).toBe(0);
    const cleared = browserReducer(state, { type: "setFacetValues", facet: "series", values: [] });
    expect(cleared.queries.machines.filters.values).toEqual({});
  });
});

describe("saved criteria", () => {
  it("apply atomically, from the first page", () => {
    const before = apply(
      { type: "setText", text: "pac" },
      { type: "applyText" },
      { type: "toggleValue", facet: "manufacturer", value: "Namco" },
      { type: "setPage", page: 4 },
      { type: "setScope", scope: "software" },
      { type: "setPage", page: 2 },
      { type: "openDetail", target: { kind: "softwareList", name: "nes" }, resultIndex: null },
    );
    const filters = {
      values: { series: ["194X"] },
      flags: { clones: "parents" },
      ranges: { versionAdded: { from: "0.30" } },
    };
    const state = browserReducer(before, {
      type: "applyCriteria",
      criteria: { scope: "machines", text: "1942", filters, favoritesOnly: true },
      presetId: "preset-1",
    });
    expect(state.scope).toBe("machines");
    expect(state.text).toBe("1942");
    expect(state.appliedText).toBe("1942");
    expect(state.queries.machines.filters).toEqual(filters);
    expect(state.queries.machines.page).toBe(0);
    expect(state.queries.software.page).toBe(0);
    expect(state.stack).toEqual([]);
    expect(state.presetId).toBe("preset-1");
  });

  it("show the machines of a classification value from a detail", () => {
    const state = apply(
      { type: "setText", text: "1942" },
      { type: "applyText" },
      { type: "toggleValue", facet: "manufacturer", value: "Capcom" },
      { type: "setSort", sort: "year" },
      { type: "openDetail", target: { kind: "machine", name: "1942" }, resultIndex: 0 },
      { type: "showClassification", facet: "series", value: "194X" },
    );
    expect(state.stack).toEqual([]);
    expect(state.text).toBe("");
    expect(state.queries.machines.filters).toEqual({
      values: { series: ["194X"] },
      flags: {},
      ranges: {},
    });
    // The sort is a view choice: it is kept.
    expect(state.queries.machines.sort).toBe("year");
  });
});

describe("category tree", () => {
  it("is derived from the node paths, with any depth", () => {
    const roots = buildCategoryTree(
      [
        { value: "Shooter", count: 3 },
        { value: "Shooter / Flying Vertical", count: 2 },
        { value: "Handheld / Plug n' Play TV Game / Music", count: 1 },
        { value: "Ball & Paddle", count: 1 },
      ],
      ["Maze / Collect"],
    );
    expect(roots.map((node) => [node.label, node.count])).toEqual([
      ["Ball & Paddle", 1],
      ["Handheld", null],
      ["Maze", null],
      ["Shooter", 3],
    ]);
    const handheld = roots[1];
    expect(handheld?.children[0]?.children[0]?.path).toBe(
      "Handheld / Plug n' Play TV Game / Music",
    );
    // A selected node missing from the values is kept, to be removed.
    expect(roots[2]?.children[0]).toMatchObject({ path: "Maze / Collect", count: null });
  });
});
