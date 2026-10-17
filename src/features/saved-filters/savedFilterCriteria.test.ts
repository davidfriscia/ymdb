import { describe, expect, it } from "vitest";
import { emptyFilters, type BrowserCriteria } from "../browser/browserState";
import {
  CRITERIA_VERSION,
  encodeCriteria,
  parseCriteria,
  sameCriteria,
} from "./savedFilterCriteria";

const CRITERIA: BrowserCriteria = {
  scope: "machines",
  text: " 1942 ",
  filters: {
    values: { series: ["194X", "1942"], genre: ["Shooter"], manufacturer: [null, "Capcom"] },
    flags: { clones: "parents" },
    ranges: { versionAdded: { from: "0.30" } },
  },
  favoritesOnly: true,
};

describe("saved filter criteria", () => {
  it("store the search and filters only, normalized", () => {
    expect(encodeCriteria(CRITERIA)).toEqual({
      scope: "machines",
      text: "1942",
      values: { genre: ["Shooter"], manufacturer: [null, "Capcom"], series: ["1942", "194X"] },
      flags: { clones: "parents" },
      ranges: { versionAdded: { from: "0.30" } },
      favoritesOnly: true,
    });
  });

  it("round-trip through the stored form", () => {
    const parsed = parseCriteria(CRITERIA_VERSION, encodeCriteria(CRITERIA));
    expect(parsed.kind).toBe("ok");
    if (parsed.kind !== "ok") return;
    expect(parsed.ignored).toEqual([]);
    expect(sameCriteria(parsed.criteria, CRITERIA)).toBe(true);
  });

  it("compare regardless of value order and empty entries", () => {
    const reordered: BrowserCriteria = {
      ...CRITERIA,
      text: "1942",
      filters: {
        ...CRITERIA.filters,
        values: { ...CRITERIA.filters.values, series: ["1942", "194X"], year: [] },
      },
    };
    expect(sameCriteria(reordered, CRITERIA)).toBe(true);
    expect(sameCriteria({ ...CRITERIA, favoritesOnly: false }, CRITERIA)).toBe(false);
    expect(
      sameCriteria({ ...CRITERIA, filters: { ...CRITERIA.filters, flags: {} } }, CRITERIA),
    ).toBe(false);
  });

  it("of an unknown version cannot be loaded", () => {
    expect(parseCriteria(2, { anything: true })).toEqual({ kind: "unsupported", version: 2 });
  });

  it("report invalid content", () => {
    expect(parseCriteria(1, null).kind).toBe("invalid");
    expect(parseCriteria(1, { scope: "consoles" }).kind).toBe("invalid");
    expect(
      parseCriteria(1, { scope: "machines", text: "", values: {}, flags: {}, ranges: {} }).kind,
    ).toBe("invalid");
  });

  it("leave out filters this version does not know, and say so", () => {
    const parsed = parseCriteria(1, {
      scope: "software",
      text: "",
      values: { list: ["nes"], genre: ["Shooter"], publisher: [1] },
      flags: { clones: "parents", bios: "yes", hasRoms: "maybe" },
      ranges: { versionAdded: { from: "0.1" } },
      favoritesOnly: false,
    });
    expect(parsed).toEqual({
      kind: "ok",
      criteria: {
        scope: "software",
        text: "",
        filters: { ...emptyFilters(), values: { list: ["nes"] }, flags: { clones: "parents" } },
        favoritesOnly: false,
      },
      // Software has no genre, BIOS or version added; `publisher` and `hasRoms` are malformed.
      ignored: ["genre", "publisher", "bios", "hasRoms", "versionAdded"],
    });
  });
});
