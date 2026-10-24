import { beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_PREFERENCES,
  LEGACY_PREFERENCES_KEY,
  PREFERENCES_KEY,
  exportStoredPreferences,
  importStoredPreferences,
  loadPreferences,
  savePreferences,
  type UiPreferences,
} from "./preferences";

const store = (key: string, value: unknown) => {
  localStorage.setItem(key, JSON.stringify(value));
};

describe("UI preferences", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to the light theme, the filters and classic cards", () => {
    expect(loadPreferences()).toEqual({
      theme: "light",
      filtersVisible: true,
      viewOptionsVisible: false,
      renderer: "classicCards",
      rendererOptions: {
        classicCards: { cardSize: 220 },
        mediaGrid: {
          tileSize: 220,
          category: "snap",
          fit: "contain",
          showTitle: true,
          showName: false,
          showBadges: true,
        },
      },
      favoritesOnly: false,
      showCommandPreview: false,
    });
  });

  it("restores saved preferences, with options kept per renderer", () => {
    const saved: UiPreferences = {
      ...DEFAULT_PREFERENCES,
      theme: "dark",
      filtersVisible: false,
      viewOptionsVisible: true,
      renderer: "mediaGrid",
      rendererOptions: {
        classicCards: { cardSize: 300 },
        mediaGrid: {
          ...DEFAULT_PREFERENCES.rendererOptions.mediaGrid,
          tileSize: 360,
          category: "flyers",
        },
      },
      showCommandPreview: true,
    };
    savePreferences(saved);

    expect(JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null")).toEqual(saved);
    expect(loadPreferences()).toEqual(saved);
  });

  it("migrates version 1 preferences and drops them on save", () => {
    store(LEGACY_PREFERENCES_KEY, {
      theme: "dark",
      cardSize: 300,
      filtersVisible: false,
      renderer: "cards",
      showCommandPreview: true,
    });

    const migrated = loadPreferences();
    expect(migrated).toEqual({
      ...DEFAULT_PREFERENCES,
      theme: "dark",
      filtersVisible: false,
      renderer: "classicCards",
      rendererOptions: { ...DEFAULT_PREFERENCES.rendererOptions, classicCards: { cardSize: 300 } },
      showCommandPreview: true,
    });

    savePreferences(migrated);
    expect(localStorage.getItem(LEGACY_PREFERENCES_KEY)).toBeNull();
    expect(loadPreferences()).toEqual(migrated);
  });

  it("prefers version 2 over leftover version 1 data", () => {
    store(LEGACY_PREFERENCES_KEY, { theme: "dark" });
    store(PREFERENCES_KEY, { theme: "light", renderer: "mediaGrid" });
    expect(loadPreferences()).toMatchObject({ theme: "light", renderer: "mediaGrid" });
  });

  it("keeps the Favorites only option, separate from the favorites themselves", () => {
    savePreferences({ ...DEFAULT_PREFERENCES, favoritesOnly: true });
    expect(loadPreferences().favoritesOnly).toBe(true);
    store(PREFERENCES_KEY, { favoritesOnly: "yes" });
    expect(loadPreferences().favoritesOnly).toBe(false);
  });

  it("ignores malformed data", () => {
    localStorage.setItem(PREFERENCES_KEY, "{not json");
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);
    store(PREFERENCES_KEY, [1, 2]);
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);
    store(PREFERENCES_KEY, null);
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);
  });

  it("replaces invalid or obsolete values one by one", () => {
    store(PREFERENCES_KEY, {
      theme: "solarized",
      filtersVisible: "yes",
      viewOptionsVisible: 1,
      renderer: "table",
      showCommandPreview: 1,
      rendererOptions: {
        classicCards: { cardSize: 5000, removedOption: true },
        mediaGrid: {
          tileSize: 251,
          category: "music",
          fit: "stretch",
          showTitle: "no",
          showName: true,
        },
        tableRenderer: { columns: 3 },
      },
    });
    expect(loadPreferences()).toEqual({
      ...DEFAULT_PREFERENCES,
      rendererOptions: {
        classicCards: { cardSize: 420 },
        mediaGrid: {
          ...DEFAULT_PREFERENCES.rendererOptions.mediaGrid,
          tileSize: 250,
          showName: true,
        },
      },
    });
  });

  it("clamps sizes to their range", () => {
    store(PREFERENCES_KEY, {
      rendererOptions: { classicCards: { cardSize: 10 }, mediaGrid: { tileSize: 1e9 } },
    });
    const { rendererOptions } = loadPreferences();
    expect(rendererOptions.classicCards?.cardSize).toBe(160);
    expect(rendererOptions.mediaGrid?.tileSize).toBe(480);
  });

  it("works without storage", () => {
    const failing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
      removeItem: () => {
        throw new Error("denied");
      },
    } as unknown as Storage;

    expect(loadPreferences(failing)).toEqual(DEFAULT_PREFERENCES);
    expect(() => {
      savePreferences(DEFAULT_PREFERENCES, failing);
    }).not.toThrow();
    expect(loadPreferences(null)).toEqual(DEFAULT_PREFERENCES);
  });
});

describe("Stored preferences in backups", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("exports the raw stored entries only", () => {
    expect(exportStoredPreferences()).toEqual({});
    localStorage.setItem(PREFERENCES_KEY, '{"theme":"dark"}');
    localStorage.setItem("other", "x");

    expect(exportStoredPreferences()).toEqual({ [PREFERENCES_KEY]: '{"theme":"dark"}' });
  });

  it("replaces the stored entries, ignoring unknown keys and invalid values", () => {
    localStorage.setItem(LEGACY_PREFERENCES_KEY, '{"theme":"dark"}');

    importStoredPreferences({
      [PREFERENCES_KEY]: JSON.stringify({ theme: "dark" }),
      [LEGACY_PREFERENCES_KEY]: 42,
      other: "x",
    });

    expect(localStorage.getItem(PREFERENCES_KEY)).toBe('{"theme":"dark"}');
    expect(localStorage.getItem(LEGACY_PREFERENCES_KEY)).toBeNull();
    expect(localStorage.getItem("other")).toBeNull();
    expect(loadPreferences().theme).toBe("dark");
  });
});
