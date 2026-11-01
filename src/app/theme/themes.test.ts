import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { BUILTIN_THEME_TOKENS } from "./builtinThemeTokens";
import {
  DEFAULT_THEME,
  THEMES,
  applyTheme,
  applyThemeTokens,
  customSelection,
  customThemeIdOf,
  isThemeId,
  isThemeSelection,
} from "./themes";
import {
  THEME_PROPERTY_NAMES,
  TOKENS_VERSION,
  parseThemeTokens,
  themeProperties,
} from "./themeTokens";

const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

/** The rule of `themes.css` whose selector names the theme. */
function ruleOf(theme: string): { selector: string; body: string } {
  for (const [, selector = "", body = ""] of themesCss.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (selector.includes(`[data-theme="${theme}"]`)) return { selector, body };
  }
  return { selector: "", body: "" };
}

/** The custom properties declared by a theme. */
function tokensOf(theme: string): string[] {
  return [...ruleOf(theme).body.matchAll(/(--[\w-]+)\s*:/g)].map((match) => match[1] ?? "").sort();
}

describe("themes", () => {
  it("registers light, dark and neon80s, with light as default", () => {
    expect(THEMES.map((theme) => theme.id)).toEqual(["light", "dark", "neon80s"]);
    expect(THEMES.map((theme) => theme.label)).toEqual(["Light", "Dark", "Neon 80s"]);
    expect(DEFAULT_THEME).toBe("light");
  });

  it("recognizes registered theme ids only", () => {
    expect(isThemeId("dark")).toBe(true);
    expect(isThemeId("neon80s")).toBe(true);
    expect(isThemeId("neon")).toBe(false);
    expect(isThemeId("solarized")).toBe(false);
    expect(isThemeId(undefined)).toBe(false);
  });

  it("applies a theme to the document root", () => {
    applyTheme("neon80s");
    expect(document.documentElement.dataset.theme).toBe("neon80s");
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    applyTheme("light");
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
  });

  it("defines the same complete token set for every registered theme", () => {
    const reference = tokensOf("light");
    for (const required of [
      "--color-app-bg",
      "--color-surface",
      "--color-surface-elevated",
      "--color-text-primary",
      "--color-text-secondary",
      "--color-text-muted",
      "--color-accent",
      "--color-accent-secondary",
      "--color-border",
      "--color-hover",
      "--color-active",
      "--color-selected",
      "--color-focus",
      "--color-success",
      "--color-warning",
      "--color-error",
      "--shadow-elevated",
      "--glow-accent",
      "--glow-focus",
    ]) {
      expect(reference).toContain(required);
    }
    for (const theme of THEMES) {
      expect(tokensOf(theme.id), theme.id).toEqual(reference);
    }
  });

  it("lets every registered theme be previewed on a single element", () => {
    for (const theme of THEMES) {
      expect(ruleOf(theme.id).selector, theme.id).toContain(`[data-theme-preview="${theme.id}"]`);
    }
  });

  it("keeps glow out of the light and dark themes", () => {
    for (const theme of ["light", "dark"]) {
      const { body } = ruleOf(theme);
      expect(body).toMatch(/--glow-accent:\s*0 0 0 transparent;/);
      expect(body).toMatch(/--glow-focus:\s*0 0 0 transparent;/);
      expect(body).toMatch(/--text-glow:\s*none;/);
    }
  });

  it("is the only stylesheet with colors, and stylesheets use only defined tokens", () => {
    const stylesheets = readdirSync("src", { recursive: true, encoding: "utf8" })
      .filter((path) => path.endsWith(".css") && !path.endsWith("themes.css"))
      .map((path) => ({ path, css: readFileSync(`src/${path}`, "utf8") }));
    expect(stylesheets.length).toBeGreaterThan(5);
    const defined = new Set(tokensOf("light"));
    for (const { path, css } of stylesheets) {
      expect(css, path).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
      for (const [, token = ""] of css.matchAll(
        /var\((--(?:color|glow|shadow|text-glow)[\w-]*)/g,
      )) {
        expect(defined.has(token), `${path} ${token}`).toBe(true);
      }
    }
  });
});

/** The declarations of a theme's rule, including `color-scheme`. */
function declarationsOf(theme: string): Record<string, string> {
  return Object.fromEntries(
    [...ruleOf(theme).body.matchAll(/(--[\w-]+|color-scheme)\s*:\s*([^;]+);/g)].map((match) => [
      match[1] ?? "",
      (match[2] ?? "").trim(),
    ]),
  );
}

describe("built-in themes in the token model", () => {
  it("produce exactly the declarations of themes.css", () => {
    for (const theme of THEMES) {
      expect(themeProperties(BUILTIN_THEME_TOKENS[theme.id]), theme.id).toEqual(
        declarationsOf(theme.id),
      );
    }
  });

  it("are valid tokens of the current version", () => {
    for (const theme of THEMES) {
      expect(parseThemeTokens(TOKENS_VERSION, BUILTIN_THEME_TOKENS[theme.id]), theme.id).toEqual(
        BUILTIN_THEME_TOKENS[theme.id],
      );
    }
  });

  it("list every generated property for removal", () => {
    expect([...THEME_PROPERTY_NAMES].sort()).toEqual(
      Object.keys(themeProperties(BUILTIN_THEME_TOKENS.light)).sort(),
    );
  });
});

describe("theme selections", () => {
  it("accepts built-in ids and custom ids of the stored form", () => {
    expect(isThemeSelection("neon80s")).toBe(true);
    expect(isThemeSelection(customSelection("0123456789abcdef0123456789abcdef"))).toBe(true);
    expect(isThemeSelection("custom:0123456789ABCDEF0123456789ABCDEF")).toBe(false);
    expect(isThemeSelection("custom:../light")).toBe(false);
    expect(customThemeIdOf("custom:0123456789abcdef0123456789abcdef")).toBe(
      "0123456789abcdef0123456789abcdef",
    );
    expect(customThemeIdOf("dark")).toBeNull();
  });

  it("applies custom tokens inline and removes them for a built-in theme", () => {
    const root = document.createElement("div");
    root.style.setProperty("width", "10px");
    applyThemeTokens(BUILTIN_THEME_TOKENS.neon80s, root);
    expect(root.dataset.theme).toBe("custom");
    expect(root.style.getPropertyValue("--color-accent")).toBe("#ff27d8");
    expect(root.style.getPropertyValue("--glow-focus")).toBe("0 0 10px rgb(52 223 255 / 55%)");
    applyTheme("light", root);
    expect(root.dataset.theme).toBe("light");
    for (const name of THEME_PROPERTY_NAMES) expect(root.style.getPropertyValue(name)).toBe("");
    // Other inline styles are kept.
    expect(root.style.width).toBe("10px");
  });
});
