import { beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_PREFERENCES,
  PREFERENCES_KEY,
  loadPreferences,
  savePreferences,
  type UiPreferences,
} from "./preferences";
import { CARD_SIZE } from "../features/browser/browserPreferences";

describe("UI preferences", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to the light theme, visible filters and cards", () => {
    expect(loadPreferences()).toEqual({
      theme: "light",
      cardSize: CARD_SIZE.default,
      filtersVisible: true,
      renderer: "cards",
    });
  });

  it("restores saved preferences", () => {
    const saved: UiPreferences = {
      theme: "dark",
      cardSize: 300,
      filtersVisible: false,
      renderer: "cards",
    };
    savePreferences(saved);

    expect(JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null")).toEqual(saved);
    expect(loadPreferences()).toEqual(saved);
  });

  it("ignores malformed data", () => {
    localStorage.setItem(PREFERENCES_KEY, "{not json");
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);

    localStorage.setItem(PREFERENCES_KEY, JSON.stringify([1, 2]));
    expect(loadPreferences().theme).toBe("light");

    localStorage.setItem(PREFERENCES_KEY, "null");
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);
  });

  it("replaces invalid or obsolete values one by one", () => {
    localStorage.setItem(
      PREFERENCES_KEY,
      JSON.stringify({
        theme: "solarized",
        cardSize: "big",
        filtersVisible: "yes",
        renderer: "table",
        removedOption: true,
      }),
    );
    expect(loadPreferences()).toEqual(DEFAULT_PREFERENCES);

    localStorage.setItem(
      PREFERENCES_KEY,
      JSON.stringify({ theme: "dark", cardSize: 5000, renderer: "table" }),
    );
    expect(loadPreferences()).toEqual({
      ...DEFAULT_PREFERENCES,
      theme: "dark",
      cardSize: CARD_SIZE.max,
    });
  });

  it("clamps and rounds the card size", () => {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ cardSize: 10 }));
    expect(loadPreferences().cardSize).toBe(CARD_SIZE.min);
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ cardSize: 250.6 }));
    expect(loadPreferences().cardSize).toBe(251);
  });

  it("works without storage", () => {
    const failing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
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
