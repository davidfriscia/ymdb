# Themes

YMDB has three built-in themes, chosen in **Settings → Appearance**: **Light** (the
default), **Dark** and **Neon 80s**. The theme applies immediately to the whole
application, is kept for the next start and is the same for every MAME installation.

## Choosing a theme

**Settings → Appearance** lists every registered theme with a small color preview,
its name and a short description. Selecting a theme applies it at once; no restart
is needed. The choice is an interface preference (see
[Theme persistence](#theme-persistence)).

## Theme architecture

A theme is a complete set of **semantic design tokens**: CSS custom properties that
name a role (`--color-surface`, `--color-accent`, `--glow-focus`), never a hue.

- `src/app/theme/themes.css` defines the tokens of each theme in one rule,
  `:root[data-theme="<id>"], [data-theme-preview="<id>"]`. The light rule also
  matches `:root`, so Light applies before any theme is set.
- `src/app/theme/themes.ts` registers the themes (`THEMES`: id, label, description)
  and `applyTheme` sets `data-theme` on `<html>`.
- Every stylesheet of the application uses only `var(--…)` tokens: no stylesheet or
  component contains a color. Components are written once for every theme.
- `[data-theme-preview="<id>"]` applies a theme's tokens to a single element; the
  Appearance previews are drawn this way, from the real tokens of each theme.
- The application uses its own stylesheets (no CSS framework); buttons, inputs,
  selects, modals, tabs, navigation, badges, tables, cards, messages and scrollbars
  all take their colors from the tokens.

## Design tokens

| Group       | Tokens                                                                                                                                           |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Backgrounds | `--color-app-bg`, `--color-surface`, `--color-surface-elevated`, `--color-sidebar-bg`, `--color-card-bg`, `--color-input-bg`                     |
| Text        | `--color-text-primary`, `--color-text-secondary`, `--color-text-muted`, `--color-text-on-accent`                                                 |
| Accents     | `--color-accent` (primary), `--color-accent-hover`, `--color-accent-secondary` (links, secondary highlights)                                     |
| Borders     | `--color-border`, `--color-border-strong` (inputs and controls)                                                                                  |
| States      | `--color-hover`, `--color-active` (pressed), `--color-selected`, `--color-focus` (focus ring), `--color-disabled-bg`, `--color-disabled-text`    |
| Status      | `--color-success`, `--color-warning`, `--color-error` and their `-bg` variants                                                                   |
| Depth       | `--shadow-elevated`, `--color-overlay` (modal backdrop)                                                                                          |
| Glow        | `--glow-accent` (primary buttons, hover), `--glow-selected` (selected items), `--glow-focus` (keyboard focus), `--text-glow` (brand, page title) |
| Other       | `--scrollbar-color` (`auto` or `<thumb> <track>`), `--heading-letter-spacing` (page, panel and dialog titles)                                    |

Glow tokens are box shadows, combined with other shadows in lists, so a theme without
glow sets them to a transparent shadow (`0 0 0 transparent`, never `none`);
`--text-glow` is a text shadow and uses `none`.

## Built-in themes

- **Light**: bright neutral surfaces with a blue accent. The default theme.
- **Dark**: dark neutral surfaces with a light blue accent.
- **Neon 80s**: synthwave and arcade signage. Dark violet surfaces, fuchsia primary
  accent, cyan secondary accent and focus ring, electric violet selections and a
  moderate glow.

Light and Dark have no glow, `auto` scrollbars and normal title spacing, so they look
as they did before the token set was extended.

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
  letter spacing and the brand and page titles a soft text glow. Tables, filters and
  content are unchanged. No font is loaded from the network.
- **Media** keep their original colors: no filter, overlay tint or decorative
  background is applied to images.
- **Performance**: glow uses one plain `box-shadow` per element, shown on hover,
  focus or selection; there are no CSS filters, `backdrop-filter` or continuous
  animations, so large Media Grid pages cost the same as in the other themes.

## How to add a new theme

1. Add a rule to `src/app/theme/themes.css` with the selector
   `:root[data-theme="<id>"], [data-theme-preview="<id>"]`, its `color-scheme`, and a
   value for **every** token of the light rule.
2. Register the theme in `THEMES` (`src/app/theme/themes.ts`) with its id, label
   and description. **Settings → Appearance** lists it automatically.
3. Add theme-specific assets only if needed, referenced through tokens.

No page or component changes are needed. `themes.test.ts` checks that every
registered theme defines exactly the tokens of the light theme, can be previewed,
and that stylesheets contain no colors and use only defined tokens.

## Theme persistence

The theme is stored with the other interface preferences in the web view's
`localStorage` (`ymdb.preferences.v2`, field `theme`) by `src/app/preferences.ts`,
and is included in application data backups. `main.tsx` applies the saved theme
before the first render, so the default theme never flashes at startup. An unknown
or obsolete theme id (or any invalid value) falls back to **Light**, without
affecting the other preferences.
