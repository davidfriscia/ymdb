import { describe, expect, it } from "vitest";
import { BUILTIN_THEME_TOKENS } from "../../app/theme/builtinThemeTokens";
import { TOKENS_VERSION } from "../../app/theme/themeTokens";
import { buildThemeDocument, parseThemeDocument, THEME_FILE_VERSION } from "./themeFile";
import { themeNameError, uniqueThemeName } from "./themeNames";

const NOW = new Date("2026-11-01T10:00:00Z");

describe("theme files", () => {
  it("exports a versioned document with the name, tokens and metadata only", () => {
    const document = buildThemeDocument(
      "Neon 80s",
      BUILTIN_THEME_TOKENS.neon80s,
      "neon80s",
      NOW,
      "1.2.3",
    );
    expect(document).toEqual({
      format: "ymdb-theme",
      formatVersion: THEME_FILE_VERSION,
      name: "Neon 80s",
      basedOn: "neon80s",
      tokensVersion: TOKENS_VERSION,
      tokens: BUILTIN_THEME_TOKENS.neon80s,
      exportedAt: "2026-11-01T10:00:00.000Z",
      appVersion: "1.2.3",
    });
    // A copy: the built-in tokens are never shared with the document.
    expect(document.tokens).not.toBe(BUILTIN_THEME_TOKENS.neon80s);
    // Only built-in themes are named as a base, never a local custom id.
    expect(
      buildThemeDocument("Mine", BUILTIN_THEME_TOKENS.dark, "custom:0123", NOW, "1"),
    ).not.toHaveProperty("basedOn");
  });

  it("imports an exported document", () => {
    const document = JSON.parse(
      JSON.stringify(buildThemeDocument("Dark", BUILTIN_THEME_TOKENS.dark, "dark", NOW, "1")),
    ) as unknown;
    expect(parseThemeDocument(document)).toEqual({
      ok: true,
      name: "Dark",
      basedOn: "dark",
      tokens: BUILTIN_THEME_TOKENS.dark,
    });
  });

  it("ignores unknown metadata but never unknown tokens", () => {
    const document = {
      ...buildThemeDocument("Light", BUILTIN_THEME_TOKENS.light, "light", NOW, "1"),
      author: "Someone",
    };
    expect(parseThemeDocument(document).ok).toBe(true);
    expect(
      parseThemeDocument({ ...document, tokens: { ...document.tokens, css: "body{}" } }).ok,
    ).toBe(false);
  });

  it("refuses other files, newer versions and invalid tokens", () => {
    const valid = buildThemeDocument("Light", BUILTIN_THEME_TOKENS.light, "light", NOW, "1");
    const cases: [unknown, RegExp][] = [
      [null, /not a YMDB theme/],
      [[valid], /not a YMDB theme/],
      [{ ...valid, format: "other" }, /not a YMDB theme/],
      [{ ...valid, formatVersion: "1" }, /format version/],
      [{ ...valid, formatVersion: 0 }, /format version/],
      [{ ...valid, formatVersion: THEME_FILE_VERSION + 1 }, /newer version/],
      [{ ...valid, tokensVersion: TOKENS_VERSION + 1 }, /newer version/],
      [{ ...valid, tokensVersion: undefined }, /invalid colors/],
      [
        {
          ...valid,
          tokens: { ...valid.tokens, colors: { ...valid.tokens.colors, accent: "red" } },
        },
        /invalid colors/,
      ],
      [
        {
          ...valid,
          tokens: { ...valid.tokens, effects: { ...valid.tokens.effects, glowIntensity: 900 } },
        },
        /invalid colors or effects/,
      ],
    ];
    for (const [document, error] of cases) {
      const parsed = parseThemeDocument(document);
      expect(parsed.ok, JSON.stringify(document)).toBe(false);
      if (!parsed.ok) expect(parsed.error).toMatch(error);
    }
  });

  it("gives a name to documents without a usable one", () => {
    const valid = buildThemeDocument("Light", BUILTIN_THEME_TOKENS.light, undefined, NOW, "1");
    for (const name of [undefined, "", "   ", "Bad\u0007name", 3]) {
      const parsed = parseThemeDocument({ ...valid, name });
      expect(parsed.ok && parsed.name).toBe("Imported theme");
    }
    const parsed = parseThemeDocument({ ...valid, name: "x".repeat(80) });
    expect(parsed.ok && parsed.name.length).toBe(60);
  });
});

describe("theme names", () => {
  const taken = ["Light", "Dark", "Neon 80s", "Sunset"];

  it("refuses empty, long, control and duplicate names, ignoring case and spaces", () => {
    expect(themeNameError("Midnight", taken)).toBeNull();
    expect(themeNameError("  ", taken)).toMatch(/Enter a name/);
    expect(themeNameError("x".repeat(61), taken)).toMatch(/at most 60/);
    expect(themeNameError("Tab\there", taken)).toMatch(/control characters/);
    expect(themeNameError(" sunset ", taken)).toMatch(/already exists/);
    expect(themeNameError("neon 80S", taken)).toMatch(/already exists/);
  });

  it("proposes the first free name", () => {
    expect(uniqueThemeName("Midnight", taken)).toBe("Midnight");
    expect(uniqueThemeName("Sunset", taken)).toBe("Sunset 2");
    expect(uniqueThemeName("Sunset", [...taken, "sunset 2"])).toBe("Sunset 3");
    expect(uniqueThemeName("   ", taken)).toBe("Theme");
  });
});
