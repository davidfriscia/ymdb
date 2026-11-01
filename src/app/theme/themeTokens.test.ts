import { describe, expect, it } from "vitest";
import { BUILTIN_THEME_TOKENS } from "./builtinThemeTokens";
import {
  COLOR_GROUPS,
  COLOR_TOKENS,
  EFFECTS,
  TOKENS_VERSION,
  copyTokens,
  mixColors,
  parseHexColor,
  parseThemeTokens,
  sameTokens,
  themeProperties,
} from "./themeTokens";

const neon = BUILTIN_THEME_TOKENS.neon80s;

function withColor(key: string, value: unknown): unknown {
  return { ...neon, colors: { ...neon.colors, [key]: value } };
}

function withEffect(key: string, value: unknown): unknown {
  return { ...neon, effects: { ...neon.effects, [key]: value } };
}

describe("theme tokens", () => {
  it("covers every color group with labelled tokens", () => {
    for (const group of COLOR_GROUPS) {
      expect(
        COLOR_TOKENS.some((token) => token.group === group.id),
        group.id,
      ).toBe(true);
    }
    for (const token of COLOR_TOKENS) {
      expect(token.label).not.toMatch(/^--|[A-Z]{2}/);
      expect(token.description.length).toBeGreaterThan(5);
    }
    const keys = COLOR_TOKENS.map((token) => token.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("normalizes HEX colors and refuses anything else", () => {
    expect(parseHexColor("#1F5FBF")).toBe("#1f5fbf");
    expect(parseHexColor("1f5fbf")).toBe("#1f5fbf");
    expect(parseHexColor(" #FfF ")).toBe("#ffffff");
    expect(parseHexColor("abc")).toBe("#aabbcc");
    for (const text of [
      "",
      "#",
      "#12",
      "#1234",
      "#12345",
      "#1234567",
      "#12345678",
      "#ggg",
      "red",
      "rgb(0 0 0)",
      "#fff; background: url(x)",
      "var(--color-accent)",
    ]) {
      expect(parseHexColor(text), text).toBeNull();
    }
  });

  it("accepts complete tokens of the current version only", () => {
    expect(parseThemeTokens(TOKENS_VERSION, neon)).toEqual(neon);
    expect(parseThemeTokens(2, neon)).toBeNull();
    expect(parseThemeTokens("1", neon)).toBeNull();
    expect(parseThemeTokens(TOKENS_VERSION, null)).toBeNull();
    expect(parseThemeTokens(TOKENS_VERSION, [])).toBeNull();
    expect(parseThemeTokens(TOKENS_VERSION, { ...neon, colorScheme: "sepia" })).toBeNull();
    expect(parseThemeTokens(TOKENS_VERSION, { ...neon, scrollbars: "thin" })).toBeNull();
    expect(parseThemeTokens(TOKENS_VERSION, { ...neon, css: "body{}" })).toBeNull();
    const withoutAccent: Record<string, string> = { ...neon.colors };
    delete withoutAccent.accent;
    expect(parseThemeTokens(TOKENS_VERSION, { ...neon, colors: withoutAccent })).toBeNull();
  });

  it("refuses colors that are not lowercase #rrggbb, so no CSS can be injected", () => {
    for (const value of [
      "#FF27D8",
      "#fff",
      "red",
      "#ff27d8; color: red",
      "url(javascript:alert(1))",
      "var(--x)",
      12,
      null,
    ]) {
      expect(
        parseThemeTokens(TOKENS_VERSION, withColor("accent", value)),
        String(value),
      ).toBeNull();
    }
    expect(parseThemeTokens(TOKENS_VERSION, withColor("unknownColor", "#000000"))).toBeNull();
  });

  it("refuses effects out of range or not integers", () => {
    for (const effect of EFFECTS) {
      for (const value of [effect.min - 1, effect.max + 1, 1.5, "10", null, Number.NaN]) {
        expect(
          parseThemeTokens(TOKENS_VERSION, withEffect(effect.key, value)),
          effect.key,
        ).toBeNull();
      }
      expect(parseThemeTokens(TOKENS_VERSION, withEffect(effect.key, effect.min))).not.toBeNull();
      expect(parseThemeTokens(TOKENS_VERSION, withEffect(effect.key, effect.max))).not.toBeNull();
    }
  });

  it("turns every glow off at intensity 0", () => {
    const tokens = copyTokens(neon);
    tokens.effects.glowIntensity = 0;
    const properties = themeProperties(tokens);
    expect(properties["--glow-accent"]).toBe("0 0 0 transparent");
    expect(properties["--glow-selected"]).toBe("0 0 0 transparent");
    expect(properties["--glow-focus"]).toBe("0 0 0 transparent");
    expect(properties["--text-glow"]).toBe("none");
  });

  it("scales glow and shadow with their intensity", () => {
    const tokens = copyTokens(neon);
    tokens.effects.glowIntensity = 200;
    tokens.effects.shadowIntensity = 30;
    tokens.effects.shadowDistance = 2;
    tokens.effects.shadowBlur = 8;
    const properties = themeProperties(tokens);
    // Opacities double, never above 100%.
    expect(properties["--glow-accent"]).toBe("0 0 10px rgb(255 39 216 / 90%)");
    expect(properties["--glow-selected"]).toBe("0 0 8px rgb(133 40 255 / 100%)");
    expect(properties["--shadow-elevated"]).toBe("0 2px 8px rgb(0 0 0 / 30%)");
  });

  it("blends borders toward the primary text with the border emphasis", () => {
    const tokens = copyTokens(BUILTIN_THEME_TOKENS.light);
    tokens.effects.borderEmphasis = 100;
    const properties = themeProperties(tokens);
    expect(properties["--color-border"]).toBe(mixColors("#d5dae1", "#1a1d23", 50));
    expect(properties["--color-border"]).not.toBe("#d5dae1");
    expect(mixColors("#000000", "#ffffff", 50)).toBe("#808080");
    expect(mixColors("#123456", "#ffffff", 0)).toBe("#123456");
  });

  it("uses theme scrollbar colors and title spacing only when set", () => {
    const tokens = copyTokens(BUILTIN_THEME_TOKENS.dark);
    expect(themeProperties(tokens)["--scrollbar-color"]).toBe("auto");
    tokens.scrollbars = "themed";
    tokens.effects.headingLetterSpacing = 5;
    expect(themeProperties(tokens)["--scrollbar-color"]).toBe("#4c5361 #171a1f");
    expect(themeProperties(tokens)["--heading-letter-spacing"]).toBe("0.05em");
  });

  it("copies tokens independently", () => {
    const copy = copyTokens(neon);
    expect(sameTokens(copy, neon)).toBe(true);
    copy.colors.accent = "#000000";
    copy.effects.glowIntensity = 0;
    expect(neon.colors.accent).toBe("#ff27d8");
    expect(neon.effects.glowIntensity).toBe(100);
    expect(sameTokens(copy, neon)).toBe(false);
  });
});
