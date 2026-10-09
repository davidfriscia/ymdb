import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, THEMES, applyTheme, isThemeId } from "./themes";

describe("themes", () => {
  it("registers light and dark, with light as default", () => {
    expect(THEMES.map((theme) => theme.id)).toEqual(["light", "dark"]);
    expect(DEFAULT_THEME).toBe("light");
  });

  it("recognizes registered theme ids only", () => {
    expect(isThemeId("dark")).toBe(true);
    expect(isThemeId("solarized")).toBe(false);
    expect(isThemeId(undefined)).toBe(false);
  });

  it("applies a theme to the document root", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    applyTheme("light");
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
  });
});
