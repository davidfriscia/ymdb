import { describe, expect, it, vi } from "vitest";
import { customTheme, THEME_ID } from "../../test/themeFixtures";
import { applyStartupTheme } from "./startupTheme";

describe("startup theme", () => {
  it("applies a built-in theme without reading the custom themes", async () => {
    const root = document.createElement("div");
    const load = vi.fn();
    await applyStartupTheme("neon80s", root, load);
    expect(root.dataset.theme).toBe("neon80s");
    expect(load).not.toHaveBeenCalled();
  });

  it("applies a saved custom theme before the first render", async () => {
    const root = document.createElement("div");
    await applyStartupTheme(`custom:${THEME_ID}`, root, () => Promise.resolve([customTheme()]));
    expect(root.dataset.theme).toBe("custom");
    expect(root.style.getPropertyValue("--color-accent")).toBe("#ff8800");
  });

  it("falls back to Light for a missing, unusable or unreadable custom theme", async () => {
    for (const load of [
      () => Promise.resolve([]),
      () => Promise.resolve([customTheme({ tokensVersion: 9 })]),
      () => Promise.reject(new Error("invalid file")),
    ]) {
      const root = document.createElement("div");
      await applyStartupTheme(`custom:${THEME_ID}`, root, load);
      expect(root.dataset.theme).toBe("light");
      expect(root.getAttribute("style") ?? "").toBe("");
    }
  });
});
