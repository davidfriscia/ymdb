/**
 * The editable model of a theme and the CSS custom properties it produces.
 *
 * A theme is defined by `ThemeTokens`: validated colors (hex only), a color
 * scheme and a few numeric effects. `themeProperties` turns them into the
 * exact set of semantic tokens declared by `themes.css` for each built-in
 * theme, so custom themes reach the interface only through the same
 * `var(--…)` tokens and never through CSS text. Built-in themes are described
 * in this model too (`builtinThemeTokens.ts`), which is what a clone copies.
 *
 * The format is versioned by `TOKENS_VERSION`: tokens of another version are
 * kept as they are by the backend and never applied or rewritten here.
 */

export const TOKENS_VERSION = 1;

export type ColorScheme = "light" | "dark";

export type ColorGroup =
  "backgrounds" | "typography" | "accents" | "states" | "status" | "bordersEffects";

export const COLOR_GROUPS: { id: ColorGroup; label: string }[] = [
  { id: "backgrounds", label: "Backgrounds" },
  { id: "typography", label: "Typography" },
  { id: "accents", label: "Accents" },
  { id: "states", label: "Interactive states" },
  { id: "status", label: "Status colors" },
  { id: "bordersEffects", label: "Borders and effects" },
];

interface ColorTokenDefinition {
  key: string;
  label: string;
  description: string;
  group: ColorGroup;
  /** The custom property set to this color as is; derived colors have none. */
  property?: string;
}

export const COLOR_TOKENS = [
  {
    key: "appBg",
    label: "Background",
    description: "Behind every page.",
    group: "backgrounds",
    property: "--color-app-bg",
  },
  {
    key: "surface",
    label: "Surface",
    description: "Panels, tabs and the results area.",
    group: "backgrounds",
    property: "--color-surface",
  },
  {
    key: "surfaceElevated",
    label: "Elevated surface",
    description: "Dialogs, menus and popovers.",
    group: "backgrounds",
    property: "--color-surface-elevated",
  },
  {
    key: "sidebarBg",
    label: "Sidebar",
    description: "Main navigation and page sidebars.",
    group: "backgrounds",
    property: "--color-sidebar-bg",
  },
  {
    key: "cardBg",
    label: "Cards",
    description: "Catalog cards and media tiles.",
    group: "backgrounds",
    property: "--color-card-bg",
  },
  {
    key: "inputBg",
    label: "Inputs",
    description: "Text fields and selects.",
    group: "backgrounds",
    property: "--color-input-bg",
  },
  {
    key: "textPrimary",
    label: "Primary text",
    description: "Titles and main content.",
    group: "typography",
    property: "--color-text-primary",
  },
  {
    key: "textSecondary",
    label: "Secondary text",
    description: "Labels and secondary content.",
    group: "typography",
    property: "--color-text-secondary",
  },
  {
    key: "textMuted",
    label: "Muted text",
    description: "Hints, descriptions and counts.",
    group: "typography",
    property: "--color-text-muted",
  },
  {
    key: "textOnAccent",
    label: "Text on accent",
    description: "Labels of primary buttons, on the primary accent.",
    group: "typography",
    property: "--color-text-on-accent",
  },
  {
    key: "accent",
    label: "Primary accent",
    description: "Primary buttons, selection markers and the accent glow.",
    group: "accents",
    property: "--color-accent",
  },
  {
    key: "accentHover",
    label: "Primary accent hover",
    description: "Primary buttons under the pointer.",
    group: "accents",
    property: "--color-accent-hover",
  },
  {
    key: "accentSecondary",
    label: "Secondary accent (highlight)",
    description: "Links and secondary highlights.",
    group: "accents",
    property: "--color-accent-secondary",
  },
  {
    key: "hover",
    label: "Hover",
    description: "Items under the pointer.",
    group: "states",
    property: "--color-hover",
  },
  {
    key: "active",
    label: "Active",
    description: "Pressed buttons and items.",
    group: "states",
    property: "--color-active",
  },
  {
    key: "selected",
    label: "Selected",
    description: "Selected items, tabs and navigation entries.",
    group: "states",
    property: "--color-selected",
  },
  {
    key: "focus",
    label: "Focus",
    description: "Keyboard focus ring and focus glow.",
    group: "states",
    property: "--color-focus",
  },
  {
    key: "disabledBg",
    label: "Disabled background",
    description: "Controls that cannot be used.",
    group: "states",
    property: "--color-disabled-bg",
  },
  {
    key: "disabledText",
    label: "Disabled text",
    description: "Labels of controls that cannot be used.",
    group: "states",
    property: "--color-disabled-text",
  },
  {
    key: "success",
    label: "Success",
    description: "Success messages and badges.",
    group: "status",
    property: "--color-success",
  },
  {
    key: "successBg",
    label: "Success background",
    description: "Behind success messages and badges.",
    group: "status",
    property: "--color-success-bg",
  },
  {
    key: "warning",
    label: "Warning",
    description: "Warning messages and badges.",
    group: "status",
    property: "--color-warning",
  },
  {
    key: "warningBg",
    label: "Warning background",
    description: "Behind warning messages and badges.",
    group: "status",
    property: "--color-warning-bg",
  },
  {
    key: "error",
    label: "Danger",
    description: "Errors and destructive actions.",
    group: "status",
    property: "--color-error",
  },
  {
    key: "errorBg",
    label: "Danger background",
    description: "Behind error messages and badges.",
    group: "status",
    property: "--color-error-bg",
  },
  {
    key: "border",
    label: "Border",
    description: "Panels, cards and separators.",
    group: "bordersEffects",
  },
  {
    key: "borderStrong",
    label: "Strong border",
    description: "Inputs and controls.",
    group: "bordersEffects",
  },
  {
    key: "shadow",
    label: "Shadow",
    description: "Shadow of dialogs and menus.",
    group: "bordersEffects",
  },
  {
    key: "glow",
    label: "Selection glow",
    description: "Glow of selected items. Buttons glow with the primary accent, focus with Focus.",
    group: "bordersEffects",
  },
  {
    key: "overlay",
    label: "Dialog backdrop",
    description: "Behind dialogs, with the backdrop opacity.",
    group: "bordersEffects",
  },
  {
    key: "scrollbarThumb",
    label: "Scrollbar thumb",
    description: "Used when scrollbars follow the theme.",
    group: "bordersEffects",
  },
  {
    key: "scrollbarTrack",
    label: "Scrollbar track",
    description: "Used when scrollbars follow the theme.",
    group: "bordersEffects",
  },
] as const satisfies readonly ColorTokenDefinition[];

export type ColorToken = (typeof COLOR_TOKENS)[number]["key"];

interface EffectDefinition {
  key: string;
  label: string;
  description: string;
  group: ColorGroup;
  min: number;
  max: number;
  step: number;
  /** Formats a value for display. */
  format: (value: number) => string;
}

const percent = (value: number) => `${String(value)}%`;
const pixels = (value: number) => `${String(value)} px`;

export const EFFECTS = [
  {
    key: "glowIntensity",
    label: "Glow intensity",
    description:
      "Glow of primary buttons, selected items, focus and titles. 0 turns every glow off; 100 is the glow of Neon 80s.",
    group: "bordersEffects",
    min: 0,
    max: 200,
    step: 5,
    format: percent,
  },
  {
    key: "shadowIntensity",
    label: "Shadow intensity",
    description: "Opacity of the shadow of dialogs and menus. 0 removes it.",
    group: "bordersEffects",
    min: 0,
    max: 100,
    step: 1,
    format: percent,
  },
  {
    key: "shadowDistance",
    label: "Shadow distance",
    description: "How far the shadow falls below dialogs and menus.",
    group: "bordersEffects",
    min: 0,
    max: 24,
    step: 1,
    format: pixels,
  },
  {
    key: "shadowBlur",
    label: "Shadow blur",
    description: "How soft the shadow is.",
    group: "bordersEffects",
    min: 0,
    max: 48,
    step: 1,
    format: pixels,
  },
  {
    key: "borderEmphasis",
    label: "Border emphasis",
    description: "Blends both border colors toward the primary text color. 0 keeps them as chosen.",
    group: "bordersEffects",
    min: 0,
    max: 100,
    step: 5,
    format: percent,
  },
  {
    key: "overlayOpacity",
    label: "Backdrop opacity",
    description: "Opacity of the backdrop behind dialogs.",
    group: "bordersEffects",
    min: 0,
    max: 100,
    step: 1,
    format: percent,
  },
  {
    key: "headingLetterSpacing",
    label: "Title letter spacing",
    description: "Extra spacing of page, panel and dialog titles, in hundredths of an em.",
    group: "typography",
    min: 0,
    max: 10,
    step: 1,
    format: (value) => (value === 0 ? "Normal" : `${String(value / 100)} em`),
  },
] as const satisfies readonly EffectDefinition[];

export type EffectToken = (typeof EFFECTS)[number]["key"];

export type Scrollbars = "system" | "themed";

export interface ThemeTokens {
  colorScheme: ColorScheme;
  /** Whether scrollbars use the system style or the scrollbar colors. */
  scrollbars: Scrollbars;
  /** Lowercase `#rrggbb` colors. */
  colors: Record<ColorToken, string>;
  /** Integers within the range of their definition. */
  effects: Record<EffectToken, number>;
}

const HEX_COLOR = /^#[0-9a-f]{6}$/;

/** Whether a value is a normalized color of the model (`#rrggbb`, lowercase). */
export function isHexColor(value: unknown): value is string {
  return typeof value === "string" && HEX_COLOR.test(value);
}

/**
 * Normalizes a color typed by the user: `#rgb` or `#rrggbb`, with or without
 * `#`, any case. Anything else (names, functions, CSS text) is refused.
 */
export function parseHexColor(text: string): string | null {
  const match = /^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.exec(text.trim());
  if (!match?.[1]) return null;
  const digits = match[1].toLowerCase();
  return digits.length === 3 ? `#${digits.replace(/./g, "$&$&")}` : `#${digits}`;
}

export function effectDefinition(key: EffectToken) {
  return EFFECTS.find((effect) => effect.key === key) ?? EFFECTS[0];
}

export function isValidEffect(key: EffectToken, value: unknown): value is number {
  const { min, max } = effectDefinition(key);
  return typeof value === "number" && Number.isInteger(value) && value >= min && value <= max;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const present = Object.keys(value);
  return present.length === keys.length && keys.every((key) => key in value);
}

/**
 * Validates stored or imported tokens. Only the current version is
 * accepted; every color and effect must be present and valid, and nothing
 * else may be present. Returns `null` for anything else.
 */
export function parseThemeTokens(version: unknown, value: unknown): ThemeTokens | null {
  if (version !== TOKENS_VERSION || !isRecord(value)) return null;
  if (!hasExactKeys(value, ["colorScheme", "scrollbars", "colors", "effects"])) return null;
  const { colorScheme, scrollbars, colors, effects } = value;
  if (colorScheme !== "light" && colorScheme !== "dark") return null;
  if (scrollbars !== "system" && scrollbars !== "themed") return null;
  if (!isRecord(colors) || !isRecord(effects)) return null;
  const colorKeys = COLOR_TOKENS.map((token) => token.key);
  const effectKeys = EFFECTS.map((effect) => effect.key);
  if (!hasExactKeys(colors, colorKeys) || !hasExactKeys(effects, effectKeys)) return null;
  if (!colorKeys.every((key) => isHexColor(colors[key]))) return null;
  if (!effectKeys.every((key) => isValidEffect(key, effects[key]))) return null;
  return {
    colorScheme,
    scrollbars,
    colors: Object.fromEntries(colorKeys.map((key) => [key, colors[key]])) as ThemeTokens["colors"],
    effects: Object.fromEntries(
      effectKeys.map((key) => [key, effects[key]]),
    ) as ThemeTokens["effects"],
  };
}

/** An independent copy: changing it never changes the source. */
export function copyTokens(tokens: ThemeTokens): ThemeTokens {
  return {
    colorScheme: tokens.colorScheme,
    scrollbars: tokens.scrollbars,
    colors: { ...tokens.colors },
    effects: { ...tokens.effects },
  };
}

export function sameTokens(a: ThemeTokens, b: ThemeTokens): boolean {
  return (
    a.colorScheme === b.colorScheme &&
    a.scrollbars === b.scrollbars &&
    COLOR_TOKENS.every(({ key }) => a.colors[key] === b.colors[key]) &&
    EFFECTS.every(({ key }) => a.effects[key] === b.effects[key])
  );
}

function channels(color: string): [number, number, number] {
  const value = Number.parseInt(color.slice(1), 16);
  return [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
}

/** `rgb(r g b / alpha%)`, the notation of `themes.css`. */
function rgba(color: string, alpha: number): string {
  return `rgb(${channels(color).join(" ")} / ${String(alpha)}%)`;
}

/** Mixes `color` toward `target` by `amount` percent, as a hex color. */
export function mixColors(color: string, target: string, amount: number): string {
  const from = channels(color);
  const to = channels(target);
  return `#${from
    .map((channel, index) =>
      Math.round(channel + ((to[index] ?? channel) - channel) * (amount / 100))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

/**
 * Glows at intensity 100 (Neon 80s): blur in pixels and opacity in percent.
 * The intensity scales the opacity; 0 removes the glow.
 */
const GLOWS = {
  "--glow-accent": { color: "accent", blur: 10, alpha: 45 },
  "--glow-selected": { color: "glow", blur: 8, alpha: 60 },
  "--glow-focus": { color: "focus", blur: 10, alpha: 55 },
} as const satisfies Record<string, { color: ColorToken; blur: number; alpha: number }>;
const TEXT_GLOW = { color: "accent", blur: 8, alpha: 55 } as const;

/** Glow tokens are box shadows combined in lists: `none` would be invalid there. */
const NO_GLOW = "0 0 0 transparent";

/**
 * The CSS custom properties of a theme (plus `color-scheme`), in the order
 * of `themes.css`. Every value is generated from validated tokens.
 */
export function themeProperties(tokens: ThemeTokens): Record<string, string> {
  const { colors, effects } = tokens;
  const properties: Record<string, string> = { "color-scheme": tokens.colorScheme };
  for (const token of COLOR_TOKENS) {
    if ("property" in token) properties[token.property] = colors[token.key];
  }
  const emphasis = effects.borderEmphasis;
  properties["--color-border"] =
    emphasis === 0 ? colors.border : mixColors(colors.border, colors.textPrimary, emphasis / 2);
  properties["--color-border-strong"] =
    emphasis === 0
      ? colors.borderStrong
      : mixColors(colors.borderStrong, colors.textPrimary, emphasis / 2);
  properties["--shadow-elevated"] =
    `0 ${String(effects.shadowDistance)}px ${String(effects.shadowBlur)}px ${rgba(colors.shadow, effects.shadowIntensity)}`;
  properties["--color-overlay"] = rgba(colors.overlay, effects.overlayOpacity);

  const glow = effects.glowIntensity;
  const scaled = (alpha: number) => Math.min(100, Math.round((alpha * glow) / 100));
  for (const [property, { color, blur, alpha }] of Object.entries(GLOWS)) {
    properties[property] =
      glow === 0 ? NO_GLOW : `0 0 ${String(blur)}px ${rgba(colors[color], scaled(alpha))}`;
  }
  properties["--text-glow"] =
    glow === 0
      ? "none"
      : `0 0 ${String(TEXT_GLOW.blur)}px ${rgba(colors[TEXT_GLOW.color], scaled(TEXT_GLOW.alpha))}`;

  properties["--scrollbar-color"] =
    tokens.scrollbars === "themed" ? `${colors.scrollbarThumb} ${colors.scrollbarTrack}` : "auto";
  properties["--heading-letter-spacing"] =
    effects.headingLetterSpacing === 0
      ? "normal"
      : `${String(effects.headingLetterSpacing / 100)}em`;
  return properties;
}

/** Every property `themeProperties` sets, to remove them when a built-in theme applies. */
export const THEME_PROPERTY_NAMES: readonly string[] = [
  "color-scheme",
  ...COLOR_TOKENS.flatMap((token) => ("property" in token ? [token.property] : [])),
  "--color-border",
  "--color-border-strong",
  "--shadow-elevated",
  "--color-overlay",
  ...Object.keys(GLOWS),
  "--text-glow",
  "--scrollbar-color",
  "--heading-letter-spacing",
];
