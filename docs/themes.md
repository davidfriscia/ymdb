# Themes

YMDB has three built-in themes, chosen in **Settings → Appearance**: **Light** (the
default), **Dark** and **Neon 80s**. Any theme can be cloned into a **custom theme**
and edited in the **Theme Editor**, without writing CSS. The theme applies
immediately to the whole application, is kept for the next start and is the same for
every MAME installation.

## Choosing a theme

**Settings → Appearance** lists the **Built-in themes** and the **Custom themes**, each
with a small color preview, its name and a description. Selecting a theme applies it
at once; no restart is needed. The choice is an interface preference (see
[Theme persistence](#theme-persistence)).

## Built-in themes

- **Light**: bright neutral surfaces with a blue accent. The default theme.
- **Dark**: dark neutral surfaces with a light blue accent.
- **Neon 80s**: synthwave and arcade signage. Dark violet surfaces, fuchsia primary
  accent, cyan secondary accent and focus ring, electric violet selections and a
  moderate glow.

Built-in themes are part of the application and never change: they can be selected,
cloned and exported, but not edited, renamed or deleted. Light and Dark have no glow,
`auto` scrollbars and normal title spacing.

## Custom themes

Every custom theme has a stable identifier that never changes, so renaming the theme
in use keeps it selected. The actions of a custom theme are **Edit**, **Clone**,
**Rename**, **Export** and **Delete**; **Import theme…** adds a theme from a file.

### Cloning

**Clone** is available for every theme, built-in or custom. It asks for the name of
the new theme (a free name such as "Neon 80s copy" is proposed), creates an exact,
independent copy of the source's values and opens the Theme Editor. Later changes of
the source never affect the clone. A name already used by any theme, ignoring case
and surrounding spaces, is refused: an existing theme is never replaced.

### Editing

**Edit** opens the Theme Editor, a wide window with:

- **Name**, **Color scheme** (light or dark native controls and scrollbars) and
  **Scrollbars** (system style or the scrollbar colors of the theme).
- Tabs grouping the tokens by meaning: **Backgrounds**, **Typography**, **Accents**,
  **Interactive states**, **Status colors** and **Borders and effects**. Each token
  has a readable name and a description; the CSS property names are never shown.
- For each color, a color picker and a HEX field (`#rgb` or `#rrggbb`) showing the
  same value. An invalid value is reported, is not applied and blocks **Save** until
  it is fixed. **Restore** puts back the saved value of that token.
- Effect sliders: **Glow intensity** (0 turns every glow off; 100 is the glow of Neon
  80s; up to 200), **Shadow intensity**, **Shadow distance**, **Shadow blur**,
  **Border emphasis** (blends both border colors toward the primary text color),
  **Backdrop opacity** and, under Typography, **Title letter spacing**.
- A live **Preview** drawn with the edited values: navigation, typography and a link,
  tabs, buttons (primary, secondary, pressed, disabled), inputs with a focused field,
  cards (normal, selected, hover), badges, messages and a dialog over its backdrop.
  It uses the real classes of the application.

Changes are shown in the preview only: the application keeps its theme and nothing is
saved until **Save**. **Reset changes** returns to the saved values; **Cancel**
discards the changes, so nothing remains applied. Closing the window with Escape or
the close button asks before discarding unsaved changes. Saving the theme in use
applies the new values at once.

### Deleting

**Delete** asks for confirmation. Deleting the theme in use selects **Light**, the
default theme, and says so. A theme whose file was written by a newer version of
YMDB (another token format) is listed as unusable: it cannot be selected, edited,
cloned or exported, but it is kept unchanged and can be renamed or deleted.

## Import and export

**Export** (every theme) saves a portable `.ymdb-theme.json` file:

```json
{
  "format": "ymdb-theme",
  "formatVersion": 1,
  "name": "Neon 80s",
  "basedOn": "neon80s",
  "tokensVersion": 1,
  "tokens": { "colorScheme": "dark", "scrollbars": "themed", "colors": {}, "effects": {} },
  "exportedAt": "2026-11-01T10:00:00.000Z",
  "appVersion": "0.1.0"
}
```

`basedOn` names a built-in theme only. The file never contains local paths, ids,
credentials, other application data, CSS or code.

**Import theme…** reads a file (at most 64 KiB, JSON) and checks the format, the
format and token versions, every color (HEX only) and every effect (integer within
its range). A file of a newer format, or with missing or invalid values, is refused
with a message. A valid file asks for the name of the new theme (a free name is
proposed when the file's name is taken) and always creates a new, independent custom
theme: importing an exported built-in theme gives an editable copy, and no existing
theme is ever overwritten.

## Theme architecture

A theme is a complete set of **semantic design tokens**: CSS custom properties that
name a role (`--color-surface`, `--color-accent`, `--glow-focus`), never a hue.

- `src/app/theme/themes.css` defines the tokens of each built-in theme in one rule,
  `:root[data-theme="<id>"], [data-theme-preview="<id>"]`. The light rule also
  matches `:root`, so Light applies before any theme is set.
- `src/app/theme/themes.ts` registers the built-in themes (`THEMES`: id, label,
  description); `applyTheme` sets `data-theme` on `<html>` and removes any custom
  value; `applyThemeTokens` applies a custom theme.
- `src/app/theme/themeTokens.ts` is the editable model (`ThemeTokens`) and
  `themeProperties`, which generates the full token set from it.
  `builtinThemeTokens.ts` describes the built-in themes in that model: this is what a
  clone copies.
- A custom theme is applied by setting its generated properties inline on `<html>`
  (with `data-theme="custom"`): one `setProperty` per token, no generated stylesheet.
  The Theme Editor preview and the Appearance previews set them on their own element.
- Every stylesheet uses only `var(--…)` tokens: no stylesheet or component contains
  a color. The application uses its own stylesheets (no CSS framework); buttons,
  inputs, selects, modals, tabs, navigation, badges, tables, cards, messages and
  scrollbars all take their colors from the tokens.

## Token model

| Group               | Model keys (`colors`)                                                                     | Tokens                                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Backgrounds         | `appBg`, `surface`, `surfaceElevated`, `sidebarBg`, `cardBg`, `inputBg`                   | `--color-app-bg`, `--color-surface`, `--color-surface-elevated`, `--color-sidebar-bg`, `--color-card-bg`, `--color-input-bg` |
| Typography          | `textPrimary`, `textSecondary`, `textMuted`, `textOnAccent`                               | `--color-text-primary`, `--color-text-secondary`, `--color-text-muted`, `--color-text-on-accent`                             |
| Accents             | `accent`, `accentHover`, `accentSecondary` (links and highlights)                         | `--color-accent`, `--color-accent-hover`, `--color-accent-secondary`                                                         |
| Interactive states  | `hover`, `active`, `selected`, `focus`, `disabledBg`, `disabledText`                      | `--color-hover`, `--color-active`, `--color-selected`, `--color-focus`, `--color-disabled-bg`, `--color-disabled-text`       |
| Status colors       | `success`, `successBg`, `warning`, `warningBg`, `error` (danger), `errorBg`               | `--color-success`, `--color-warning`, `--color-error` and their `-bg` variants                                               |
| Borders and effects | `border`, `borderStrong`, `shadow`, `glow`, `overlay`, `scrollbarThumb`, `scrollbarTrack` | `--color-border`, `--color-border-strong`, `--shadow-elevated`, `--glow-*`, `--color-overlay`, `--scrollbar-color`           |

Derived tokens are computed by `themeProperties`, never typed by the user:

- `--color-border`, `--color-border-strong`: the border colors, blended toward the
  primary text by **Border emphasis** (0 keeps them).
- `--shadow-elevated`: `0 <distance>px <blur>px` with the shadow color at the shadow
  intensity.
- `--glow-accent` (primary accent, 10 px, 45 %), `--glow-selected` (glow color, 8 px,
  60 %), `--glow-focus` (focus color, 10 px, 55 %) and `--text-glow` (primary accent,
  8 px, 55 %): opacities at glow intensity 100, scaled by it and capped at 100 %. At 0
  the box shadows are `0 0 0 transparent` (never `none`, because they are combined
  in lists) and `--text-glow` is `none`.
- `--color-overlay`: the backdrop color at the backdrop opacity.
- `--scrollbar-color`: `auto`, or `<thumb> <track>` when scrollbars follow the theme.
- `--heading-letter-spacing`: `normal`, or the title spacing in em.

Colors are stored as lowercase `#rrggbb`, effects as integers within their ranges;
anything else is refused when a theme is loaded, saved or imported, so no CSS text
can reach the interface. Effects are static shadows: there are no animations, so
`prefers-reduced-motion` needs nothing more, and glow costs one plain `box-shadow`
per element, shown only on hover, focus or selection.

## Schema versioning

- `custom-themes.json` (configuration directory) has `schemaVersion` 1 and holds the
  themes: `id` (32 hex digits, immutable), `name` (1–60 characters, unique ignoring
  case), optional `sourceThemeId` (the theme it was cloned from, for information
  only), `tokensVersion`, `tokens`, `createdAt` and `updatedAt` (milliseconds).
- The backend stores the tokens verbatim and checks the file's structure, names, ids
  and sizes; the frontend validates the tokens (`parseThemeTokens`). A theme with
  another `tokensVersion` is kept, never rewritten.
- The file is written atomically. A file that cannot be read or is invalid (including
  a newer `schemaVersion`) is reported in Appearance and by every command, and is
  never replaced: built-in themes stay available, and nothing is written until the
  file is fixed.
- A new token is a new `TOKENS_VERSION` with a conversion of older tokens; an
  incompatible change of the file is a new `schemaVersion`. Theme files have their
  own `formatVersion`.

## Theme persistence

The selected theme is stored with the other interface preferences in the web view's
`localStorage` (`ymdb.preferences.v2`, field `theme`: `light`, `dark`, `neon80s` or
`custom:<id>`) by `src/app/preferences.ts`. `main.tsx` applies it before the first
render, so the default theme never flashes at startup; for a custom theme it waits
briefly for the custom themes. An invalid value falls back to **Light**. A custom
theme that no longer exists or cannot be used shows **Light** with a message in
Appearance, without changing the preference.

Custom themes (`custom-themes.json`) and the interface preferences (with the selected
theme) are both included in application data backups and restored together (see
[Backup and restore](backup-and-restore.md)).

## Neon 80s design guidelines

- **Palette**: background `#10091f`, surfaces `#1c1231` / `#261843`, fuchsia
  `#ff27d8` (primary accent, with dark text on it), cyan `#34dfff` (links and focus),
  violet `#8528ff` (selection glow), text `#f5f0ff`, muted text `#b9a1e8`.
- **Contrast** (WCAG): primary text is above 14:1 on surfaces, secondary above 11:1,
  muted above 7:1; fuchsia as text is above 5:1 on surfaces and 4.6:1 on selections;
  status colors are above 5.5:1 on their backgrounds. Buttons put dark text on
  fuchsia (about 6:1), because white text on it would not be readable enough.
- **Glow** is reserved for primary buttons, the selected navigation item, tab and
  view mode, pressed toggle buttons, keyboard focus and hover of cards and tiles. It
  is never applied permanently to every card, and nothing is animated.
- **Selection** never relies on color alone: selected items keep their marker
  (left border, checked radio, `aria-pressed` state), background and weight.
- **Typography** stays the system font; page, panel and dialog titles get a slight
  letter spacing and the brand and page titles a soft text glow. No font is loaded
  from the network.
- **Media** keep their original colors: no filter, overlay tint or decorative
  background is applied to images.
- **Performance**: no CSS filters, `backdrop-filter` or continuous animations, so
  large Media Grid pages cost the same as in the other themes.

## How to add a built-in theme

1. Add a rule to `src/app/theme/themes.css` with the selector
   `:root[data-theme="<id>"], [data-theme-preview="<id>"]`, its `color-scheme`, and a
   value for **every** token of the light rule.
2. Describe the same theme in the token model in
   `src/app/theme/builtinThemeTokens.ts`.
3. Register the theme in `THEMES` (`src/app/theme/themes.ts`) with its id, label
   and description. **Settings → Appearance** lists it automatically, and it can be
   cloned and exported.

No page or component changes are needed. `themes.test.ts` checks that every
registered theme defines exactly the tokens of the light theme, can be previewed,
that its model produces exactly its CSS declarations, and that stylesheets contain
no colors and use only defined tokens.
