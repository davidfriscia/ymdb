import { describe, expect, it } from "vitest";
import {
  activeFilters,
  browserReducer,
  buildFacetRequest,
  buildResultsRequest,
  countActiveFilters,
  initialBrowserState,
  pageWindow,
  type BrowserAction,
  type BrowserState,
} from "./browserState";

function apply(...actions: BrowserAction[]): BrowserState {
  return actions.reduce(browserReducer, initialBrowserState);
}

describe("browser state", () => {
  it("builds the default machine requests", () => {
    const query = initialBrowserState.queries.machines;
    const { scope, request } = buildResultsRequest("machines", "", query);
    expect(scope).toBe("machines");
    expect(request).toEqual({ direction: "ascending", page: 0, pageSize: 50, filters: {} });
    expect(buildFacetRequest("machines", "", query.filters, query.facetOptions).request).toEqual({
      pageSize: 1,
      filters: {},
      facets: [
        { facet: "manufacturer", limit: 50 },
        { facet: "year", limit: 50 },
        { facet: "driverStatus", limit: 50 },
        { facet: "emulationStatus", limit: 50 },
        { facet: "softwareList", limit: 50 },
      ],
    });
  });

  it("combines facet values and flags into typed filters", () => {
    const state = apply(
      { type: "toggleValue", facet: "manufacturer", value: "Capcom" },
      { type: "toggleValue", facet: "manufacturer", value: null },
      { type: "toggleValue", facet: "softwareList", value: "nes" },
      { type: "setFlag", flag: "clones", value: "parents" },
      { type: "setFlag", flag: "bios", value: "no" },
      { type: "setFlag", flag: "hasDisks", value: "yes" },
    );
    const { request } = buildResultsRequest("machines", " street  ", state.queries.machines);
    expect(request.text).toBe("street");
    expect(request.filters).toEqual({
      manufacturers: ["Capcom", null],
      softwareLists: ["nes"],
      clones: "parents",
      bios: false,
      hasDisks: true,
    });
    expect(countActiveFilters(state.queries.machines.filters)).toBe(6);
  });

  it("maps software filters", () => {
    const state = apply(
      { type: "setScope", scope: "software" },
      { type: "toggleValue", facet: "list", value: "nes" },
      { type: "toggleValue", facet: "supported", value: "partial" },
      { type: "toggleValue", facet: "publisher", value: null },
      { type: "setFlag", flag: "hasRoms", value: "yes" },
    );
    const { scope, request } = buildResultsRequest("software", "", state.queries.software);
    expect(scope).toBe("software");
    expect(request.filters).toEqual({
      lists: ["nes"],
      supported: ["partial"],
      publishers: [null],
      hasRoms: true,
    });
    // Machine filters are kept separately.
    expect(state.queries.machines.filters).toEqual({ values: {}, flags: {} });
  });

  it("toggles and removes filters", () => {
    let state = apply(
      { type: "toggleValue", facet: "year", value: "1980" },
      { type: "toggleValue", facet: "year", value: "1981" },
      { type: "toggleValue", facet: "year", value: "1980" },
    );
    expect(state.queries.machines.filters.values.year).toEqual(["1981"]);

    state = browserReducer(state, { type: "setFlag", flag: "device", value: "yes" });
    state = browserReducer(state, { type: "setFlag", flag: "device", value: null });
    expect(state.queries.machines.filters.flags).toEqual({});

    state = browserReducer(state, { type: "clearFacet", facet: "year" });
    expect(state.queries.machines.filters.values).toEqual({});
  });

  it("returns to the first page when the query changes", () => {
    const paged = apply({ type: "setPage", page: 4 });
    expect(paged.queries.machines.page).toBe(4);

    for (const action of [
      { type: "toggleValue", facet: "year", value: "1980" },
      { type: "setFlag", flag: "bios", value: "yes" },
      { type: "setSort", sort: "year" },
      { type: "setDirection", direction: "descending" },
      { type: "setPageSize", pageSize: 100 },
      { type: "clearFilters" },
    ] satisfies BrowserAction[]) {
      expect(browserReducer(paged, action).queries.machines.page).toBe(0);
    }
    const searched = browserReducer(browserReducer(paged, { type: "setText", text: "pac" }), {
      type: "applyText",
    });
    expect(searched.appliedText).toBe("pac");
    expect(searched.queries.machines.page).toBe(0);
  });

  it("does not search while typing", () => {
    const state = apply({ type: "setText", text: "pac" });
    expect(state.appliedText).toBe("");
    expect(
      buildResultsRequest("machines", state.appliedText, state.queries.machines).request.text,
    ).toBe(undefined);
  });

  it("uses relevance only with search text", () => {
    const state = apply({ type: "setSort", sort: "relevance" });
    expect(
      buildResultsRequest("machines", "", state.queries.machines).request.sort,
    ).toBeUndefined();
    expect(buildResultsRequest("machines", "pac", state.queries.machines).request.sort).toBe(
      "relevance",
    );
    const byYear = apply({ type: "setSort", sort: "year" });
    expect(buildResultsRequest("machines", "", byYear.queries.machines).request.sort).toBe("year");
  });

  it("sends facet value searches and larger limits", () => {
    const state = apply(
      { type: "setFacetSearch", facet: "manufacturer", search: " cap " },
      { type: "showMoreFacetValues", facet: "year" },
      { type: "showMoreFacetValues", facet: "year" },
      { type: "showMoreFacetValues", facet: "year" },
      { type: "showMoreFacetValues", facet: "year" },
    );
    const { filters, facetOptions } = state.queries.machines;
    const { request } = buildFacetRequest("machines", "pac", filters, facetOptions);
    expect(request.text).toBe("pac");
    expect(request.facets?.[0]).toEqual({ facet: "manufacturer", search: "cap", limit: 50 });
    expect(request.facets?.[1]).toEqual({ facet: "year", limit: 200 });
  });

  it("describes active filters with readable labels", () => {
    const state = apply(
      { type: "toggleValue", facet: "driverStatus", value: "imperfect" },
      { type: "toggleValue", facet: "manufacturer", value: null },
      { type: "setFlag", flag: "clones", value: "clones" },
    );
    const active = activeFilters("machines", state.queries.machines.filters);
    expect(active.map((filter) => filter.label)).toEqual([
      "Manufacturer: Unknown",
      "Driver status: Imperfect",
      "Parent / clone: Clones only",
    ]);
    const removed = browserReducer(state, active[1]?.remove ?? { type: "back" });
    expect(removed.queries.machines.filters.values.driverStatus).toEqual([]);
  });

  it("navigates details as a stack", () => {
    let state = apply(
      { type: "openDetail", target: { kind: "machine", name: "pacman" }, resultIndex: 3 },
      { type: "setDetailTab", tab: "roms" },
    );
    state = browserReducer(state, {
      type: "replaceDetail",
      target: { kind: "machine", name: "mspacman" },
      resultIndex: 4,
    });
    expect(state.stack).toEqual([
      { target: { kind: "machine", name: "mspacman" }, resultIndex: 4, tab: "roms" },
    ]);
    state = browserReducer(state, {
      type: "openDetail",
      target: { kind: "softwareList", name: "nes" },
      resultIndex: null,
    });
    expect(state.stack).toHaveLength(2);
    expect(browserReducer(state, { type: "back" }).stack).toHaveLength(1);
    expect(browserReducer(state, { type: "backToResults" }).stack).toHaveLength(0);
    // Changing the catalog type closes the details.
    expect(browserReducer(state, { type: "setScope", scope: "software" }).stack).toHaveLength(0);
  });

  it("lists page numbers around the current page", () => {
    expect(pageWindow(0, 0)).toEqual([]);
    expect(pageWindow(0, 1)).toEqual([0]);
    expect(pageWindow(0, 5)).toEqual([0, 1, 2, null, 4]);
    expect(pageWindow(1, 4)).toEqual([0, 1, 2, 3]);
    expect(pageWindow(10, 50)).toEqual([0, null, 8, 9, 10, 11, 12, null, 49]);
    expect(pageWindow(49, 50)).toEqual([0, null, 47, 48, 49]);
  });
});
