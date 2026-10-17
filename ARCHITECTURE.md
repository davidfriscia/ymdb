# Architecture

This document describes the intended architecture of Your MAME DB (YMDB). Each
component is marked as **Implemented** or **Planned**. Planned components describe
direction, not commitments; details will be decided when each component is built.

## Overview

```text
┌──────────────────────────────────────────────────────────────┐
│ Desktop UI (React + TypeScript, WebView)                     │
└───────────────────────────────┬──────────────────────────────┘
                                │ Tauri IPC (commands / events)
┌───────────────────────────────▼──────────────────────────────┐
│ Application Core (Rust)                                      │
│                                                              │
│  ┌───────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ MAME          │  │ Catalog      │  │ ROM manager       │  │
│  │ integration   │  │              │  │                   │  │
│  └───────┬───────┘  └──────┬───────┘  └─────────┬─────────┘  │
│          │                 │                    │            │
│          │          ┌──────▼───────┐  ┌─────────▼─────────┐  │
│          │          │ SQLite       │  │ Remote providers  │  │
│          │          │              │  │ (SFTP, HTTPS next)│  │
│          │          └──────────────┘  └───────────────────┘  │
└──────────┼───────────────────────────────────────────────────┘
           │ process execution
┌──────────▼──────────┐
│ Local MAME installs │
│ (one or more)       │
└─────────────────────┘
```

Guiding principles:

- **Cross-platform first.** Windows, macOS and Linux are equally supported.
  Platform-specific behavior is isolated behind small, explicit boundaries.
- **Offline first.** Every feature that does not inherently need the network works
  without it. Remote repositories are strictly optional.
- **Thin frontend, rich core.** Business logic lives in Rust; the UI renders state
  and dispatches commands.
- **No copyrighted content.** YMDB never ships ROMs, BIOS files or CHDs.

## Components

### Desktop UI — Implemented (partially)

React + TypeScript application rendered by the system WebView through Tauri.

- `src/app/` contains the application shell and global styles.
- `src/features/<feature>/` groups each feature's components, IPC wrappers and tests.
  IPC calls are wrapped in typed functions (for example `mameApi.ts`) so components
  never call `invoke` directly. The TypeScript types in these wrappers mirror the
  serialized Rust types field by field.
- `src/shared/ipc.ts` provides `invokeCommand`, the single place where `invoke` is
  called, and maps command failures to the `CommandError` contract.
- `src/shared/useLoader.ts` runs an asynchronous load when its (memoized) input or
  a refresh key changes, ignores responses of superseded loads and keeps the last
  value for display while the next load runs.
- `src/shared/Modal.tsx` is the modal window (focus moved inside, trapped and
  returned to the opener; Escape and the backdrop close it; only the content
  scrolls). `src/shared/icons.tsx` holds the small inline icons, drawn with
  `currentColor` so they follow the theme; controls that show only an icon carry
  an `aria-label` and a `title`.
- Implemented features:
  - `mame-installation`: the MAME Installation panel (see below).
  - `catalog`: the MAME Catalog panel (catalog summary, import, progress, cancel).
    It notifies the shell when an import replaced the catalog.
  - `search`: typed wrappers of the search commands (`searchApi.ts`).
  - `browser`: the catalog browser and detail views (see
    [Catalog browser](#catalog-browser--implemented)).
  - `installations`: `InstallationsProvider`, the MAME page (`MamePage`: navigation by
    installation, `AddInstallationPanel`, `InstallationDetails` for Status).
  - `media`: **Settings → Media Sources** and `MediaProvider`, which shares the
    sources and their version with the Media Grid and `MediaGallery`, the image
    carousel, image categories and lightbox of the detail pages (see
    [Media sources](#media-sources--implemented)).
  - `launcher`: launch panels in the machine and software details, the **Launch
    Options** section of each installation on the MAME page and `LauncherProvider`,
    which shares the launch
    configuration and the session statuses (see [Launcher](#launcher--implemented)).
  - `remote`: **Settings → Remote Connections** and the **ROM Manager** section of
    each installation on the MAME page (see [Remote repositories](#remote-repositories--implemented-connections-sources-sftp-provider)).
  - `roms`: Prepare Download and Download & Play in the launch bar, with the plan in
    a modal window (see
    [ROM Manager downloads](#rom-manager-downloads--implemented)).
  - `support-files`: **Settings → Support Files** and the typed wrappers of the
    support file and classification commands (see
    [MAME support files](#mame-support-files--implemented)).
  - `saved-filters`: the **Saved Filters** section of View Options and the criteria
    format (see [Saved filter presets](#saved-filter-presets--implemented)).
  - `health-check`: calls `check_health` to verify frontend-backend communication.

#### Application shell

`App.tsx` shows a navigation rail with three pages: **Catalog** (the browser, the
start page), **MAME** and **Settings**. Pages stay mounted and are hidden with
the `hidden` attribute, so the browser keeps its query, results and scroll position
while the user visits another page. There is no router. Below 640 px the rail
becomes a top bar.

MAME and Settings are two-pane pages built with `src/shared/TwoPaneLayout.tsx`: the
page title and a secondary navigation (`TwoPaneNavItem`, `aria-current` plus a marker
and weight for the selected item; arrow keys, Home and End move between visible
buttons) on the left, and on the right a content panel with the selected item's
heading (and context, such as the installation) that replaces the previous content.
Both columns scroll independently; below 960 px the navigation narrows and below
720 px it collapses behind a toggle above the content. Sections are mounted when
first shown and then kept mounted, hidden (`src/shared/useVisited.ts`), so that
unsaved forms, the list of detected installations and running import polls survive
a change of selection; save semantics are unchanged. The selection is component
state (no persistence): the pages stay mounted, so it survives visits to other pages.

- **MAME** (`MamePage`): **Add Installation**, then one collapsible group per
  installation, keyed by its id (label, version, catalog, **Active** marker), with
  **Status**, **Catalog**, **ROM Manager** and **Launch Options**. The page opens on
  the Status of the active installation; a selection naming a removed installation
  falls back to it (or to Add Installation when none is left), and an added
  installation (or the one already registering a chosen executable) is shown at once.
  Choosing a section never changes the active installation.
- **Settings** (`SettingsView`): **Appearance** (theme), **Media Sources**, **Remote
  Connections**, **Favorites**, **Support Files**, **About** (name, version) and
  **Diagnostics** (backend connection check). It opens on Appearance; the selected
  section is held by the shell, so the catalog can open **Support Files** directly.

The shell also counts changes of the support files; the browser receives the sum of
the catalog and support file counters as its refresh key, so a change in Settings
reloads the classifications, results and facets of the catalog.

The shell keeps two counters, incremented when the MAME configuration changes and
when an import replaces the catalog; the browser reloads its catalog status,
results and open detail when their sum changes.

#### Themes

- Colors are semantic CSS custom properties (`--color-app-bg`, `--color-surface`,
  `--color-surface-elevated`, `--color-sidebar-bg`, `--color-card-bg`,
  `--color-input-bg`, `--color-text-primary`/`-secondary`/`-muted`/`-on-accent`,
  `--color-border`/`-strong`, `--color-accent`/`-hover`, `--color-hover`,
  `--color-selected`, `--color-focus`, `--color-disabled-bg`/`-text`,
  `--color-success`/`-warning`/`-error` and their `-bg` variants,
  `--color-overlay` (backdrop of modal views such as the lightbox),
  `--shadow-elevated`). Stylesheets use only these tokens.
- `src/app/theme/themes.css` defines the complete token set of each theme under
  `:root[data-theme="<id>"]`; `themes.ts` registers the themes (`light`, `dark`) and
  `applyTheme` sets `data-theme` on `<html>`. Light is the default and also applies
  before any `data-theme` is set. The operating system preference is not followed.
- Adding a theme means defining its tokens in `themes.css` and registering it in
  `THEMES`; **Settings → Appearance** lists registered themes automatically.
- `main.tsx` applies the saved theme before the first render to avoid a flash.

#### Interface preferences

`src/app/preferences.ts` stores the interface preferences as JSON in the webview's
`localStorage` under the versioned key `ymdb.preferences.v2`: theme, visibility of
the Filters and View Options panels, selected renderer, the options of every
renderer (kept separately, so switching renderer keeps each one's sizes and
settings), **Favorites only** (`favoritesOnly`, the only preference that restricts
the search; the favorites themselves are in `lists.json`) and the command line
preview. They are separate from `installations.json`
(MAME installations and the active one), the launch configurations, `media.json`
(media sources) and the catalogs, and are the same for every MAME installation.
Every field is validated when read: unknown themes or renderers, wrong types and
malformed JSON fall back to the default of that field, and renderer options are
normalized against the renderer definitions (ranges clamped to their bounds and
steps, unknown choices replaced). Version 1 data (`ymdb.preferences.v1`: one card
size and the `cards` renderer) is migrated when there is no version 2 yet, and
removed at the next save. Unavailable storage only means the preferences last for
the session.

Planned: user lists beyond Favorites (see [User lists](#user-lists-favorites--implemented)),
input devices.

### Catalog browser — Implemented

`src/features/browser/` browses machines or software of the active MAME installation
with the [search engine](#catalog-search--implemented). The frontend never searches,
filters, sorts, counts or paginates itself: it only builds typed requests, each naming
the installation (see [Version selection](#version-selection-in-the-interface) for
what happens when the active installation changes).

- **State.** `browserState.ts` holds the query in a reducer: catalog type, search
  box text and applied text, per catalog type the filters, sort, direction, page,
  page size and facet options (value search, limit), and the stack of open detail
  views. Every query change returns to the first page. Machine and software queries
  are kept separately, the text is shared.
- **Requests.** `buildResultsRequest` and `buildFacetRequest` translate the state to
  `MachineSearchRequest`/`SoftwareSearchRequest`. Results and facets are two
  requests: facets depend on the text and filters only, so changing page, sort or
  page size does not recount them (on 0.280 all machine facets take about 60 ms and
  software facets about 150 ms, a result page under 5 ms). Requests are memoized,
  so rendering never searches again; `useLoader` ignores out-of-order responses.
- **Text.** The search text is applied 300 ms after the last keystroke, or
  immediately with Enter or the Search button.
- **Filters.** `filterDefinitions.ts` lists the facet filters (machines:
  manufacturer, year, driver status, emulation status, software list; software:
  list, publisher, year, support status, part interface) and the single-choice
  filters (machines: parent/clone, BIOS, device, mechanical, runnable, declares
  ROMs, declares CHDs; software: parent/clone, declares ROMs, declares disks).
  Facet values are checkboxes with counts (disjunctive semantics from the
  backend), a value search on large facets (debounced), "Show more" up to 200
  values and, for long lists, "Browse all". Every value list scrolls on its own
  (`.facet__values--scroll`, bounded height) instead of growing the panel; the
  legend shows the number of selected values and "Clear". Selected values missing
  from the returned values stay listed, with "—" as count. The shared pieces live in
  `filters/`: `FacetValueList` (checkbox rows), `CategoryTree`, `FacetBrowserModal`
  and the pure helpers of `facetValues.ts`.
- **Classification filters** (machines only): Genre, Category, Series and Version
  Added are facet definitions with a `classification` dimension; they are rendered
  and requested only for the dimensions the catalog has
  (`get_classification_status`), otherwise a note links to Settings → Support Files
  and a projection that could not be updated is reported with Retry. Genre and Series
  are lists searched by the backend like manufacturers. Category is a tree
  (`presentation: "tree"`) built from the node paths of the facet (levels joined with
  `/`, any depth), searched locally; selecting a node includes its descendants,
  shown checked and disabled. Version Added (`presentation: "version"`) has From/To
  selects in the MAME version order returned by the backend (`ranges` in the filters,
  sent as `versionAddedRange`) and exact versions, newest first. Tree and version
  facets are requested with every value (1,000 at most).
- **Browse all** (`FacetBrowserModal`): the values of one facet for the current text
  and other filters (one facet request, up to 1,000 values, searched by the backend
  for lists and locally for trees), sorted alphabetically, with a search, the number
  of selected values and "Clear selection". The selection is a draft: "Apply"
  replaces the facet's values with `setFacetValues` (one state change), "Cancel",
  Escape, the backdrop and the close button discard it. Searching never removes
  selected values. Active filters are shown as removable chips with "Clear all filters";
  the Filters button shows their number. Declared ROMs/CHDs are what MAME lists,
  not files on disk.
- **Layout.** Three areas: the Filters panel (left), the results (center) and the
  View Options panel (right). The **Filters** and **View** toolbar buttons
  (`aria-pressed`) show or hide each panel independently; the body grid only has
  columns for visible panels, so the results take the freed width. Hiding a panel
  keeps filters, search, page and scroll, and never searches again. Below 1100 px
  both panels narrow; below 800 px they stack above the results.
- **Scrolling.** The page itself never scrolls: `.browser` fills the shell's main
  area, the toolbar keeps its height and the body grid takes the rest
  (`min-height: 0` on every flex/grid level). Each side panel scrolls on its own.
  The results column (`.browser__results`) is a flex column with the only
  scrolling element of the results (`.browser__scroll`: active filter chips,
  summary, renderer output) above a footer (`.browser__footer`) holding the
  pagination, which never scrolls; no fixed positioning or window-based heights are
  used. A new request (search, filter, sort, page or page size) sets the scroll of
  `.browser__scroll` back to the top before the new results arrive, without touching
  the filter or view panels.
- **Renderers.** Results are displayed by a renderer implementing
  `ResultRendererProps` (`renderers/types.ts`): the items of the current page (the
  same search DTOs for every renderer), the renderer's options and `onOpen(index)`.
  Renderers only display; the choice of renderer never changes the text, filters,
  facets, sort or page. `renderers/registry.ts` is a typed list of definitions: a
  stable id, a label, an icon (SVG path), the component and the supported options
  (`range`, `toggle` or `choice`, each with its default). The View Options panel
  builds the view mode selector and the option controls from these definitions, and
  the preferences normalize stored options against them (`renderers/options.ts`),
  so a future table or list renderer only needs its component and its definition.
- **Classic Cards** (`ClassicCardRenderer`, the original renderer) uses a CSS grid
  with `repeat(auto-fill, minmax(min(var(--card-size), 100%), 1fr))`: the Card size
  option (160-420 px) sets the minimum card width and the number of columns follows
  the available width, without horizontal scrolling. Machine cards show
  description, short name, manufacturer, year, driver status and
  BIOS/device/mechanical/clone/not runnable badges; software cards show description,
  list/name, publisher, year, list description and support status.
- **Media Grid** (`MediaGridRenderer`) shows the artwork of each result in a fixed
  4:3 frame, the same for every tile so the grid stays regular; the image is
  contained (whole image, default) or covers the frame (cropped), never stretched,
  so horizontal and vertical arcade screens, consoles, computers and software
  artwork keep their proportions. Below the image, in smaller text, are the title
  (two lines at most), optionally the short name and the status badge. Options:
  tile size (120-480 px, columns follow the width), artwork category, image fit,
  show title, show technical name, show status badges. Tiles are buttons (keyboard
  focus and Enter open the detail, visible focus ring); the full title, short name,
  manufacturer and year are in the accessible name and the tooltip, and
  manufacturer and year appear over the image on hover or focus. Images use
  `loading="lazy"`; an image that is missing or fails to load shows a themed
  placeholder ("No snapshots"), only for that tile. When the category has no
  directory, no image is requested and a note points to Settings.
- Pages hold at most 200 items, so no virtualization is used.
- **Saved filters.** The View Options panel holds **Saved Filters** (see
  [Saved filter presets](#saved-filter-presets--implemented)). Loading one dispatches
  `applyCriteria`, which sets the catalog type, text, filters and first pages in one
  reducer step (and Favorites only in the same event), so the controls, results,
  counts and pages change together. Classification values that no machine of the
  catalog has are checked with `find_missing_classification_values` and reported in a
  banner above the results; they stay in the request and match nothing.
- **Favorites.** Both renderers place `ResultBookmark` (`FavoriteBookmark`) in the
  top right corner of each item, as a sibling of the button that opens it (never
  nested), so a click on it never opens the detail. **Favorites only** is the
  `favoritesOnly` interface preference: it adds `list: "favorites"` to the results
  and facet requests, returns both catalog types to their first page and is shown
  as a chip above the results. See [User lists](#user-lists-favorites--implemented).
- **Pagination.** Page numbers around the current page, previous/next, "Page N of
  M", page size (25, 50, 100, 200) and "Go to". The workspace renders one
  `Pagination` in the results footer for every renderer, whenever the current
  results have at least one match (also for a single page, so the page size stays
  reachable); it is hidden without results, on search errors and when software
  lists are not imported. Its controls wrap to several rows on narrow windows.
- **Details.** Opening a result pushes a detail entry on the stack; links inside a
  detail push further entries. "Back" pops one entry, "Back to results" clears the
  stack. Results stay mounted (hidden) so the query and results are unchanged; the
  results scroll position (of `.browser__scroll`) is saved when a detail opens and
  restored when the results are shown again; the results stayed rendered meanwhile,
  so the position applies to the same content. The query (text, filters, sort, page,
  page size) lives in the browser state and the view (renderer and its options) in
  the interface preferences, so neither changes. Previous/Next move through the
  items of the current result page.
- **Detail layout** (`DetailFrame`, the same for machines and software), from top
  to bottom:
  1. **Header**: one compact row with Back to results (and Back after following a
     link) on the left and Previous / position / Next on the right; no title.
     `CatalogBrowser` provides the navigation controls (`DetailNavigation`); the
     header is shown while a detail loads too.
  2. **Media**: the `MediaGallery` of the record, a carousel on the left with the
     image categories in a grid of two columns on its right (see
     [Detail gallery](#detail-gallery)).
  3. **Information and launch bar** (`LaunchBar`): the title (the page's only
     `h2`, two lines at most), the short information (short name,
     manufacturer/publisher and year, status and relation badges) and the launch
     actions (see [Interface](#interface) of the launcher). Without launch
     (software lists), the information alone.
  4. **Launch status** (`LaunchStatus`), only once there is something to report.
  5. **Advanced** launch options, collapsed by default.
  6. **Technical sections**: a side tab list (top tabs below 800 px) and the
     selected section, keeping the selected tab when moving to the previous or next
     result.

  The page scrolls as a whole; the gallery has a bounded height so that the bar and
  the sections below stay perceptible. Software list details have no media and no
  launch.
  - Machine: Overview (general data, parent, ROM parent, sample set, clones,
    driver and emulation features, samples), ROMs (BIOS sets, ROMs with hashes,
    regions, merge names, status), CHDs, Devices (device references linked to
    their machine, media devices, RAM options), Display (displays, sound),
    Controls (input, controls), Chipset, Software lists (declared lists with
    status and filter, linked to the list detail, marked when not imported).
  - Software: Overview (list, publisher, year, support, notes, parent, clones,
    information and shared features), Parts (features, data areas with ROMs and
    load instructions, disk areas), Machines (machines declaring the list, with a
    note that this is not a compatibility guarantee).
  - Software list: description, notes, software count, machines declaring it as
    original or compatible, the software of the list and its machines. Lists
    referenced by machines but missing from the imported software lists open with
    an explanation.
  - Clones, software of a list and machines of a list are paginated lists (20 per
    page) served by the search engine (`cloneOf`, `lists`, `softwareLists`).
  - Unresolved references (a parent or device missing from the catalog) are shown
    as plain text.

- **States.** Loading, no MAME installation, no catalog for the active installation
  (with a link to the MAME page and the version menu), catalog unreadable, catalog
  not matching the installation's executable (warning banner, the
  catalog stays browsable), software lists not imported, no results (with "Clear
  all filters"), search or detail errors with Retry, a page past the end, and a
  record that is no longer in the catalog after a reimport.

Measured on the 0.280 catalog (48,893 machines, 139,122 software, release build,
Windows): first results 0.65 s after the page starts loading; a typed search shows
results about 340 ms after the last keystroke (300 ms of debounce); selecting a
filter updates results in 12-18 ms; page changes 45-140 ms in the UI; machine and
software details open in 13-40 ms (1-3 ms in the backend); "Show more" on
manufacturers about 0.1 s; JavaScript heap about 4 MB after a full session.

### Application Core — Implemented (partially)

The Rust crate in `src-tauri/`.

- `lib.rs` builds the Tauri application, registers plugins, managed state and commands.
- `commands/` contains Tauri command adapters (`health.rs`, `mame.rs`,
  `installations.rs`, `catalog.rs`, `search.rs`, `detail.rs`, `launcher.rs`,
  `media.rs`, `remote.rs`, `roms.rs`, `lists.rs`, `support_files.rs`, `saved_filters.rs`).
  Commands extract what they need from the Tauri runtime and delegate
  to plain functions that are unit-tested without Tauri. Blocking work (process
  execution, native dialogs, searches) runs on the blocking thread pool, and
  long-running work (catalog import, ROM downloads) on a dedicated thread, so the UI never freezes.
  Failures are returned as a structured `CommandError` (`settings`,
  `installationNotFound`, `duplicateInstallation`, `removalNotAllowed`,
  `migrationFailed`, `importInProgress`, `downloadInProgress`, `invalidRequest`, `catalogNotImported`,
  `catalogUnreadable`, `softwareListsNotImported`, `catalogMismatch`, `notFound`,
  `alreadyRunning`, `launchFailed`, `connectionNotFound`, `connectionInUse`, `remote`,
  `internal`). A problem with one installation
  (missing executable, unreadable catalog) is reported for that installation only.
- `health.rs` builds the health report returned by `check_health`.
- `atomic_file.rs` writes small files atomically (`installations.json`, `launch.json`,
  `media.json`, `remote-connections.json`, `remote-sources.json`).
- `installations/` manages the registered MAME installations, the active one and the
  migration from the single-installation layout (see
  [MAME installations](#mame-installations--implemented)).
- `settings.rs` reads the `settings.json` of the single-installation layout, only for
  the migration.
- `mame/` implements the MAME integration (see below).
- `catalog/` implements the machine and software list catalog (see [Catalog](#catalog--implemented-machines-and-software-lists)).
- `launcher/` builds and runs MAME command lines (see [Launcher](#launcher--implemented)).
- `media/` finds and serves local artwork (see [Media sources](#media-sources--implemented)).
- `remote/` implements remote connections, remote sources and repository providers
  (see [Remote repositories](#remote-repositories--implemented-connections-sources-sftp-provider)).
- `roms/` plans and performs ROM Manager downloads (see
  [ROM Manager downloads](#rom-manager-downloads--implemented)).
- `support_files/` parses MAME support files and keeps the global repository of
  classifications (see [MAME support files](#mame-support-files--implemented)).
- `saved_filters.rs` stores the saved filter presets (see
  [Saved filter presets](#saved-filter-presets--implemented)).

### MAME integration — Implemented

Implemented in `src-tauri/src/mame/`, as independent modules reused by the
catalog importer and the launcher:

- `process.rs`: runs a program directly (never through a shell) with closed stdin.
  `run_with_timeout` captures size-bounded output and kills the process after a
  timeout (used for `-version`). `spawn_streaming` hands the standard output to the
  caller for incremental reading, captures standard error (bounded), and runs a
  watchdog that kills the process when a cancellation flag is set or when no output
  is produced for an inactivity timeout (used for `-listxml`). Dropping a streaming
  process always terminates and reaps it.
  `spawn_detached` starts a game session without a timeout, with its output written
  to a log file instead of pipes; the launcher owns and monitors the child.
- `version.rs`: recognizes the output of `mame -version`.
- `validation.rs`: validates an executable and produces a `ValidationReport`.
- `detection.rs`: finds candidate executables in well-known locations.
- `ini.rs`: reads `rompath` and `samplepath` from `mame.ini` (never written, MAME not
  run), for the ROM Manager destinations and search paths.
- `platform.rs`: all OS-specific details (executable names, search directories, MAME's
  default `inipath`,
  executable checks, spawn error codes, console window suppression on Windows).

**Validation** of a path proceeds in order and stops at the first failure:

1. The path must be absolute and exist (`notFound`).
2. It must be a regular file (`notAFile`).
3. It must be directly executable: an `.exe` file on Windows (so `.bat`/`.cmd` files,
   which require `cmd.exe`, are rejected), a file with an execute permission bit on
   Unix (`notExecutable`).
4. It is run as `<path> -version` from its own directory, with a 20-second timeout
   (`launchFailed`, `timedOut`). A process that times out is killed and reaped.
5. The exit code must be 0 (`invalidExitCode`).
6. Standard output must be exactly one line in the format MAME prints,
   `<major>.<minor> (<revision>)`, for example `0.280 (mame0280)` (`unrecognizedOutput`).
   This format comes from `emulator_info::get_build_version()` in the MAME sources
   and was verified against real MAME 0.240 and 0.280 builds. MAME versions that do not
   support `-version` (for example 0.162) are reported as `invalidExitCode`.

The `ValidationReport` contains the executable path, status, parsed version
(`release` and `revision`), raw stdout/stderr/exit code and the check timestamp.

**Detection** checks, without recursion, for files named `mame` (Unix) or
`mame.exe`/`mame64.exe` (Windows) in: the configured path, every absolute directory in
`PATH`, and conventional directories (for example `C:\mame`, `%ProgramFiles%\MAME`,
Chocolatey and Scoop locations on Windows; `/opt/homebrew/bin` and `/usr/local/bin`
on macOS; `/usr/games`, `/usr/bin`, `/snap/bin` on Linux). Candidates are deduplicated
by canonical path, capped at 16, and validated concurrently. Detection only reports
results; it never changes the saved configuration.

**Execution policy:** MAME is only executed in response to an explicit user action
(select, verify, detect, import catalog, import software lists, Play). Management
commands use fixed arguments (`-version`, `-listxml`, `-listsoftware`); launches use
command lines built by the launcher from catalog names and validated options. Loading
the configuration or the catalog at startup never runs MAME. The frontend cannot
pass arbitrary arguments.

Planned:

- Verify ROM sets, BIOS and CHD requirements.

### MAME installations — Implemented

Implemented in `src-tauri/src/installations/` (model, persistence, migration) and
`commands/installations.rs`, typed in `src/features/installations/`. YMDB manages any
number of MAME installations side by side; one of them is active.

#### Model and identifiers

A `MameInstallation` (`installations/registry.rs`) has:

- `id`: 32 lowercase hexadecimal digits (a random UUID), stable for the life of the
  installation. It is never derived from the version, so two installations may have
  the same version with different executables. Ids are only used as path segments
  after being found in the registry.
- `executablePath`: absolute path of the executable. It never changes: another
  executable is another installation, so a catalog can never be silently associated
  with a build it was not imported from. Duplicates are refused by
  `mame::platform::same_executable`: the canonical path when the file exists, and
  case-insensitive comparison on Windows and macOS.
- `displayName`: optional name chosen by the user (at most 60 characters); the label
  is otherwise `MAME <release>`.
- `version`, `identity` (file size and modification time) and `verifiedAt`: recorded
  by the last successful verification. The executable status (`verified`,
  `notVerified`, `changed`, `missing`) is computed from the identity without running
  MAME.
- `addedAt`.

`installations.json` (configuration directory, `schemaVersion` 1, atomic writes,
validated on load and save) holds the installations in registration order, the
`activeInstallationId` and the record of the legacy migration:

```json
{
  "schemaVersion": 1,
  "installations": [
    {
      "id": "3f2a…",
      "executablePath": "C:\\Emulators\\MAME 0.280\\mame.exe",
      "displayName": null,
      "version": { "release": "0.280", "revision": "mame0280" },
      "identity": { "size": 123456789, "modifiedAt": 1760000000000 },
      "addedAt": 1760000000000,
      "verifiedAt": 1760000000000
    }
  ],
  "activeInstallationId": "3f2a…"
}
```

An invalid file is reported and never replaced implicitly, because it references
the files of every installation. Rules: the first installation added becomes active;
adding one never changes the others; when the active installation is removed, the
first remaining one in registration order becomes active (none when the list is
empty, which shows the empty state with a link to add one). Selecting an
installation never starts MAME.

#### Files of an installation

| File                                        | Directory                         |
| ------------------------------------------- | --------------------------------- |
| `installations/<id>/catalog.sqlite`         | local data (`app_local_data_dir`) |
| `installations/<id>/catalog.sqlite.partial` | local data, during an import      |
| `installations/<id>/launch.json`            | configuration (`app_config_dir`)  |
| `installations/<id>/remote-sources.json`    | configuration (`app_config_dir`)  |

Each catalog is a separate SQLite database built by the unchanged parser, migrations,
`CatalogStore` and search engine; there is no database mixing versions. It keeps the
version and executable identity of the build it was imported from
(`catalog_metadata`). `media.json` and the interface preferences are global.

#### Resolving requests

Every command that reads a catalog or a launch configuration receives an explicit
`installationId` and resolves it once (`InstallationsState::resolve`) into the
installation, its `CatalogStore` and its `LaunchConfigStore`. A request therefore
always reads the catalog it started with, even if the active installation changes
meanwhile; nothing reads a mutable "current installation" in the middle of a
request. Unknown ids fail with `installationNotFound`.

One `CatalogStore` per installation used since startup is kept, each with its own
lock and cached read connection, so the atomic replacement on Windows works as
before. Only the catalogs of the requested and of the active installation keep a
read connection open; the others are closed (`release_reader`) whenever another
catalog is requested or the active installation changes, so browsing many
installations does not keep every database open.

#### Imports

`start_catalog_import(installationId)` and `start_software_list_import(installationId)`
run on a dedicated thread with the store and executable of that installation, and
the import status names it (`installationId`). Changing the active installation does
not affect a running import. Only one import runs at a time (any installation):
it bounds MAME's memory and disk use and keeps the existing job model. Streaming,
partial file, atomic commit, cancellation, integrity checks, migrations and the
machine/software list consistency check are unchanged.

#### Version selection in the interface

`InstallationsProvider` loads the list and exposes the active installation. Selecting
another one (**Set as active** in the Status of an installation on the MAME page, or the
**MAME Version** menu of the catalog's View Options)
switches the interface immediately and saves the selection with
`set_active_installation`; selections are saved one after the other so the last one
wins. The catalog browser:

- names the installation in every search, facet, detail, related list and launch
  request, and tags responses with it: `useLoader` ignores superseded responses and
  results of another installation are never shown, not even while the new catalog is
  being searched;
- keeps the search text, filters and sort order, returns to the first page and closes
  the detail views (they belong to the previous catalog; the rule is applied while
  rendering, so no detail is requested from the new catalog by mistake);
- keeps filter values that do not exist in the new catalog (they match nothing and
  are listed without a count) and shows a notice offering to clear them;
- keeps showing the workspace while the new catalog status loads when that
  installation has a catalog, and shows the empty state (with the version menu)
  otherwise;
- keeps the renderer and its options, which are global interface preferences.

#### Launch configuration per installation

`launch.json` is per installation, with the same format and validation. Global
options, profiles, machine overrides and software overrides are therefore isolated:
option, device and machine names can change between versions, so they are never
shared or copied automatically. The precedence is unchanged: MAME configuration,
global options of the installation, profile, machine override, software override.
The **Launch Options** section of each installation on the MAME page configures that
installation only.

`LaunchRequest` contains `installationId`; preview and launch resolve that
installation, check its catalog against its executable and build a `LaunchPlan`
that records `installationId`, `profileId` (the applied profile), the executable,
the working directory and the arguments with their roles.

#### Sessions

Each session records `installationId` and `installationLabel` (the name when it
started), shown by the session status. Switching the active installation never stops
a session. The same machine or software may run once per installation at a time
(two installations can run it concurrently). Removing an installation is refused
(`removalNotAllowed`) while a session started from it or an import of it is running;
nothing is terminated implicitly.

#### Removal

`remove_installation(installationId, deleteCatalog, deleteLaunchConfig)` unregisters
the installation after a confirmation in the interface. The catalog is deleted only
when requested (default in the interface: yes, it is derived data) and the launch
configuration and the remote sources (ROM Manager settings) only when requested
(default: no); kept directories are reported. Global remote connections are never
removed with an installation. Files
are deleted before the registry changes: if one cannot be deleted, the installation
stays registered. The MAME executable and MAME's files (ROMs, CHDs, artwork, INI,
cfg, nvram) are never touched.

#### Migration from the single-installation layout

Sessions 1-9 stored one executable in `settings.json`, one `catalog.sqlite` in the
local data directory and one `launch.json`. When `installations.json` does not exist,
`installations::migration` upgrades this layout at startup without running MAME:

1. **Plan.** Read `settings.json` and the old catalog's metadata. The configured
   executable becomes an installation; when the catalog was imported from it (same
   executable by the platform rules), the catalog's release, revision and executable
   identity become the installation's version and identity, so it starts verified.
   A catalog imported from another executable gets its own installation for that
   executable (never associated with the configured one). The launch configuration
   goes to the configured installation. The registry is saved with a `pending`
   migration record naming these installations and the catalog's expected release
   and machine count.
2. **Move** `catalog.sqlite` into `installations/<id>/` with a rename on the same
   volume (no copy, no re-import) and delete the old partial file of an interrupted
   import, if any. **Copy** `launch.json` atomically.
3. **Verify** the moved catalog (readable, same release and machine count) and the
   copied launch configuration (same bytes).
4. Mark the record `completed`.

Every step can run again: an interrupted migration resumes from the saved record with
the same installation ids, and a completed one never runs again. A failure (for
example another program keeping the old catalog open) leaves the record `pending`,
moves nothing else and is reported as `migrationFailed`; every access retries, so the
migration completes once the cause is gone. `settings.json` and `launch.json` are
never modified and remain as a backup. Without any old configuration or catalog, no
file is written. Media sources and interface preferences need no migration.

#### Commands

- `list_installations`, `set_active_installation(installationId)`,
  `rename_installation(installationId, displayName)`.
- `add_installation(path)`: validates with `-version`, then registers; refuses
  duplicates (`duplicateInstallation`) before running MAME.
- `verify_installation(installationId)`: validates again and records the version.
- `remove_installation(installationId, deleteCatalog, deleteLaunchConfig)`.
- `pick_mame_executable`, `detect_mame_installations` (each candidate reports the
  installation that already registers it).

#### Limitations

- One import at a time across installations.
- Profiles and overrides cannot be copied between installations yet.
- The executable of an installation cannot be changed; add another installation.
- After a downgrade to an older YMDB, the moved catalog is not found and must be
  imported again (the old `settings.json` and `launch.json` are still there).
- Paths are compared case-insensitively on macOS, which assumes the default
  case-insensitive file system.

### Launcher — Implemented

Implemented in `src-tauri/src/launcher/`, exposed by `commands/launcher.rs`, typed in
`src/features/launcher/launcherApi.ts`. It is specific to MAME: it only builds MAME
command lines from catalog records and validated options; the frontend never passes
raw arguments.

#### MAME command syntax (verified with MAME 0.280)

- Usage is `mame [machine] [media] [software] [options]`. YMDB always passes the
  machine short name first, then one media argument per software part, then options.
- A software part is mounted with `-<media brief name> <list>:<software>:<part>`,
  for example `nes -cart nes:smb:cart` or
  `apple2e -flop1 apple2_flop_orig:ultima4:flop1 -flop2 apple2_flop_orig:ultima4:flop2`.
  The fully qualified form is unambiguous, while the bare positional form (`nes smb`)
  lets MAME pick the list and the device and loads a single part. An unknown part or
  list is reported with exit code 4.
- MAME only looks for software in the lists declared by the machine (`nes kof98`
  fails with exit code 6), and only through media devices that exist in the
  machine's default configuration (`nes -flop1 smb`: unknown option).
- Boolean options are enabled with `-name` and disabled with `-noname`; they take no
  value (`-window 1` fails). Value options take exactly one following argument.
  Repeating an option is not an error: the last occurrence wins, so MAME core
  options are not repeatable. Search paths are a single argument with
  `;`-separated directories. Paths with spaces and non-ASCII characters work when
  passed as separate arguments.
- Invalid option values are not fatal: MAME prints a warning and reverts to its
  default (`-volume -100`, `-speed abc`, an unknown `-bios`). YMDB validates values
  before launching so this does not happen silently.
- Exit codes (MAME `emu.h`, observed): 0 success, 1 failed validity checks, 2
  missing files, 3 fatal error, 4 device or image error, 5 unknown system, 6 invalid
  configuration (unknown option or software), 7-9 ROM identification results.
- MAME resolves its configuration relative to the working directory: `mame.ini` is
  searched in `inipath` (default `.;ini;ini/presets`) and the default `rompath` is
  `roms`. Started from another directory, `mame neogeo` cannot find the ROMs it
  finds from its own directory. YMDB therefore always starts MAME with the
  executable's directory as working directory, as when the user runs it there, and
  never writes `mame.ini` or any other MAME file.

#### Modules

- `options.rs`: the option model. Each supported option has a definition (name,
  kind, category, label, description). Kinds: `boolean`, `integer` (range), `float`
  (range), `identifier` (a word such as a video backend or BIOS name), `path` and
  `pathList` (joined with `;`). Values are validated against the definition: no
  control characters, no value starting with `-` (MAME would read it as an
  option), ranges for numbers, no `;` inside one path. Unknown option names are
  rejected. An `OptionSet` maps option names to values and only contains options the
  user set.
- `config.rs`: the launch configuration (global options, profiles, default profile,
  per-machine and per-software overrides), its validation and the option
  resolution by precedence.
- `store.rs`: `launch.json` persistence.
- `resolve.rs`: catalog lookups for a launch: the machine must exist and be
  runnable; for software, the candidate machines declaring its list, their
  compatibility and the media devices able to mount each part.
- `search_paths.rs`: extends `rompath`/`samplepath` with the ROM Manager destinations
  (see below).
- `command.rs`: `LaunchPlan`, the structured command line (executable, working
  directory, arguments with their role and, for options, the level they come from).
  The arguments of the plan are what is executed; the preview shown to the user is
  the same plan serialized, so there is a single composition.
- `session.rs`: started MAME processes, their status and termination.

#### Option precedence

From lowest to highest:

1. MAME native configuration (`mame.ini` and other INI files, MAME defaults).
2. YMDB global options.
3. The launch profile selected for the launch (the default profile otherwise).
4. Overrides of the machine.
5. Overrides of the software (software launches only).

The ROM Manager may then extend `rompath`/`samplepath` (see
[Search paths of the ROM Manager](#search-paths-of-the-rom-manager)).

Only options explicitly set at some level are passed to MAME; everything else stays
under MAME's own configuration. Each option is passed once, with the value of the
highest level that sets it, and the preview names that level. A profile is a flat
set of options with an id and a name; profiles do not inherit from each other. The
`default` profile always exists and cannot be deleted; another profile can be made
the default.

The option set is deliberately small (display, performance, sound, paths, machine
configuration such as BIOS and RAM size, `seconds_to_run`) and grows by adding
definitions. Input options (`joystick`, `mouse`, `ctrlr`, controller mappings) are
intentionally not included: a future **Input Devices** feature will manage
controllers and mappings per profile, machine or software and contribute options
through the same `OptionSet` and precedence mechanism. Profiles are not tied to any
controller type, so this needs no change to them.

#### Search paths of the ROM Manager

`-rompath` and `-samplepath` replace MAME's value instead of adding to it. When the
installation's enabled remote sources store files in local destinations
(`installations/<id>/remote-sources.json`), `launcher/search_paths.rs` makes sure
MAME searches them, inside the same `prepare` used by preview and launch:

1. Take the value resolved by the precedence above (global options, profile, machine
   or software override), or, when no level sets the option, MAME's own value from
   `mame.ini` (`mame/ini.rs`, read only) or MAME's default (`roms`, `samples`).
2. Append the destinations it does not already contain (relative entries are
   compared from the MAME directory, with the platform's path rules).
3. Pass the option only if something was appended; its origin is then `romManager`
   in the plan and the preview. Otherwise nothing changes and MAME's configuration
   stays in charge.

ROM and CHD destinations extend `rompath`; sample destinations extend `samplepath`.
An unreadable `remote-sources.json` does not prevent launching: the launch gets a
warning and the search paths are not extended.

#### Software launches and compatibility

For a software, YMDB lists the machines declaring its list, from the catalog:

- the `original` or `compatible` relation, as declared by MAME;
- the list filter, evaluated as MAME does (`software_list_device::is_compatible`):
  the software is compatible when the machine's list has no filter, when the
  software declares no `compatibility` value (part feature, else shared feature), or
  when one of the filter values appears in the compatibility values. Otherwise MAME
  warns that the software might not work, and YMDB shows the mismatch;
- the media devices of the machine (from `-listxml`) whose interfaces accept the
  software's parts. A machine without a device for any part cannot load the
  software and cannot be selected.

YMDB never picks a machine by an arbitrary order. A machine is preselected only when
it is the only one that is `original`, matches the filter and can mount the
software. Everything else requires an explicit choice, and the interface states that
the catalog cannot guarantee that the software works on the chosen machine (slot
cards, RAM size and `requirement` features of parts are not evaluated).

Parts are assigned to the compatible media devices of the chosen machine: by default
the parts in order, one per device; the user can change the assignment or leave
devices empty. At least one part must be mounted, and a part is mounted once.

#### Process management

- MAME is started directly (no shell) with an argument array, stdin closed and the
  executable directory as working directory. On Windows the console window is
  suppressed; MAME opens its own window.
- Standard output and error go to a per-session log file in the application local
  data directory (`launch-logs/`, the 20 most recent kept) instead of pipes, so a
  long session never blocks on a full pipe and keeps running if YMDB exits. The end
  of the log is shown when MAME fails.
- No timeout is applied to game sessions; a monitor thread checks the process every
  250 ms and records the exit code. Statuses are kept in memory and polled by the
  frontend while a session runs (no per-session events).
- A second launch of the same machine and software while a session for it runs is
  refused (`alreadyRunning`); different targets may run at the same time.
- The user can stop a running session after a confirmation. This kills the process
  immediately, so MAME does not save NVRAM or its configuration for that session.
- Closing YMDB never terminates running sessions: MAME keeps running on its own and
  YMDB forgets it. Nothing is killed implicitly.
- Before launching, the executable of the request's installation must exist and its
  catalog must be `current` for it: a catalog from another or a modified executable is refused
  (`catalogMismatch`), because machine, device and software names may differ. YMDB
  does not check whether ROM files are present: MAME reports missing files (exit
  code 2) and the session shows that error.

#### Persistence

Each MAME installation has its own `launch.json`, in `installations/<id>/` of the
application configuration directory. It has its own `schemaVersion`, is written
atomically and is validated when loaded: a missing file gives the
default configuration; an invalid file is reported and replaced only when the user
saves. It is separate from the catalog (derived data) and from the interface
preferences (web storage).

Format (options are only those the user set; values are booleans, numbers, strings
or lists of paths):

```json
{
  "schemaVersion": 1,
  "globalOptions": { "skip_gameinfo": true, "rompath": ["roms", "D:\\MAME\\roms"] },
  "profiles": [
    { "id": "default", "name": "Default", "options": {} },
    { "id": "big-screen", "name": "Big screen", "options": { "window": false } }
  ],
  "defaultProfileId": "default",
  "machineOverrides": { "pacman": { "bios": "japan" } },
  "softwareOverrides": { "nes:smb": { "speed": 1.5 } }
}
```

#### Commands

- `get_launch_config(installationId)` / `save_launch_config(installationId, config)`:
  the whole configuration of an installation, validated and saved atomically, with
  the option definitions for the interface.
- `get_software_launch_info(installationId, list, name)`: parts and candidate
  machines of a software.
- `preview_launch(request)`: the plan of a launch, without running it.
- `start_launch(request)`: starts MAME and returns the session.
- `get_launch_sessions`: in-memory statuses, polled while a session runs.
- `stop_launch_session(id)`: kills a running session.

A request names the installation, the machine, optionally the software with its part
assignments, and optionally a profile; the backend always rebuilds the command line
from it.

#### Interface

- Machine detail (`LaunchControls`): a compact launch bar below the media with the
  short information of the record on the left and, on the right, **Prepare
  Download** and **Download & Play**, **Play** and the profile selector (the real
  profiles of the installation, `Default (<name>)` first). The three actions are
  icon buttons with an accessible name and a tooltip explaining the action; Play is
  the only filled control, named `Starting…` (with a spinner and `aria-busy`) or
  `Running` while disabled. Below the bar, a collapsible **Launch status** section
  (`LaunchStatus`, shown only when there is a message or a session) keeps the
  launch errors and a summary line of the last session always visible (running
  with Stop and a confirmation, completed, failed with MAME's message and exit
  code, or stopped); its **Last launch** disclosure, collapsed by default and
  independent from Advanced, shows the installation, the start and end times, the
  exit code and the end of MAME's output already kept by the session (`logTail`).
  Both sections use the same `Disclosure` button and `.launch-section` frame.
  An **Advanced** section (`aria-expanded`, collapsed
  whenever a detail opens, with the number of overrides as counter) holds **Options
  for this machine** (the machine override) and **Command line** (the backend
  preview: executable, working directory, each argument with its role or the level
  its option comes from, and the full command line). Its content is mounted the
  first time it is opened and then kept, hidden, so collapsing it keeps an unsaved
  override draft; no preview is requested before it is opened. Devices and other
  non-runnable entries have no Play button.
- ROM downloads (`RomDownloadSection`) open a modal window (`shared/Modal.tsx`:
  `role="dialog"`, `aria-modal`, focus moved inside, trapped and returned to the
  opener, Escape and the backdrop close it) with the plan, the progress,
  cancellation, retry and the result; only the content scrolls, and the actions stay
  at the bottom of the window. Closing the window never cancels a download: while a
  transfer runs, Escape, the header button and **Hide** only hide it, and a button
  next to the actions (`Downloading 42%`, then the result) shows it again. A
  download started before the page was opened is shown the same way, without
  opening the window by itself.
- Software detail: the same bar, with a second row holding the machine selector
  (the candidates with their relation, filter result and whether they can mount the
  software; machines that cannot are disabled) and the media assignment (one
  selector per compatible device), then the badges of the chosen machine and a note
  on the limits of the catalog data; **Advanced** holds **Options for this
  software**.
- MAME page → Launch Options of an installation: its global options, profiles
  (create, rename, edit, make default, delete;
  the `default` profile cannot be deleted) and the list of machine and software
  overrides, which can be removed there.
- The option editor only lists supported options, grouped by category, and checks
  values as they are typed; the backend validates them again when saving.
- Whether the command line preview is shown is an interface preference.

#### Verified with MAME 0.280

Only the `neogeo` BIOS ROM set was installed, so running emulation was verified with
`pong` (no ROMs) and `neogeo`, and launches needing missing files were verified up to
MAME's own error:

- `pong` with `-video none -sound none -seconds_to_run 3` and `cfg`/`nvram`
  directories containing spaces and non-ASCII characters: running status, normal
  exit after about 6 seconds, configuration written in the given directory.
- `puckman`: exit code 2 with the missing ROM names in the output.
- `neogeo` with a profile setting `seconds_to_run 600`: the profile overrides the
  global option, a second launch is refused, the session keeps running, Stop kills
  MAME.
- `nespal -cart nes:smb:cart` and
  `apple2e -flop1 apple2_flop_orig:ultima4:flop1 -flop2 apple2_flop_orig:ultima4:flop2`:
  MAME resolves the software and parts and only reports missing files (exit code 2),
  never an unknown option, software or part.
- A catalog imported from another executable is refused before MAME runs.
- Closing the YMDB window while `neogeo` runs leaves MAME running.

#### Limitations

- ROM, CHD and software media availability is not checked before launching.
- Slot options (cards, expansions) are not configurable; parts are mounted on the
  media devices of the machine's default configuration.
- The `requirement` feature of parts (for example a disk needing another software)
  and the machine's RAM options are not evaluated for compatibility.
- Options are limited to the supported definitions; anything else is configured in
  MAME's own INI files.
- Sessions started before YMDB restarts are not tracked after the restart.

### Media sources — Implemented

Implemented in `src-tauri/src/media/` and `commands/media.rs`, typed in
`src/features/media/`. YMDB shows local artwork and screenshots from directories the
user chooses; it never copies, modifies, downloads or indexes images, and stores no
image in the catalog.

#### Categories and sources

- Categories are a registry (`media/config.rs`, mirrored in `mediaApi.ts`): `snap`
  (Snapshots), `titles` (Title screens), `flyers`, `cabinets`, `marquees`, named
  after the folders of the common MAME artwork collections. A category is added by
  registering it on both sides.
- Each category has an ordered list of directories (at most 16), chosen with the
  native folder picker, reordered or removed in **Settings → Media Sources**. They
  can be on local disks or network shares already mounted by the operating system;
  no remote protocol is implemented. Paths must be absolute; a directory that is
  missing, not a directory or unreadable is kept and shown with that status (an
  offline share is not an error).
- `media.json` (configuration directory, shared by all MAME installations,
  `schemaVersion` 1, atomic writes, validated
  on load and save):

  ```json
  {
    "schemaVersion": 1,
    "sources": {
      "snap": ["D:\\MAME Artwork\\snap", "D:\\MAME Artwork\\snap_SL"],
      "titles": ["D:\\MAME Artwork\\titles"]
    }
  }
  ```

  The backend keeps the configuration in memory, so image requests never read it.

#### Naming conventions

Checked against the artwork collections (for example the Progetto-SNAPS packs) and
MAME itself; images are matched by name only, not by content:

| Record   | Path in a source directory                            | Origin of the convention                               |
| -------- | ----------------------------------------------------- | ------------------------------------------------------ |
| Machine  | `<machine>.<ext>`, e.g. `snap/pacman.png`             | artwork collections, one folder per category           |
| Machine  | `<machine>/0000.png`, `<machine>/0001.png`, …         | screenshots saved by MAME (`snapname` default `%g/%i`) |
| Software | `<list>/<software>.<ext>`, e.g. `snap_SL/nes/smb.png` | software list artwork packs and MAME's internal UI     |

- Extensions, in order: `png`, `jpg`, `jpeg`, `webp` (displayed by WebView2,
  WebKitGTK and WKWebView). Other files (`bmp`, `gif`, archives) are ignored.
- A category can hold several images of a title (`resolve_all`): the directories
  are searched in order and each contributes its `<name>.<ext>` image (the first
  existing extension) and, for machines, its numbered screenshots (1 to 8 digits,
  sorted by number). A file reachable from two sources is listed once, and at most
  100 images are listed per category and title. The first image is the one used
  where a single image is needed (Media Grid, category thumbnails); the rule is
  deterministic: an earlier directory wins even with a less preferred format.
- Software names are only unique within a list, so the list is always part of the
  path, and a machine image is never used for a software of the same name.
- No parent/clone fallback: a clone can be a different title (regional names, hacks,
  sequels), so showing the parent's image could attribute it to the wrong game. A
  missing image shows a placeholder.
- Names are MAME short names (`[a-z0-9_]`, verified on all 48,893 machines and
  139,122 software of MAME 0.280), so file names are matched exactly as stored; on
  case-sensitive file systems image names must be lowercase, as in the collections.
- Zipped collections (`snap.zip`) are not read.

#### Serving images and security

- Images are served by the `ymdb-media` URI scheme registered with
  `register_asynchronous_uri_scheme_protocol`, handled on the blocking thread pool.
  The Tauri asset protocol is not enabled, so the webview has no general file
  access.
- Requests contain identifiers only: `<category>/machine/<machine>[/<index>]` or
  `<category>/software/<list>/<software>[/<index>]` (`mediaUrl` in `mediaApi.ts`,
  built with `convertFileSrc`, plus `?v=<version>` that changes when the sources are
  saved). `index` (decimal, no padding, below 100, default 0) selects an image in
  the order of `resolve_all`; index 0 never lists the screenshot directory when a
  named image exists.
- `list_media` (`commands/media.rs`, `protocol::list_images`) returns, for one
  machine or software, the categories with at least one image and their number of
  images, in category order. It runs on the blocking pool and looks up only the
  files of that title.
  The handler decodes the path, accepts only a registered category and MAME short
  names, and builds the candidate file names itself. There is no way to name a
  path, so traversal (`..`), absolute paths and other files are rejected (400) or not
  found (404).
- Each candidate is resolved with `canonicalize` and must still be inside the
  canonical source directory: symbolic links and junctions that point outside are
  refused, links within the directory are followed.
- Only regular files with a supported extension up to 32 MiB are served, with
  their content type, `X-Content-Type-Options: nosniff` and `Cache-Control:
max-age=3600` (errors are `no-store`). Missing, unreadable or unsupported files
  answer 404/500 and the tile shows its placeholder.
- The CSP allows images from `'self'`, `data:` and the `ymdb-media` scheme only.

#### Performance

- No scan: each image request checks at most (sources × 5) candidate paths, and the
  frontend only requests images of tiles near the viewport (`loading="lazy"`), so
  large collections cost nothing until displayed. Results are not cached by the
  backend: measurements showed no need.
- Measured (release build, Windows, local SSD, Classic/Media Grid on the 0.280
  catalog, a TEST collection of 48,893 generated images as first `snap` source plus
  two other sources): with 200 results per page, 30 images were requested for the
  visible area and the rows around it, and the visible images were displayed 60 ms
  after the page change; one image request took 5-6 ms (median, 8 ms at most). With
  small fixture sources, a request took 1.1 ms. JavaScript heap: 6-7 MB. Network
  shares were not measured; requests to an offline share only cost the failed
  look-ups of that share.
- No thumbnails are generated; images are served as they are.

#### Detail gallery

`src/features/media/MediaGallery.tsx` shows the artwork of one machine or software
on its detail page: a carousel of the images of one category, with the categories
in a grid of two columns on its right (three rows as tall as the carousel, so
all five categories fit). It does not resolve files itself: `list_media` tells
which categories have images of the title and how many, and the images are
requested by category and index through the same `ymdb-media` URLs as the Media
Grid (`mediaUrl`), so software artwork is looked up by list and software name.

- **Categories.** Only the categories with at least one image are shown, in
  registry order (snapshots, title screens, flyers, cabinets, marquees), each as a
  toggle button (`aria-pressed`) with its first image as thumbnail, its label and
  the number of images. The first one is selected initially. Selecting a category
  shows its images only, from the first one. Below 760 px the categories become a
  scrolling row under the carousel.
- **Carousel.** Previous/next buttons (hidden with a single image, disabled on the
  first and last image, no wrapping) and position dots (`aria-current`; as `n / N`
  text above 12 images). With the focus in the carousel, the arrow keys, Home and
  End move between images; the position is announced through a polite live region.
  When an arrow button becomes disabled under the focus, the focus moves to the
  image.
- **Records.** The gallery state is keyed by the record and the media version: a
  new record (Previous/Next, links) or saved sources list the images again from
  scratch, so images of the previous record are never shown while the new ones are
  listed.
- **Proportions.** The stage has a constant height (`clamp()` on the viewport
  height) and keeps it between images and categories, the dot row included, so
  nothing below moves; every image uses `object-fit: contain`, so flyers, cabinets
  and marquees are never cropped or stretched.
- **Lightbox.** Clicking the image opens a modal (`role="dialog"`, `aria-modal`,
  rendered in `document.body` through a portal) with the images of the selected
  category at the largest size that fits the window, the label and the position.
  Arrow keys and buttons move between them (no wrapping), Escape, the Close button
  or a click on the backdrop close it. Focus moves to Close when it opens, Tab cycles
  inside it, and focus returns to the carousel image when it closes; the image shown
  last stays selected.
- **States.** While the sources or the images are listed: a placeholder of the
  stage size; no configured directory or no image found: a one-line note, without
  categories; listing failure: a note with Retry; an image that cannot be read: a
  note in the stage, the other images stay available.
- Images are served with `Cache-Control: max-age=3600`, so the thumbnail, the
  carousel and the lightbox share one request per image. Images are decoded
  asynchronously (`decoding="async"`); no thumbnails are generated.

#### Isolated data directory

`YMDB_DATA_DIR`, when set, makes YMDB use `<dir>/config` and `<dir>/data` instead of
the platform directories. It is used to run end-to-end tests without touching the
user's settings, catalog, launch and media configuration, and can serve portable
setups.

### Catalog — Implemented (machines and software lists)

Implemented in `src-tauri/src/catalog/`:

- `parser.rs`: streaming parser for `mame -listxml` (quick-xml, event by event,
  reusable buffers). Memory use is bounded by one `<machine>` entry; the document
  is never loaded as a whole.
- `software_parser.rs`: streaming parser for `mame -listsoftware`; memory use is
  bounded by one `<software>` entry.
- `xml.rs`: helpers shared by both parsers (attributes, text, entities, numbers).
- `model.rs`, `software_model.rs`: plain data models, close to the XML.
- `schema.rs`: versioned SQLite schema (migrations tracked in `PRAGMA user_version`).
- `store.rs`: catalog database files, inserts and atomic replacement.
- `import.rs`: orchestration (validate, run MAME, parse, store, swap). Machines and
  software lists share `stream_mame_output`, built on `mame::process::spawn_streaming`.
- `freshness.rs`: whether a catalog matches its installation's executable.
- `search/`: full-text search and faceted filtering (see
  [Catalog search](#catalog-search--implemented)).

#### The `-listxml` format

Analyzed on the real output of MAME 0.280 (302 MB, 48,893 entries) and on the DTDs of
MAME 0.77 to 0.278:

- Each entry is a `<machine>` element (`<game>` before MAME 0.162; both are
  accepted). Entries include devices (`isdevice="yes"`, all `runnable="no"`; 6,802
  in 0.280), BIOS sets (`isbios="yes"`, 78) and mechanical machines
  (`ismechanical="yes"`, 15,683). Devices have no year, manufacturer or driver.
- The format does not say whether a machine is an arcade game, a console or a
  computer, so YMDB does not classify machines; only the XML flags are stored.
- `cloneof` names the parent; `romof` names the machine whose ROMs are inherited
  (the parent for clones, the BIOS for parents that need one); both always
  reference existing entries, sometimes listed later in the document.
  `sampleof` names a sample set, which is not always a machine (14 of 76 in 0.280).
- `device_ref` lists the devices a machine instantiates, with repetitions (one per
  instance); ROM names can repeat within a machine.
- Text values are not always numeric (years such as `199?`; control `ways` such as
  `3 (half4)`), and new attribute values appear over time (feature types,
  display types). Attributes were added over time (`feature` in 0.191, driver flags
  and `softwarelist@tag` in 0.229), so all of them are optional for the parser.
- MAME 0.280 on Windows with a decimal-comma locale (for example Italian)
  sporadically prints `refresh` rates with a comma (`61,419025`), nondeterministically
  across runs; the parser accepts a single comma as the decimal separator for
  decimal values.

#### The `-listsoftware` format

Analyzed on the real output of MAME 0.280 (102 MB; MAME needs 12-66 s to produce
it depending on system load, with continuous output and pauses below 2 s) and on
its DTD:

- The root `<softwarelists>` carries no version; the build is identified by running
  `-version` on the same executable.
- 720 `<softwarelist>` elements (name, description, optional `notes`) containing
  139,122 `<software>` elements: description, year, publisher, optional notes,
  `info` and `sharedfeat` name/value pairs, and `part` elements.
- Software names are unique within a list but not across lists (13,306 names
  appear in more than one list), so a software is identified by list and name.
- `cloneof` references a software of the same list; 4,656 parents are listed after
  their clones. `supported` is `yes` (default), `partial` (2,005) or `no` (39,054).
- A software can have several parts (23,272 do); each part has a name, an
  interface, `feature` name/value pairs, `dataarea` elements (name, size, optional
  `databits`/`endian`) with `rom` entries, and `diskarea` elements with `disk`
  entries. Parts and data areas can be many per software.
- `rom` entries without a name (482) are loading instructions (`loadflag` such as
  `reload` or `fill`, with an offset); `crc`/`sha1` are absent for `nodump` ROMs.
- `info` names can repeat within a software (153 cases).
- Compatibility between machines and software is expressed by MAME through the
  `filter` attribute of a machine's `<softwarelist>` reference (for example `NTSC`,
  `2,3`, `!DVC`) and the `compatibility` shared feature or part feature of software.
  Both are stored verbatim; YMDB does not evaluate them yet.
- Machines of 0.280 reference 733 list names, all 720 provided lists among them;
  13 referenced names are not provided by MAME (for example `sorcerer_quik`).
- The DTD allows part DIP switches and list notes; MAME 0.280 outputs neither.

#### Schema (version 3)

Migration 1 creates the machine tables, migration 2 the software list tables and
migration 3 the search structures described in
[Catalog search](#catalog-search--implemented).

| Table                             | Content                                                                                                                                                                                                                                                                             |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `catalog_metadata`                | Single row: MAME release/revision/build, `mameconfig`, executable path/size/mtime, import time and duration, machine count, status                                                                                                                                                  |
| `machines`                        | One row per entry: name, description, year, manufacturer, source file, `is_bios`/`is_device`/`is_mechanical`/`is_runnable`, `clone_of`, `rom_of`, `sample_of`, sound channels, input summary (players, coins, service, tilt), driver status/emulation/cocktail/save state and flags |
| `machine_roms`                    | Name, BIOS set, size, CRC, SHA-1, merge name, region, offset, dump status, optional                                                                                                                                                                                                 |
| `machine_disks`                   | CHDs: name, SHA-1, merge name, region, index, writable, dump status, optional                                                                                                                                                                                                       |
| `machine_bios_sets`               | BIOS set name, description, default flag                                                                                                                                                                                                                                            |
| `machine_device_refs`             | Devices instantiated by a machine, with instance count                                                                                                                                                                                                                              |
| `machine_samples`                 | Sample names                                                                                                                                                                                                                                                                        |
| `machine_chips`                   | CPU/audio chips: type, name, tag, clock                                                                                                                                                                                                                                             |
| `machine_displays`                | Type, tag, rotation, flip, width, height, refresh rate                                                                                                                                                                                                                              |
| `machine_controls`                | Control type, player, buttons, required buttons, ways                                                                                                                                                                                                                               |
| `machine_features`                | Imperfect/unemulated features: type, status, overall                                                                                                                                                                                                                                |
| `machine_media_devices`           | Media devices (cartridge, floppy, ...): type, tag, interfaces, instance names, mandatory/fixed image                                                                                                                                                                                |
| `machine_media_device_extensions` | Accepted file extensions per media device                                                                                                                                                                                                                                           |
| `machine_software_lists`          | Software lists referenced by a machine: list name, tag, status, filter                                                                                                                                                                                                              |
| `machine_ram_options`             | RAM options: name, size, default flag                                                                                                                                                                                                                                               |

Software list tables (migration 2):

| Table                       | Content                                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------------------- |
| `software_catalog_metadata` | Single row: MAME release/revision, executable path/size/mtime, import time and duration, counts, status |
| `software_lists`            | Name (`UNIQUE`), description, notes                                                                     |
| `software`                  | List name, name (`UNIQUE` per list), clone of, supported, description, year, publisher, notes           |
| `software_info`             | `info` name/value pairs                                                                                 |
| `software_shared_features`  | `sharedfeat` name/value pairs (for example `compatibility`)                                             |
| `software_parts`            | Part name and interface                                                                                 |
| `software_part_features`    | Part `feature` name/value pairs                                                                         |
| `software_data_areas`       | Name, size, data bits, endianness                                                                       |
| `software_roms`             | Name (nullable), size, length, CRC, SHA-1, offset, value, dump status, load flag                        |
| `software_disk_areas`       | Name                                                                                                    |
| `software_disks`            | Name, SHA-1, dump status, writeable                                                                     |

Design rules:

- All tables are `STRICT`. Flags are `0`/`1` with `CHECK` constraints.
- Machines are referenced by their MAME short name (`machines.name`, `UNIQUE`),
  which is stable across MAME versions. `machines` keeps an integer rowid so that a
  future FTS5 index can use it as `content_rowid`.
- `clone_of`, `rom_of` and `machine_device_refs.device_name` are foreign keys
  `DEFERRABLE INITIALLY DEFERRED`, checked at commit, because the XML references
  entries listed later. Child rows reference `machines(name)` with `ON DELETE CASCADE`.
- Child tables have no uniqueness constraints, because MAME can repeat items.
- Values whose set grows with MAME versions are stored as text without `CHECK`.
- Indexes exist on every foreign key and on `clone_of`, `rom_of`,
  `machine_device_refs.device_name` and `machine_software_lists.list_name`.
- Software are identified by `(list_name, name)`. `software.clone_of` is a deferred
  composite foreign key to a software of the same list. Parts, areas, ROMs, disks
  and metadata hang off integer ids with `ON DELETE CASCADE`.
- Machines are linked to software lists by name: `machine_software_lists.list_name`
  = `software_lists.name`, joined through the existing index. It is not a foreign
  key because machines reference lists that MAME does not provide; the summary
  counts them. A list is stored once, however many machines use it, and the
  machine-side attributes (`status` `original`/`compatible`, `filter`, `tag`) stay
  on the relation. A relation does not mean that every software of the list runs
  on every configuration of the machine.
- `info`, `sharedfeat` and `feature` names are free-form metadata defined by each
  list; they are stored as name/value rows, not interpreted as categories.
- Data bits and endianness are stored with the DTD defaults (8, `little`) applied.

Not stored in this version (deferred): for machines, DIP switches and their
locations/values, configuration switches, input ports and analog masks, adjusters,
slots and slot options, conditions, display timing details (pixel clock, blanking),
analog control parameters (minimum, maximum, sensitivity, key delta, reverse), and
the legacy pre-0.191 driver attributes (`color`, `sound`, `graphic`, `protection`).
These matter for launching and configuring machines and will be added with the
launcher. For software, part DIP switches (allowed by the DTD, not output by MAME
0.280). Everything else in `-listsoftware` is stored.

#### Import strategy

Each installation's catalog is a separate database file,
`installations/<id>/catalog.sqlite`, containing only data derived from MAME. An
import of an installation:

1. Validates the installation's executable with `-version` and records its release,
   revision, path, size and modification time.
2. Creates `catalog.sqlite.partial` from scratch (journaling off, foreign keys on),
   applies the migrations and opens a single transaction.
3. Runs `<mame> -listxml` from the executable's directory through `spawn_streaming`
   (inactivity timeout 120 s; no total timeout, no output size limit) and parses
   its standard output incrementally, inserting each machine with cached prepared
   statements. Progress is an atomic counter of processed machines.
4. After the end of the document, waits for MAME and requires exit code 0.
5. Checks that the number of stored machines equals the number parsed, runs
   `PRAGMA foreign_key_check` (reporting the first violations by name), writes the
   metadata row, rebuilds the full-text search indexes and commits (deferred foreign
   keys are enforced here).
6. Closes and syncs the partial file and renames it over `catalog.sqlite` while
   holding the lock that also guards catalog reads.

Failure handling: a parse error, a MAME failure (non-zero exit, inactivity), a
database error, an integrity violation or a cancellation before step 6 discards the
partial file and leaves the previous catalog untouched and readable. A crash leaves
at most a partial file, which the next import deletes. Re-importing (for example
after updating MAME) follows the same steps and replaces the catalog only on
success. Cancellation sets a flag that the parser checks before each machine and the
watchdog uses to kill MAME; it is honored until the final commit.

A **software list import** uses the same replacement mechanism:

1. Validates the installation's executable and requires a machine catalog built from
   the same MAME release and revision, read from the same unchanged executable
   (path, size and modification time); otherwise it fails with `catalogMismatch`
   (or `machineCatalogRequired`) before running `-listsoftware`.
2. Copies `catalog.sqlite` to `catalog.sqlite.partial` (under the file lock), empties
   the software list tables of the copy and opens a single transaction. Machine
   data is copied unchanged.
3. Streams `<mame> -listsoftware` through the same `stream_mame_output` as machines
   (same watchdog, inactivity timeout and exit code check), inserting each list and
   software with cached prepared statements. Progress counts software.
4. Checks the stored list and software counts, checks again that the machine
   catalog in the file comes from the same build, runs `PRAGMA foreign_key_check`,
   writes `software_catalog_metadata`, rebuilds the software search index and
   commits.
5. Syncs and renames the file over `catalog.sqlite`, as for machines.

Consequences: an interrupted, failed or cancelled software import keeps both the
machines and the previous software lists. Re-importing software lists replaces them
entirely. Re-importing machines starts from an empty file, so the new catalog has no
software lists until they are imported again from the new build: software lists of
one build are never attached to machines of another. A future "full update" can
write machines and software lists into the same partial file with the same
`CatalogWriter` before a single swap.

Only one import (of either kind, for any installation) runs at a time. On Windows a file cannot be
renamed over while a handle to it is open: every connection to `catalog.sqlite` is
opened and closed under the store's file lock, which the rename also holds, so the
application never blocks its own replacement. If another program keeps the file
open, the rename fails with a message asking to close it, and the previous catalog
is kept.

User data (future favorites, collections, launch profiles) must not live in
`catalog.sqlite`, which is replaced on every import; it will be stored separately and
reference machines by short name and software by list and short name.

#### MAME versions

A catalog is tied to the MAME build it was imported from (`catalog_metadata`); its
software lists, when present, record their own build in `software_catalog_metadata`
and are only accepted from the same build and executable. The catalog status
compares the catalog, without running MAME, with its installation's executable:
`current`, `executableChanged` (same path, different size or modification time,
for example after an in-place update), `differentExecutable` or
`executableMissing`. The catalog is never treated as valid for another executable;
the UI asks to import again, and software lists can only be imported when the
status is `current`.

Schema versions: a catalog created by an older release is upgraded in place when it
is read (migrations only add tables, so existing machine data stays valid; version
1 catalogs gain empty software list tables). A catalog from a newer release is
reported as unreadable and must be imported again.

#### Interface

The **Catalog** section of each installation on the MAME page
shows, for machines, whether a catalog exists, its MAME
version, import date and statistics (entries, runnable systems, clones, BIOS sets,
devices, mechanical, ROMs, disks, referenced software lists, database size, import
duration) and the freshness warning; for software lists, whether they are imported,
their MAME version, import date and statistics (lists, software, clones, partially
supported, unsupported, parts, ROMs, disks, referenced lists not provided by MAME,
import duration). Buttons import or reimport machines, import or update software
lists (enabled only when the catalog matches the installation's executable) and
cancel the running import; while another installation imports, it says so and
waits. While importing it polls an in-memory status every 500 ms and
shows an indeterminate progress bar with the number of machines or software
processed: totals are not known in advance, so no percentage is shown. Summaries are
read from the database only when an import ends.

#### Limitations

- Machine/software compatibility (`filter` and `compatibility` values) is stored
  but not evaluated.
- Re-importing machines discards the software lists; there is no combined "full
  update" yet.
- Deferred XML data is listed above.
- Imports cannot be cancelled during the final commit and file replacement.

Planned: full catalog update (machines and software lists in one operation),
favorites, collections, screenshots, artwork and metadata.

### Catalog details — Implemented

`src-tauri/src/catalog/detail/` reads one record with every imported attribute,
exposed by `get_machine_detail(name)`, `get_software_detail(list, name)` and
`get_software_list_detail(name)` (`commands/detail.rs`), typed in
`src/features/browser/detailApi.ts`. These DTOs are separate from the lightweight
search DTOs.

- `MachineDetail`: identity, flags, source file, driver (status, emulation, cocktail,
  save states, artwork/unofficial/no sound/incomplete), emulation features, sound,
  input and controls, displays, chips, BIOS sets, ROMs, disks, device references,
  media devices with extensions, software lists, RAM options, samples and
  `classifications` (genres, category paths, series, version added from the catalog's
  projection; empty without support files). Parent
  (`cloneof`) and ROM parent (`romof`) are `RecordLink`s with the target
  description, or `null` description when the target is not in the catalog; the
  same applies to device references. Each software list says whether it is
  available in the imported software lists.
- `SoftwareDetail`: list and its description, identity, publisher, year, notes,
  support status, parent, information, shared features and parts with features,
  data areas (ROMs and load instructions) and disk areas.
- `SoftwareListDetail`: description, notes, availability, software count and the
  number of machines declaring the list as `original` or `compatible`. A list name
  referenced by machines but not provided by MAME returns a detail with
  `available: false`; an unknown name returns `null`.
- One-to-many relations that can be large (clones, software of a list, machines of
  a list) are not embedded: details carry counts and the frontend lists them
  through the paginated search (`cloneOf`, `lists`, `softwareLists` filters).
- Every command returns `null` when the record does not exist (for example after a
  reimport removed it), and `catalogNotImported` without a catalog. Queries run
  through `CatalogStore::with_reader` on the blocking pool; on the 0.280 catalog a
  detail takes 1-3 ms.

### Catalog search — Implemented

Implemented in `src-tauri/src/catalog/search/`, exposed by the commands
`search_catalog_machines` and `search_catalog_software` (`commands/search.rs`) and
the typed wrappers in `src/features/search/searchApi.ts`. Machines and software are
searched separately; there is no combined search.

- `request.rs`: request models (deserialized from the frontend; unknown fields are
  rejected) and validation.
- `text.rs`: conversion of the search text into a safe FTS5 query.
- `sql.rs`: composition of `WHERE` conditions. Only SQL fragments written in the
  code are concatenated (columns, sort orders and subqueries chosen through enums);
  every value from a request is a bound `?` parameter.
- `machines.rs`, `software.rs`: count, page, facets and result mapping to
  lightweight DTOs (`MachineSummary`, `SoftwareSummary`) without relations.
- `benchmark.rs`: an ignored test measuring representative queries on a real
  catalog and printing their `EXPLAIN QUERY PLAN`.

The functions take a read-only connection. `CatalogStore::with_reader` keeps one
cached read-only connection under the store's file lock (so its page cache survives
between searches) and closes it before the catalog file is replaced. Searches run
on the blocking thread pool, one at a time.

#### Full-text indexes

Migration 3 creates two FTS5 external-content tables: `machines_fts` (name,
description, manufacturer, year) over `machines`, and `software_fts` (name,
description, publisher, year, list name) over `software`. They index the text
without storing a copy. The tokenizer is `unicode61 remove_diacritics 2`: words are
runs of Unicode letters and digits, case- and accent-insensitive; punctuation and `_`
separate words. Other metadata (`info` such as alternative titles) is not indexed
yet: it would need a content view or a second copy of the text.

The catalog is only written by imports, which rebuild both indexes (`'rebuild'`) in
the partial file just before committing; the atomic replacement is unchanged and the
previous catalog, with its indexes, stays available until the swap. Existing
catalogs are indexed when migration 3 is applied (when they are first read). FTS5
`prefix` indexes were measured and not used: they doubled the index size while
2- and 3-character prefix queries already took about 1 ms without them.

Migration 3 also adds indexes for result orders: `machines(description COLLATE
NOCASE, name)`, `software(name, list_name)` and `software(description COLLATE
NOCASE, list_name, name)`.

#### Query semantics

- The text is split into words (runs of Unicode letters and digits); any other
  character separates words. Each word is sent to FTS5 as a quoted prefix
  (`"word"*`), so FTS5 operators typed by the user (`"`, `*`, `:`, `(`, `-`, `^`,
  `AND`, `OR`, `NOT`, `NEAR`) are never interpreted.
- All words must match (AND), each in any indexed column, as a word prefix:
  `pac` matches "Pac-Man" and `pacman`; `street fight` matches "Street Fighter";
  `capcom 1992` combines a manufacturer and a year. Matching ignores case and
  diacritics (`attache` matches "Attaché"). Text in scripts without spaces, such as
  Japanese, matches from the beginning of each run (`ストリート` matches
  "ストリートファイターIII"). Words are prefixes, not substrings: `man` does not match
  `pacman`.
- Text that is empty or has no letter or digit does not restrict the results.
  At most 200 characters and 16 words are accepted.
- With search text, the default order is relevance: an exact short name match
  first (`sf2` before `sf2ce`), then FTS5 bm25 with column weights
  name 10, description 4, manufacturer/publisher 2, year 1 (and list name 1).

#### Filters

Within a filter, values are alternatives (OR); different filters must all match
(AND). `null` in a value list selects entries without a value. All filters use data
of the catalog only; no machine is classified as arcade, console or computer.

- Machines: manufacturers, years (as printed, such as `199?`), parents or clones,
  clones of one parent (`cloneOf`),
  BIOS, device, mechanical, runnable, driver status, emulation status, has ROMs,
  has disks (CHD), software lists. "Has ROMs"/"has disks" mean that the catalog
  declares at least one ROM/disk entry (any dump status, including `nodump`); they
  say nothing about files present on disk. The software list filter uses the
  machines' references, so lists that MAME does not provide can still be selected.
- Software: lists, publishers, years, parents or clones, clones of one parent
  (`cloneOf`, combined with `lists` to select the parent's list), support status (`yes`,
  `partial`, `no` as declared by MAME), has ROMs, has disks, part interfaces. The
  support status is unrelated to media availability.
- Both: `list`, the id of a [user list](#user-lists-favorites--implemented) such as
  `favorites`. Lists are not in the catalog: the command resolves the list from the
  global lists and passes its entries to the search (`ListMembers`) as one JSON
  parameter, matched with `m.name IN (SELECT value FROM json_each(?))` (software:
  `(s.list_name, s.name) IN (...)`). The restriction is an ordinary AND condition, so
  it applies before the count, the facets and the page.
- Machines, from the support files (see
  [MAME support files](#mame-support-files--implemented)): `genres`, `categories`
  (category nodes: a node selects its descendants), `series`, `versionsAdded` and
  `versionAddedRange` (`from`/`to`, inclusive, in MAME version order; an alternative
  of `versionsAdded`). They read the catalog's projection, attached as `cls`, with
  uncorrelated `m.name IN (SELECT machine_name FROM cls.memberships WHERE dimension =
'genre' AND value IN (...))` subqueries. Values the projection does not have (a
  removed source, another catalog) match nothing, so an unresolvable criterion never
  widens the results. Range bounds must be MAME versions.

#### Facets

Requested facets return `(value, count)` pairs: machines by manufacturer, year,
driver status, emulation status and software list, and genre, category, series
and version added when the catalog has support files; software by list, publisher,
year, support status and interface.

- Counts use disjunctive semantics: each facet applies the search text and all
  filters except its own. Selecting a manufacturer therefore keeps the other
  manufacturers visible with their counts (to add alternatives), while the other
  facets count only the matching results.
- Machines referencing a list, and software with several parts on one interface,
  are counted once per value.
- Years are listed chronologically, versions added in MAME version order (by the
  projection's sort key, never as text), category nodes in path order (parents
  before children; each machine counts once per node, so a parent counts its
  descendants), other facets by decreasing count, then value. `null` values come
  last. A machine in two series counts in both.
- Each facet returns at most `limit` values (default 50, at most 1,000, enough for
  every category node and series of the support files) with
  `hasMore` when more exist, and accepts an optional `search` substring (ASCII
  case-insensitive) to find values of large facets (3,140 manufacturers in 0.280).
  Counts are computed by SQL `GROUP BY` queries; results are never loaded in memory
  to be counted.

#### Pagination and sorting

- Offset pagination: zero-based `page` (at most 100,000) and `pageSize` (1-200,
  default 50). The response contains `total`, `pageCount`, the items and the facets.
  A page beyond the end is empty. Offset pagination was kept because measured deep
  pages stay below 31 ms with the order indexes (see below); keyset pagination was
  not needed for these volumes.
- Sorts: machines by relevance, name, description, year, manufacturer; software by
  relevance, name, description, year, publisher, list; ascending or descending.
  Every order ends with the unique key (machine name; software list and name), so
  pages are stable. `null` years, manufacturers and publishers come last in both
  directions; ties are broken by description and name in ascending order. Relevance
  without search text falls back to the name.
- Text sorts use SQLite `NOCASE` collation, which folds ASCII letters only.

#### Measured performance

Measured on the MAME 0.280 catalog (48,893 machines, 139,122 software) with
`search_benchmark`, release build, Windows: "first" is the first execution on a new
connection (empty SQLite page cache, warm operating system cache), "median" the
median of 15 executions on a warm connection. Each search includes the count and
the first page of 50 items.

| Search                                        | Results | First (ms) | Median (ms) |
| --------------------------------------------- | ------- | ---------- | ----------- |
| Machines, no text, by name                    | 48,893  | 1.7        | 0.1         |
| Machines, exact name `pacman`                 | 36      | 1.0        | 0.2         |
| Machines, `street fighter`                    | 286     | 1.8        | 0.6         |
| Machines, one-letter prefix `s`               | 28,328  | 37.9       | 37.0        |
| Machines, text + 6 filters                    | 64      | 1.9        | 0.5         |
| Machines, software list filter                | 473     | 1.9        | 0.5         |
| Machines, by year / manufacturer              | 48,893  | 10.0       | 9.0         |
| Machines, 5 facets, no text                   | 48,893  | 66.2       | 66.3        |
| Machines, 5 facets, text `fighter`            | 574     | 4.0        | 1.9         |
| Machines, page at offset 45,000, description  | 48,893  | 5.2        | 3.6         |
| Software, no text, by name                    | 139,122 | 4.9        | 3.6         |
| Software, `super mario`                       | 215     | 3.1        | 1.7         |
| Software, one-letter prefix `s`               | 75,908  | 111.5      | 111.0       |
| Software, text + list + support filters       | 36      | 2.2        | 0.5         |
| Software, has ROMs = false                    | 10,055  | 106.4      | 102.8       |
| Software, interface filter                    | 20,549  | 20.8       | 19.0        |
| Software, 5 facets, no text                   | 139,122 | 162.0      | 161.9       |
| Software, 5 facets, text `mario`              | 454     | 5.0        | 3.4         |
| Software, page at offset 130,000, description | 139,122 | 31.2       | 30.4        |

Optimizations made from these measurements: relation filters are uncorrelated `IN`
subqueries (a correlated `EXISTS` on software lists took 3.3 s), and the order
indexes turned full sorts into index scans (deep software pages took 0.6-0.7 s).
The search structures add about 28 MiB to the 0.280 catalog (FTS 13 MiB, order
indexes 15 MiB), for a total of 231 MB.

#### Known limitations

- Prefix, not substring, matching; no fuzzy matching or typo tolerance.
- One-letter searches over all software, the unfiltered software facets
  (30-50 ms each, about 160 ms for all five) and "software without ROMs" are the
  slowest searches (about 0.1-0.16 s).
- Sorting by year or manufacturer without text uses a temporary sort (about 9 ms
  for all machines).
- `NOCASE` sorting and facet value search fold ASCII case only.
- Searches are serialized on the catalog store lock.

### Local storage — Implemented (partially)

The application uses two platform directories resolved by Tauri for the bundle
identifier `io.github.davidfriscia.ymdb`: the configuration directory
(`app_config_dir`) for small user files, and the local data directory
(`app_local_data_dir`) for large, machine-specific derived data that should not roam.

| Platform | Configuration directory                                                  | Local data directory                                                        |
| -------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| Windows  | `%APPDATA%\io.github.davidfriscia.ymdb`                                  | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb`                                |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb`              | `~/Library/Application Support/io.github.davidfriscia.ymdb`                 |
| Linux    | `$XDG_CONFIG_HOME/io.github.davidfriscia.ymdb` (default `~/.config/...`) | `$XDG_DATA_HOME/io.github.davidfriscia.ymdb` (default `~/.local/share/...`) |

Configuration directory:

- `installations.json`: the registered MAME installations and the active one (see
  [MAME installations](#mame-installations--implemented)).
- `installations/<id>/launch.json`: the launch configuration of each installation
  (see [Launcher](#launcher--implemented)).
- `installations/<id>/remote-sources.json`: the ROM Manager settings of each
  installation (see [Remote repositories](#remote-repositories--implemented-connections-sources-sftp-provider)).
- `remote-connections.json`: the remote connections, shared by all installations.
- `lists.json`: the user lists (Favorites), shared by all installations (see
  [User lists](#user-lists-favorites--implemented)).
- `media.json`: the media sources, shared by all installations (see
  [Media sources](#media-sources--implemented)).
- `saved-filters.json`: the saved filter presets, shared by all installations (see
  [Saved filter presets](#saved-filter-presets--implemented)).
- `settings.json` and `launch.json`: files of the single-installation layout. They are
  only read by the migration and never written again, so they remain as a backup.

Local data directory:

- `installations/<id>/catalog.sqlite`: the catalog of each installation (see
  [Catalog](#catalog--implemented-machines-and-software-lists)).
- `installations/<id>/classifications.sqlite`: the projection of the support file
  classifications for that catalog (derived data, rebuilt when needed).
- `support-files.sqlite`: the global repository of imported support files (see
  [MAME support files](#mame-support-files--implemented)).
- `launch-logs/`: the output of MAME sessions (the 20 most recent are kept).

All JSON files are written atomically (a temporary file in the same directory is
written, synced and renamed over the target) and validated when read. Access is
serialized by mutexes held in Tauri managed state. `YMDB_DATA_DIR` replaces both
directories with one directory (`config/` and `data/`), for isolated test runs.

Implemented: interface preferences (theme, panels, result renderer and options,
command line preview) in the webview `localStorage` (see
[Interface preferences](#interface-preferences)). They live in the webview data
directory and are shared by all MAME installations.

ROM, CHD and sample download destinations are configured per installation by the ROM
Manager (`remote-sources.json`).

### User lists (Favorites) — Implemented

Implemented in `src-tauri/src/lists/` (model, persistence, export format),
`commands/lists.rs` (state and commands) and `src/features/lists/` (provider,
bookmark, Settings section). Favorites is the only list of this version; it is
modeled as a _system list_ so that lists created by the user can be added without
changing the persistence, the entries or the export format.

#### Model

```text
Lists { schemaVersion, lists: [List] }
List  { id, kind: system | user, name?, entries: [ListEntry] }
ListEntry = { kind: "machine", name }               (MAME short name)
          | { kind: "software", listName, name }    (software list + short name)
```

- **Identity.** An entry references a machine by its MAME short name and a software
  by its software list and short name, the same identity MAME uses. Never a catalog
  row id, installation id, executable path, description, result position or ROM
  file name. The same entry therefore matches the same title in the catalog of every
  installation (0.252 and 0.280 alike); a software name is unique per list only, so
  `nes`/`tetris` and `c64_cart`/`tetris` are different entries, and a machine and a
  software with the same name never mix. Names are validated as MAME names (1-64
  printable ASCII characters, without whitespace, `/`, `\` or `:`), so an entry has
  one textual key (`machine:pacman`, `software:nes:smb`).
- **Missing entries are kept.** Lists do not depend on any catalog: an entry missing
  from the active catalog is neither removed nor reported, and matches again with an
  installation that has it. No approximate matching (description, manufacturer) is
  ever attempted, and a renamed short name is another entry.
- **System lists** (`SYSTEM_LISTS` in `lists/model.rs`: only `favorites`, displayed
  as "Favorites") have a stable id, no stored name, cannot be renamed or deleted,
  and are added automatically when missing. **User lists** (`kind: "user"`) have an
  id (`[A-Za-z0-9-]`, not a system id) and a name; the model validates and keeps
  them, but no command creates them yet.
- **Entries** keep the order in which they were added, without duplicates, at most
  100,000 per list. `set_membership(list, entry, member)` sets the wanted state and
  is idempotent; the interface toggles by sending the opposite of the current state.

#### Persistence

`lists.json` in the configuration directory (user data that may roam, next to
`installations.json`), validated when read and written atomically like every other
JSON file. A global SQLite database was considered and not used: the data is small
(a few thousand short names), the JSON pattern with atomic writes already exists,
and the search needs no join with it (see below). A missing file means empty system
lists and is not written until the first change. An unreadable or invalid file
(malformed JSON, unknown version or fields, invalid or duplicate entries) is
reported by every lists command, including searches with Favorites only, and never
replaced; it is read again on next use, so a file fixed by hand works without
restarting.

`ListsState` keeps the lists in memory behind a mutex. A change is applied to a
copy, saved, and only then kept: a failed save (full disk, permissions) leaves the
lists and the file as they were, and the command reports the error. Catalog
imports, installation changes and removals never touch `lists.json`, and changing a
favorite never writes to a catalog.

#### Search integration

Searches take an optional `list` filter (see [Filters](#filters)). The search
command reads the list's entries of the searched kind and passes them as a single
JSON array bound to `json_each(?)` in an uncorrelated `IN` subquery, so:

- the catalog is never modified, attached to another database or re-imported when a
  favorite changes;
- the restriction is one more AND condition, applied before counting, faceting,
  sorting and paging, with the text and every other filter;
- SQLite reads the array once and looks entries up by the unique indexes
  (`machines.name`, `software(list_name, name)`); with 2,000 favorites on the 0.280
  catalog (48,893 machines, 139,122 software) a results page with all five facets
  takes about 19 ms for machines and 40 ms for software (`search_benchmark`), less
  than the same search over the whole catalog (65 ms and 159 ms).

#### Interface

- `FavoritesProvider` (global, above every page) loads the Favorites list once with
  `get_list` and answers membership in memory, so cards never send one request each;
  switching installation does not reload it. `toggle` updates the bookmark at once,
  sends `set_list_membership`, ignores clicks on an entry being saved, and on
  failure restores the previous state and shows an error banner in the catalog.
  Every saved change increments `revision`: with Favorites only, the results and
  facets reload, so a removed favorite disappears, counts and pages follow, and an
  emptied last page moves to the new last page.
- `FavoriteBookmark` is the only control: a toggle button (`aria-pressed`, accessible
  name "Favorite: <title>", tooltip "Add to favorites" / "Remove from favorites",
  native keyboard support, visible focus, `aria-busy` while saving, disabled with an
  explanation when the list cannot be read). On cards it is an overlay in the top
  right corner, semi-transparent until the card is hovered or the bookmark hovered
  or focused, and filled with the accent color when the entry is a favorite; on
  detail pages it sits next to the title of the information bar. Colors come from
  the theme tokens only, so both themes are covered.
- **Settings › Favorites** shows the number of favorites, exports them and imports a
  file (see below).

#### Export and import

`export_lists` writes the chosen lists to a file picked with the native save dialog:

```json
{ "format": "ymdb-lists", "version": 1, "lists": [ { "id": "favorites", "kind": "system", "entries": [ ... ] } ] }
```

Lists have the same shape as in `lists.json`: ids, kinds, names and entries only,
never paths, installation ids, connections, credentials or launch settings. The
format can carry several lists, so exporting user lists later keeps its meaning.

Import is two steps. `preview_lists_import` opens the native file picker, reads at
most 16 MB, checks `format` and `version` (another format or version is refused with
a clear message), the structure (unknown fields are rejected) and every entry, and
counts the distinct entries, the new ones, those already present, the duplicates of
the file (imported once) and the entries a replace would remove. Lists of the file
that this version cannot import (user lists, unknown system lists) are named and
ignored. Nothing is changed; the checked import is kept in memory with an id.
`apply_lists_import(id, mode)` then applies it to every list at once (`merge` appends
the new entries; `replace` keeps exactly the file's entries) with a single atomic
save, so a failure leaves the previous lists. Entries missing from the active catalog
are imported like any other. Paths in the file are never used.

#### Future user lists

The model, persistence, search filter and export format already handle any number
of lists. Adding user lists means: commands to create (new `user` list with a
generated id and a name), rename (change `name`) and delete (remove a `user` list;
system lists refuse both) lists; `set_list_membership` and `ListMembers` already take
any list id, so "add to list" and "list only" views reuse them; membership in several
lists is a lookup per list; exporting several lists passes several ids, and the
import summarizes and applies each importable list (user lists would be created or
merged by id). The interface would add a list picker next to the bookmark and list
management in Settings. Manual ordering would only need the entries' order, which is
already preserved.

#### Commands

| Command                | Purpose                                                    |
| ---------------------- | ---------------------------------------------------------- |
| `get_list`             | A list with its entries (`ListView`).                      |
| `set_list_membership`  | Adds or removes one entry; returns the new membership.     |
| `export_lists`         | Writes lists to a JSON file chosen by the user.            |
| `preview_lists_import` | Chooses and checks a file; returns the summary to confirm. |
| `apply_lists_import`   | Applies a previewed import in `merge` or `replace` mode.   |

### MAME support files — Implemented

Third-party _support files_ classify MAME machines; MAME itself does not. YMDB imports
them manually into a global repository and projects the active classifications into
each catalog, where the search uses them like any other filter.

```text
support file (.ini, chosen by the user, never modified)
        │ parse, validate, preview, confirm
        ▼
Global repository (support-files.sqlite)   one source per format, active source and
        │                                  revision per dimension
        │ revision and catalog comparison, when a catalog is accessed
        ▼
Catalog projection (installations/<id>/classifications.sqlite)
        │ attached read-only as `cls`, matched by machine short name
        ▼
Catalog search, facets and machine detail
```

#### Formats

Analyzed on the files of MAME 0.288 (progettoSNAPS, `pS_SupportFiles_288`): UTF-8
without BOM, LF line endings, a header comment such as
`;; catver.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;`, and the frontend sections
`[FOLDER_SETTINGS]` (settings lines) and `[ROOT_FOLDER]` (empty).

| File          | Structure                                                         | Dimensions              | 0.288                                         |
| ------------- | ----------------------------------------------------------------- | ----------------------- | --------------------------------------------- |
| `catver.ini`  | `[Category]` and `[VerAdded]` sections of `machine=value` lines   | Category, Version Added | 50,096 machines, 676 categories, 694 versions |
| `genre.ini`   | one section per genre listing machines                            | Genre                   | 50,096 machines, 58 genres                    |
| `catlist.ini` | one section per category (`Arcade: Shooter / Flying Vertical`)    | Category                | 50,096 machines, 695 categories               |
| `version.ini` | one section per MAME version listing the machines added in it     | Version Added           | 50,096 machines, 694 versions                 |
| `series.ini`  | one section per series; a machine may be in several (18 in 0.288) | Series                  | 12,399 machines, 720 series                   |

Other files of the same packages (`mature.ini`, `genre_ows.ini`, `category.ini`, ...)
are refused by name as unsupported.

#### Parsers (`support_files/parser.rs`)

- **Identification.** The header comment names the file (`GENRE.ini`, any case); a
  header naming another file is refused (`unsupported`). Without a header the
  structure decides when it can (`key=value` lines in `[Category]`/`[VerAdded]`:
  catver; only version sections: version), and only then the file name; genre,
  catlist and series files without a header or a telling name are refused
  (`unidentified`) rather than guessed. A header that contradicts the structure (a
  "genre.ini" with catver sections) is refused.
- **Decoding.** UTF-8 with or without BOM; otherwise Windows-1252 with a warning.
  `\n`, `\r\n` and `\r` line endings. Lines starting with `;` or `#` are comments;
  blank lines are ignored; entries are trimmed.
- **Model.** Every format becomes `ParsedSupportFile`: sorted, deduplicated
  _memberships_ (machine → value, possibly several: Genre, Category, Series) and
  _attributes_ (one value per machine: Version Added). `[VerAdded]` is read as an
  attribute, never as a category; `catver.ini` sections other than `[Category]` and
  `[VerAdded]` are ignored with a warning.
- **Validation.** Machine names must be MAME short names (`[a-z0-9_]`, 1-64).
  Duplicates are kept once, a second value of an attribute is ignored (the first one
  wins), malformed lines are skipped, and machines listed in `[ROOT_FOLDER]` are not
  classified; each case is a warning with a count and the first five examples.
  Versions that cannot be ordered are kept and reported. A file without any entry is
  refused. Files are limited to 64 MB. Machines missing from every catalog are not
  errors: the parser does not know catalogs.
- **Versions** (`support_files/version.rs`) are ordered by an integer key: major,
  minor, then beta (`0.37b5`), release candidate (`0.33rc1`), release, letter
  revision (`0.26a`, `0.69b`), update (`0.106u3`, `0.71u3p`), so `0.10` follows `0.9`.
- **Hierarchy.** Category values are split into levels on `/` and `: ` (as found in
  the data; no depth is assumed) and stored as node paths joined with `/`:
  `Arcade: Shooter / Flying Vertical` gives `Arcade`, `Arcade / Shooter` and
  `Arcade / Shooter / Flying Vertical`. Text without spaced separators
  (`Game Console/Computer`) is one level. Other dimensions are flat.

#### Global repository (`support_files/repository.rs`)

`support-files.sqlite` in the local data directory (several MB of imported data that
can be imported again; it does not roam). SQLite rather than JSON: about 230,000
rows, set-based comparisons and transactions, and it is attached by the projections.

- `sources` (one per format: file name, declared version, import date, machine count,
  warnings), `source_dimensions` (machines and values per dimension), `memberships`,
  `value_nodes` (node paths of every value, `leaf` for the value itself),
  `attributes` (with the version sort key), `active_sources` and
  `dimension_revisions`, plus a random `repository_id` created with the file.
- **Import** stages the parsed rows in temporary tables (each kind bound as one JSON
  array, read with `json_each`), compares them with the stored rows of the same
  format and dimension (`EXCEPT` both ways), replaces the source and updates the
  active sources and revisions, all in one immediate transaction. A parse failure
  never reaches the repository; a database failure rolls everything back, so the
  previous source stays available.
- **Source precedence.** One source per dimension is active. The first source
  imported for a dimension becomes active (shown in the import preview); afterwards
  the active source changes only by an explicit choice in Settings. Removing the
  active source leaves the dimension without source (`active_sources` references
  `source_dimensions`, so this is always explicit); an alternative is never activated
  implicitly, and inactive sources are kept. A reimport that no longer feeds a
  dimension (a `catver.ini` without `[VerAdded]`) clears it the same way. Data of two
  sources is never combined for one dimension.
- **Revisions.** Each dimension has a revision, incremented only when its effective
  content changes: activation, change or clearing of the active source, or a reimport
  of the active source with different rows. Reimporting identical data (another file
  name or date) changes no revision. The modification date of a file is never used.
- Support files never touch `lists.json`: classifications and Favorites share the
  idea of membership by short name, but not storage or lifecycle (imported and
  replaced by sources, versus edited by the user).

#### Catalog projections (`catalog/classification.rs`)

Each catalog has `classifications.sqlite` next to `catalog.sqlite`. It is derived
data (the catalog keeps only MAME data and is replaced by imports): `memberships`
(dimension, node, machine, `direct`), with one row per ancestor node so that a parent
selects its descendants by equality, `attributes` (dimension, machine, value, sort key)
and `projection_state` (per dimension: repository id, revision, active source, catalog
token, machine and value counts).

- **Synchronization at access.** Machine searches, machine details, the
  classification status and the missing-values check first call
  `CatalogStore::sync_classifications` with a snapshot of the repository revisions
  (kept in memory by `SupportFilesState` and refreshed by every change). Under the
  catalog store lock it compares, per dimension, the snapshot and the catalog token
  (`imported_at:machine_count` of `catalog_metadata`) with `projection_state`; when
  nothing differs it returns at once (0.3 ms). Otherwise it rebuilds only the
  differing dimensions.
- **Catalog reimport.** A new machine import changes the catalog token, so every
  dimension is rebuilt even though the repository did not change.
- **Rebuild.** A separate connection opens the projection, attaches the catalog and
  the repository read-only (`file:` URIs, percent-encoded) and runs, in one
  transaction, `DELETE` and `INSERT ... SELECT` per dimension: memberships joined
  with their nodes, restricted to `machine_name IN (SELECT name FROM
catalog.machines)` (written `+m.machine_name` so that SQLite checks each row instead
  of probing the primary key once per catalog machine and node, which took 37-120 s),
  then the counts and the new state. Machines missing from the catalog stay in the
  repository only. A dimension without source is emptied, so removed classifications
  leave no orphan rows. The state is written in the same transaction as the data: a
  failure (unreadable repository, full disk) keeps the previous projection and state,
  is reported by `get_classification_status` (`syncError`) and is retried at the next
  access; searches without classification filters are never blocked.
- The catalog's read connection attaches the projection read-only as `cls` when it is
  opened (creating an empty projection if needed); searches join it directly.
  Removing an installation's catalog removes its projection.

#### Interface

- **Settings › Support Files** (`SupportFilesSettings`): import with the native file
  picker (`preview_support_file_import`, then `apply_support_file_import` after the
  summary: type, declared version, machines, values per dimension, whether each will
  be active, warnings, and the source it replaces), the active source of each
  dimension, the imported sources (file, version, date, machines, values, active or
  not, warnings) with **Reimport…** (the same format only) and **Remove…** (confirmed;
  it says which dimensions lose their source; the file on disk is not touched).
- Catalog filters, Browse all, the Category tree and Version Added: see
  [Catalog browser](#catalog-browser--implemented).
- **Machine detail**: a **Classification** section in Overview; each value and each
  category level dispatches `showClassification`, which returns to the results with
  only that filter (text and other filters cleared, sort and view kept).
- Software details show no machine classifications: support files describe machines,
  and a software is not one machine.

#### Commands

| Command                              | Purpose                                                           |
| ------------------------------------ | ----------------------------------------------------------------- |
| `get_support_files`                  | Sources and active sources (`SupportFilesView`).                  |
| `preview_support_file_import`        | Picks and checks a file (optionally of an expected format).       |
| `apply_support_file_import`          | Stores a previewed file, atomically.                              |
| `remove_support_file`                | Removes a source from the repository.                             |
| `set_active_support_source`          | Sets or clears the active source of a dimension.                  |
| `get_classification_status`          | Synchronizes a catalog and returns its dimensions and sync error. |
| `find_missing_classification_values` | The values no machine of a catalog has.                           |

#### Measured performance

Release build, Windows, the support files of MAME 0.288 and copies of the 0.280
(48,893 machines) and 0.252 (45,379) catalogs (`real_catalog_classifications`, an
ignored test):

| Operation                                           | Time             |
| --------------------------------------------------- | ---------------- |
| Parse `catver.ini` / `genre.ini` / `series.ini`     | 59 / 21 / 5 ms   |
| Store `catver.ini` / `genre.ini` / `series.ini`     | 229 / 84 / 33 ms |
| Reimport identical `catver.ini`                     | 416 ms           |
| First projection of four dimensions (0.280 / 0.252) | 593 / 544 ms     |
| Access without changes                              | 0.3 ms           |
| Genre `Shooter` (3,412 results)                     | 2.3 ms           |
| Category node `Shooter` with descendants            | 2.3 ms           |
| Series `194X` (37 results)                          | 0.2 ms           |
| Version Added `0.200`-`0.280` (9,872 results)       | 12.0 ms          |
| Genre + category + series + text                    | 1.7 ms           |
| Four classification facets, no filter               | 99.6 ms          |

The facets of a dimension cost 5 ms (series) to 42 ms (version added) over the whole
catalog; they are requested with the other facets only when filters or text change.

#### Limitations

- Only the five formats above; no download, update check or scheduler.
- Category levels come from `/` and `: ` separators only; values are shown as
  the files write them otherwise.
- Version strings outside the MAME numbering are kept but cannot be ordered (no range).
- Facet value search folds ASCII case only, like the other facets.

### Saved filter presets — Implemented

`saved_filters.rs` and `commands/saved_filters.rs` (backend), `features/saved-filters/`
(frontend).

- **Model.** `saved-filters.json` (configuration directory, written atomically) holds
  `{ schemaVersion: 1, presets: [{ id, name, criteriaVersion, criteria, createdAt,
updatedAt }] }`. Names are unique (ignoring case), 1-100 characters; at most 500
  presets; criteria are a JSON object of at most 64 KB. The backend stores criteria
  verbatim; a file of another schema version, or invalid, is reported by every command
  and never replaced. Presets are global: they reference no installation, catalog row
  or file.
- **Criteria version 1** (`savedFilterCriteria.ts`): `{ scope, text, values, flags,
ranges, favoritesOnly }`: the catalog type, the search text and the filters of that
  type (facet values including classifications, single-choice filters, version
  ranges) and Favorites only. Presentation (renderer, sizes, panels, theme), sort and
  paging are not included. `encodeCriteria` normalizes them (sorted keys and values,
  empty entries dropped) so that equal criteria compare equal.
- **Loading** validates the criteria; a preset of another `criteriaVersion` is listed
  but cannot be loaded and is kept unchanged. Filters this version does not know are
  left out and named in a notice; nothing else is applied in their place. The criteria
  are applied in one reducer step (see [Catalog browser](#catalog-browser--implemented)).
- **Modified state.** The last loaded (or saved) preset is compared with the current
  criteria; a difference shows **Modified** and **Update “name”**, which replaces the
  preset's criteria. Presets are never overwritten implicitly.
- **Unresolvable criteria.** Values missing from the active catalog (a series another
  catalog has, a classification whose source was removed) stay applied and match
  nothing, so the results never widen silently; a banner lists them, with the reason
  when the dimension has no source.

| Command               | Purpose                                        |
| --------------------- | ---------------------------------------------- |
| `get_saved_filters`   | The presets.                                   |
| `create_saved_filter` | Adds a preset; returns its id and the presets. |
| `update_saved_filter` | Replaces the criteria of a preset.             |
| `rename_saved_filter` | Renames a preset.                              |
| `delete_saved_filter` | Deletes a preset.                              |

### SQLite — Implemented (catalog database)

- SQLite is embedded through `rusqlite` with the `bundled` feature, so every
  platform uses the same SQLite version, without system libraries.
- Explicit SQL, no ORM. Versioned migrations in `catalog/schema.rs`.
- Implemented: the catalog database (see [Catalog](#catalog--implemented-machines-and-software-lists)),
  including FTS5 full-text indexes (see [Catalog search](#catalog-search--implemented)).
- User data stays out of the catalog: Favorites are in `lists.json` (see
  [User lists](#user-lists-favorites--implemented)), never in a catalog database.
- `support-files.sqlite` (global repository) and `installations/<id>/classifications.sqlite`
  (projection of each catalog) have their own versioned migrations
  (`support_files/repository.rs`, `catalog/classification.rs`); neither is part of
  `catalog.sqlite`, which keeps only data derived from MAME.
- Planned: a separate user database (profiles, cache
  metadata) that survives catalog re-imports.

### Remote repositories — Implemented (connections, sources, SFTP provider)

Implemented in `src-tauri/src/remote/` and `commands/remote.rs`, typed in
`src/features/remote/`. This is the infrastructure of the ROM Manager: it
records where ROM, CHD and sample files can be retrieved from and where they will be
stored, and provides a protocol-independent repository contract with a first SFTP
provider, used by the [ROM Manager downloads](#rom-manager-downloads--implemented).

Three layers, independent of any protocol:

1. **Remote connections** (global): how to reach a repository.
2. **Remote sources** (per installation): which connection, remote path, collection
   organization and local destination each category of files uses.
3. **Repository providers**: the operations the ROM Manager performs, behind one
   contract.

#### Remote connections

`remote/connections.rs`, persisted in `remote-connections.json` (configuration
directory, `schemaVersion` 1, atomic writes, validated on load and save):

```json
{
  "schemaVersion": 1,
  "connections": [
    {
      "id": "6c1f…",
      "name": "Home NAS",
      "providerType": "sftp",
      "providerConfig": {
        "host": "nas.local",
        "port": 22,
        "username": "mame",
        "privateKeyPath": "C:\\Users\\me\\.ssh\\id_ed25519",
        "hostKeyVerification": "ymdb",
        "approvedHostKey": {
          "algorithm": "ssh-ed25519",
          "fingerprint": "SHA256:…",
          "publicKey": "ssh-ed25519 AAAA…",
          "approvedAt": 1760000000000
        }
      }
    }
  ]
}
```

- `id` is a random UUID (32 hexadecimal digits), stable: sources reference
  connections by id only, never by copying their settings. Names are unique.
- `providerType` and `providerConfig` are an adjacently tagged enum
  (`ProviderConfig`). A new provider is a new variant with its own settings;
  existing SFTP connections are never migrated or reinterpreted. An unknown provider
  makes the file invalid (for example after a downgrade).
- Only non-sensitive data is stored: the path of the private key (never the key),
  the approved server identity and the verification mode. No password exists and
  passphrases are never stored.
- An invalid file is reported and never replaced implicitly (sources reference it);
  changes are refused until it is fixed. A missing file means no connections.
- Removing a connection is refused (`connectionInUse`) while a source of a registered
  installation references it, even a disabled one; the error lists the installations
  and categories. Sources are never changed implicitly. An unreadable sources file
  also blocks the removal rather than risking an orphan.
- Editing the host or port forgets the approved identity (it belonged to the
  previous server).

#### Remote sources

`remote/sources.rs`, persisted per installation in
`installations/<id>/remote-sources.json` (configuration directory, next to
`launch.json`, `schemaVersion` 1):

```json
{
  "schemaVersion": 1,
  "categories": {
    "machineRoms": {
      "enabled": true,
      "connectionId": "6c1f…",
      "remoteBasePath": "/volume1/mame/0280/roms",
      "organization": "merged",
      "destination": { "mode": "mameDirectory", "directory": "roms" }
    },
    "samples": {
      "enabled": true,
      "connectionId": "6c1f…",
      "remoteBasePath": "/volume1/mame/samples",
      "organization": "zipArchives",
      "destination": { "mode": "custom", "directory": "E:\\MAME\\samples" }
    }
  }
}
```

- Categories: `machineRoms`, `machineChds`, `softwareListRoms`, `softwareListChds`,
  `samples`. Each is optional and independent; a missing or disabled category never
  affects the others. Enabled categories must be complete; disabled ones may keep
  incomplete values, which are still validated. Empty categories are not written.
- `organization`: `merged`, `split` or `nonMerged` for ROM and CHD categories;
  `zipArchives` or `directories` for samples. For CHDs the same names say where the
  disks of clones are stored (with the parent, in each clone's directory, or both);
  the exact path rules for CHDs are not those of ZIP archives and belong to the ROM
  Manager's file resolution.
- The remote side and the local side are independent: several categories may use
  different connections and one local directory, or the opposite. No file naming
  rule is defined here.
- `destination`: `mameDirectory` names an entry of the installation's search path
  as written (relative entries are relative to the MAME directory); `custom` is an
  absolute directory chosen with the native picker. Both must be valid MAME search
  path entries (no `;`, no leading `-`). YMDB never assumes the first MAME directory
  is writable: the user chooses.
- The directories offered are the installation's global launch option (`rompath`,
  `samplepath`) when set, otherwise the values read from `mame.ini`
  (`mame/ini.rs`: the first `mame.ini` in MAME's default `inipath` for the platform,
  read only, never run, never written), otherwise MAME's defaults (`roms`,
  `samples`).
- Two installations may use the same local directory. The view warns about it
  (same path by the platform's rules), since files with the same name may differ
  between versions. Nothing is deduplicated or overwritten.
- The view also warns about references to removed connections and custom
  directories that are not available.
- Removing an installation with "delete launch options and ROM Manager settings"
  deletes its `remote-sources.json` too; connections stay.

#### Provider contract

`remote/provider.rs` defines `RemoteRepository`, the contract the ROM Manager uses,
and `Repository`, an enum with one variant per provider; `provider::connect` opens
the repository of a connection with the provider it names. The ROM Manager never
names a protocol.

| Operation                     | Meaning                                                             |
| ----------------------------- | ------------------------------------------------------------------- |
| `capabilities()`              | Optional operations available (`listDirectory`, `rangedDownload`)   |
| `stat(path)`                  | Kind, size and modification time; `notFound` when missing           |
| `exists(path)`                | `stat` without the `notFound` error                                 |
| `list_directory(path)`        | Entries sorted by name; `unsupported` when the provider cannot list |
| `open_download(path, offset)` | A stream of the file from `offset` (`Download`, an `AsyncRead`)     |
| `close()`                     | Ends the connection                                                 |

- Capabilities are declared, not emulated: a provider that cannot list (a plain web
  server, a manifest-based repository) or resume reports it, and the operation fails
  with `unsupported` instead of being approximated.
- Downloads are streams: no API loads a whole file in memory. `remote/transfer.rs`
  copies a `Download` into any `AsyncWrite` chunk by chunk (256 KiB), reports the
  bytes transferred, stops on a cancellation flag (`cancelled`) and detects truncated
  streams (`connectionLost`). Temporary files, verification and finalization belong
  to the ROM Manager.
- Errors (`RemoteError`, tagged with `kind`): `hostUnreachable`, `timedOut`,
  `hostKeyUnknown`, `hostKeyChanged`, `privateKeyUnavailable`, `privateKeyInvalid`,
  `passphraseRequired`, `passphraseIncorrect`, `authenticationFailed`,
  `permissionDenied`, `notFound`, `notADirectory`, `unsupported`, `connectionLost`,
  `cancelled`, `localIo`, `protocol`. Messages are short and never contain
  credentials. A failed connection test is a result (`ConnectionTestResult`), other
  commands fail with `CommandError::Remote`.
- Paths (`remote/path.rs`) follow the POSIX semantics of the remote side and never
  use `PathBuf`. `RemotePath` is absolute and normalized (repeated `/` and `.`
  removed; `..`, `\` and control characters refused). `RelativePath` is a path below
  a base, made of plain segments: `RemotePath::join` therefore never leaves the
  configured base directory. Connection identity, base path and relative object path
  are kept separate. Symbolic links on the server are resolved by the server.

#### SFTP provider

`remote/sftp.rs`, built on [`russh`](https://crates.io/crates/russh) 0.64 and
[`russh-sftp`](https://crates.io/crates/russh-sftp) 3.0 (Apache-2.0):

- Chosen because they are pure Rust, maintained and asynchronous (the Tauri runtime
  is Tokio): no OpenSSL, libssh2, system `ssh`/`sftp` program or SSH agent is
  needed, so the desktop bundles have no new system dependency. The `ring` crypto
  backend is used (built with the C compiler already required by the bundled
  SQLite); the default `aws-lc-rs` backend would need CMake and NASM on Windows.
  `libssh2` (crate `ssh2`) was rejected: native library, and no ed25519 keys with
  its default Windows backend.
- Keys: OpenSSH (ed25519, ECDSA, RSA, encrypted with bcrypt-pbkdf), PEM PKCS#1 and
  PKCS#8 and PuTTY keys. For RSA keys the best signature hash the server supports is
  used. The key file is read (at most 64 KiB) only after the server identity was
  verified, so a key is never used with an unverified server.
- A connection performs, in order, with time limits (15 s for connection steps,
  30 s per SFTP request, SSH keepalive every 20 s): name resolution, TCP connection
  (`hostUnreachable`, `timedOut`), SSH handshake, server identity verification,
  public key authentication (`authenticationFailed`) and the SFTP subsystem
  (`protocol` when unavailable). Only SFTP requests are sent: no remote command is
  ever executed. Writes are never requested.
- Server identity, always verified (`verify_host_key`):
  - `ymdb` mode: the key must match the identity approved in YMDB; without one the
    connection stops with `hostKeyUnknown` (algorithm and `SHA256:` fingerprint).
  - `knownHosts` mode: the user's `~/.ssh/known_hosts` is read (hashed host names
    and `[host]:port` entries supported, never written); a matching key is trusted,
    a different key of the same type is `hostKeyChanged` (source `knownHosts`); hosts
    it does not list fall back to the YMDB approval.
  - A different approved key is `hostKeyChanged` (source `ymdb`), and nothing is
    approved automatically. The user can forget the approved identity explicitly.
  - Approval: the test command keeps the identity the server presented in memory;
    `approve_remote_host_key` only accepts that exact fingerprint for the same host
    and port, then stores the public key. The frontend cannot approve a key the
    server never presented.
  - Host certificates are not supported.
- The SFTP session keeps its connection alive while a download stream is open.

#### Commands

- `get_remote_connections`, `save_remote_connection(connection)` (create without
  `id`, or update), `remove_remote_connection(connectionId)`.
- `test_remote_connection(connectionId, passphrase)`: connects for real and
  disconnects; returns `success` (start directory, capabilities) or `failure`.
- `approve_remote_host_key(connectionId, fingerprint)`,
  `forget_remote_host_key(connectionId)`.
- `browse_remote_directory(connectionId, path, passphrase)`: one listing (at most
  2,000 entries), read only; an empty path lists the start directory.
- `pick_private_key_file`, `pick_destination_directory`: native pickers.
- `get_remote_sources(installationId)`, `save_remote_sources(installationId, config)`:
  the sources of one installation with the connections, the MAME directories and the
  warnings. Saving checks that every referenced connection exists, under the same lock
  as connection changes.

Network commands are explicit user actions run on the async runtime; they never hold
the configuration lock. Passphrases travel with one command and are dropped.

#### Interface

- **Settings → Remote Connections**: the connections with their server, key file
  (marked when missing) and approved identity; add and edit (name, host, port,
  username, key file with the native picker, host key verification; fields checked
  before saving), remove (refused with the list of references), **Test Connection**
  (progress, success, errors), the approval of an unknown identity (fingerprint and
  key type, explicit approval, then the test runs again), a changed identity (both
  fingerprints, no approval, an explicit "forget" for YMDB-approved identities), the
  passphrase prompt (sent once, never kept) and **Browse…** (read-only directory
  listing; a passphrase entered there stays in memory while it is open). Only SFTP is
  offered; no control suggests other protocols.
- **MAME page → ROM Manager** of an installation: its five categories with
  enabled, connection (name and provider type), remote base path, organization and
  local destination (a MAME directory with its origin and status, or a custom
  directory), category warnings, and save/discard. Incomplete enabled categories are
  reported before saving.

#### Launcher integration

See [Search paths of the ROM Manager](#search-paths-of-the-rom-manager).

#### Future HTTPS provider

SFTP is the first provider, not the ROM Manager's protocol. An HTTPS provider will be a
new `ProviderConfig` variant and a new `Repository` variant implementing
`RemoteRepository`:

- it may declare `listDirectory: false` (deterministic URLs or a manifest) and
  `rangedDownload` according to HTTP `Range` support; the contract already reports
  unsupported operations instead of emulating them;
- its authentication (HTTP Basic over TLS, bearer tokens, API keys, signed URLs or
  OAuth 2.0 with PKCE) is not decided and lives in its own settings; nothing in the
  model assumes browser cookies or a particular proxy. Secrets will need their own
  storage decision (the current files hold no secret);
- remote paths are POSIX-like and map to URL paths.

No HTTPS or S3 code, placeholder or interface control exists yet.

#### Verified

- Automated, against an in-process SSH/SFTP server (`remote/test_server.rs`,
  `russh` server with fixture keys, serving a temporary directory on `127.0.0.1`):
  connection through the provider contract, stat, existence, listing, not found,
  not a directory, permission denied, streaming of a 3 MiB file with progress, ranged
  download, interrupted transfer (server disconnects: `connectionLost`),
  cancellation, RSA and passphrase-protected keys (required, incorrect, correct),
  unknown and changed identities, `known_hosts` matching and mismatch, refused key,
  missing and invalid key files, unreachable host, handshake timeout, missing SFTP
  subsystem, and the approval flow of the commands.
- Not verified: a real NAS or OpenSSH server over a network, and macOS/Linux
  connections (CI builds and runs the same tests there).

#### Limitations

- Passphrases are asked again for every connection (no credential manager, no SSH
  agent support).
- Only private key authentication; no keyboard-interactive or password login; no SSH
  certificates, jump hosts or `~/.ssh/config`.
- Changed host identities from `known_hosts` must be fixed in that file.
- Directory listings in the interface are limited to 2,000 entries.
- RSA signatures use the `rsa` crate, which has a known timing side channel
  (RUSTSEC-2023-0071) for private key operations; prefer ed25519 keys.
- Downloads are described in [ROM Manager downloads](#rom-manager-downloads--implemented).

### ROM Manager downloads — Implemented

Implemented in `src-tauri/src/roms/` and `commands/roms.rs`, typed in
`src/features/roms/`. Built on the
[remote repositories](#remote-repositories--implemented-connections-sources-sftp-provider)
infrastructure without changing it: the ROM Manager only uses `provider::RemoteRepository`
(through the small `roms::Connector` trait, whose real implementation calls
`provider::connect`) and `transfer::copy_download`; it never names SFTP.

#### Principle: whole sets, conservatively

For a launch, the ROM Manager identifies the **sets** that may be needed, downloads every
candidate file that is missing locally and available remotely, reports the others as
warnings and lets MAME decide whether the files present are enough. It never opens
archives, never reads or hashes individual ROMs, never rebuilds merged or split sets and
never deduplicates between plans or MAME versions (no content-addressed cache).
Redundancy is accepted: downloading an archive that was not strictly needed is
preferred to analysing archive contents. Only identical files within one plan are
deduplicated (by category and path).

#### Catalog metadata

No schema change was needed: the catalog already stores `cloneof`, `romof`, `sampleof`,
device references, ROMs with `merge` and `status`, disks with `merge` and `status`,
machine samples, software `cloneof`, parts, data areas and disk areas. Existing catalogs
work as they are; no reimport is required.

#### Set-based planner (`roms/planner.rs`)

Input: the machine, and for a software its list, name and mounted parts (from the
launch request), plus the organizations of the Machine CHDs and Samples sources (they
change which paths exist). Output: ordered `Candidate`s (category, set, reason, path
segments, and the organizations in which the file may legitimately be absent).

- **Machine chain**: the machine, then every set reached through `romof` and `cloneof`
  (indirect parents included, at most 16 sets); a reached set flagged `isbios` is a
  BIOS (`bios`), otherwise a parent (`parent`).
- **Machine ROMs**: `<set>.zip` for each set of the chain with at least one dumped ROM
  (status other than `nodump`), then for each device with dumped ROMs reached through
  `device_ref` from the machine, recursively (`device`). Absence notes: a clone is
  stored in its parent's archive in merged collections; a set whose dumped ROMs are all
  merged may have no archive in merged and split collections; a parent is included in
  every clone of a non-merged collection.
- **Machine CHDs**: `<set>/<disk>.chd` for the dumped disks of each set of the chain.
  In merged and split collections a disk with a `merge` attribute lives in the
  directory of the set it merges from, which the chain already covers, so it is
  skipped; in non-merged collections it is also looked for in the clone's directory.
  Disk names come from the catalog, never from the machine name.
- **Samples** (machine only, no parent/clone rule): when the machine lists samples,
  the sample set is `sampleof` (else the machine): `<set>.zip` for ZIP collections,
  `<set>/<sample>.wav` per sample for directory collections.
- **Software**: the software and its `cloneof` parents in the same list
  (`software`, `softwareParent`): `<list>/<software>.zip` when the software has dumped
  ROMs; `<list>/<software>/<disk>.chd` for the disks of the mounted parts (every part
  when none is given) and, for parents, of their parts with the same names. A clone's
  disk identical to a parent's (same name and SHA-1) may be absent in merged and split
  collections. The machine running the software is then planned with reason `system`.
- Requirements expressed only in free-form software features (such as
  `requirement`) are not followed: they are not represented reliably.

| Reason                       | Meaning                                   |
| ---------------------------- | ----------------------------------------- |
| `selected`                   | the machine launched on its own           |
| `system`                     | the machine running the selected software |
| `parent`, `bios`             | sets reached through `romof`/`cloneof`    |
| `device`                     | a device with ROMs                        |
| `software`, `softwareParent` | the software and its parents              |
| `sample`                     | the sample set                            |

#### Local availability (`roms/local.rs`)

Files are looked for in the directories MAME searches **for this launch**: the
resolved `rompath`/`samplepath` of the launch plan (global options, profile and
overrides by precedence, already composed with the ROM Manager destinations by
`launcher/search_paths.rs`), else MAME's own value from `mame.ini`. Relative entries are
relative to the MAME directory. The check is shallow: the file exists, is a regular
non-empty file and starts with the signature of its format (ZIP, 7-Zip, CHD, WAV,
FLAC); `.7z` archives and `.flac` samples are accepted as MAME accepts them. A file
that looks wrong is still `present`, with an informational warning (it is never
replaced). No collection is scanned and no hash is computed.

Names from the catalog must be portable file names (no reserved characters, control
characters, leading dots, trailing dots or spaces, or reserved Windows device names) and
valid `RelativePath` segments; otherwise the file is `unresolved`. The destination is
`<category destination>/<relative path>`.

#### Remote availability (`roms/availability.rs`, `roms/plan.rs`)

The missing files of enabled sources are looked up with one `stat` per exact path,
one connection per repository; repositories are never listed, so providers without
listings (a future HTTPS provider) work the same way. `notFound` makes a file
`remoteMissing`; any other error (permission denied, timeout, lost connection,
authentication, unapproved or changed identity, unsupported) makes it `unresolved` with
the error. After a connection-level failure (timeout, lost connection, unreachable
host) the remaining files of that repository are unresolved without further waiting. A
connection that cannot be opened is reported once (`connectionIssues`); an unknown
server identity is remembered so that the existing approval command
(`approve_remote_host_key`) can approve exactly the fingerprint presented.

#### The plan

| Availability      | Meaning                                                       |
| ----------------- | ------------------------------------------------------------- |
| `present`         | found where MAME searches (says nothing about its content)    |
| `missing`         | not found locally, no usable source to look it up             |
| `remoteAvailable` | not found locally, found remotely: downloaded                 |
| `remoteMissing`   | not found locally nor remotely: warning                       |
| `unresolved`      | the remote lookup failed or the path cannot be built: warning |

Warnings are typed: `availability` (not in the repository), `informational` (an
expected absence, or a remark about a local file), `configuration` (no enabled source,
removed connection, unreadable settings), `unresolved` (technical failure) and
`conflict`. When two different files of a plan would be written to the same local path
(for example samples and ROMs downloaded into one directory, both named `galaga.zip`),
neither is downloaded. The summary counts present files, files to download (with the
known total size), files not found, unresolved files and non-informational warnings.

A plan is a snapshot: it records the installation, the launch request, the remote
paths, the destinations and the connections it was built with, and is kept in memory
(the 16 most recent). Downloading a plan uses exactly that snapshot, whatever the
active installation becomes; it is refused when one of its connections changed since.

#### Download coordinator (`roms/download.rs`)

- One plan downloads at a time (`downloadInProgress` otherwise) and one file at a
  time, on a dedicated thread with its own Tokio runtime; it does not depend on any
  React component. The frontend polls `get_download_status` (in memory).
- Each file is streamed (256 KiB chunks, never in memory) into
  `<destination>.ymdb-download` in the destination directory (same file system), then
  flushed and synced. The bytes received must equal the size announced by the
  repository (else the size of the plan); otherwise the file is discarded
  (`sizeMismatch`).
- Finalization never replaces a file: the final name is created with a hard link,
  which fails if the name exists, then the temporary file is removed. On file systems
  without hard links the temporary file is renamed after checking that the destination
  is still free (a small race window remains). A destination that appears meanwhile is
  kept and the transfer is `skipped`.
- Cancellation stops the current transfer within 100 ms (also while connecting or
  waiting for data), removes its temporary file and the directories created for it, and
  marks the remaining files `cancelled`; completed files are kept.
- A connection that cannot be opened fails its remaining files; after a broken
  connection the next file reconnects. Errors are typed (`remote` with the
  `RemoteError`, `sizeMismatch`, `local`).
- Transfer statuses: `pending`, `downloading`, `downloaded` (transferred, size checked,
  finalized; not verified against MAME), `skipped`, `failed`, `cancelled`. A retry of
  the same plan keeps the files already downloaded. Resuming partial files is not
  implemented: the temporary file has a fixed name and is overwritten by the next
  attempt (also after YMDB was closed during a transfer; no automatic resume).
- Job outcome: `completed`, `completedWithWarnings` (the plan had non-informational
  warnings), `failed` (a transfer failed) or `cancelled`.

#### Download & Play

`start_download(planId, passphrases, play: true)` downloads the plan, then, only when
no transfer failed and nothing was cancelled, starts MAME on the download thread
through the launcher (`launch_blocking`: `prepare_resolved` and the session manager)
with the plan's launch request, so the same installation, profile and options are used.
Warnings never prevent it. The outcome (`started` with the session id, `notStarted`,
`failed` with the command error) is part of the job status. When nothing needs
downloading, the interface launches the plan's request directly. The normal Play
command is unchanged and never consults the ROM Manager.

#### Commands

- `prepare_download_plan(request, passphrases)`: validates the request as a launch
  would (catalog freshness included), builds and stores the plan. Nothing is downloaded.
- `start_download(planId, passphrases, play)`, `get_download_status()`,
  `cancel_download()`.

Passphrases (by connection id) travel with one command and are dropped with the
operation; they are never stored or logged.

#### Interface

The launch bar of the machine and software detail views has **Prepare Download** and
**Download & Play** (icon buttons with tooltips), which open a modal window. The plan shows the summary, connection issues (passphrase prompt,
approval of an unknown identity, changed identity), plan warnings and a table of files
(path, category, reason, status, size, local path or destination, warning, and the
transfer status once downloading). Progress shows the current file, bytes and overall
percentage, with **Cancel download**; the result shows the outcome, failed files with
their errors, **Retry** and the Download & Play outcome. A plan is shown only for the
installation and launch request it was built for; a download keeps running when the
page or the installation changes or the window is closed (closing hides it, never
cancels), and a button in the launch bar shows it again when its target is displayed
(or a note says that another download is running).

#### Future HTTPS providers

Nothing in the planner, the plan or the coordinator depends on SFTP: a provider only
needs `stat` and `open_download` on exact paths. Directory listings and ranged
downloads are not used, so a provider declaring neither works unchanged.

#### Verified

- Automated: planner on synthetic catalogs (standalone machine, devices and devices of
  devices, clones in every organization, all-merged clones, indirect parent and BIOS,
  machine disks with merge in merged/split/non-merged, samples as ZIP and directories,
  software clones, mounted parts, software disks identical to the parent's, undumped
  software, unknown targets); plans with fake repositories (present files, missing
  sources, removed connections, availability and informational warnings, permission
  and passphrase failures, timeouts, destination conflicts, shallow local checks,
  unusable names); downloads with fake repositories (temporary files and finalization,
  5 MiB file, existing destination kept, broken stream, size mismatch, unwritable
  destination, missing remote file, refused connection, cancellation of a stalled
  transfer, retry with a stale temporary file); through the in-process SFTP server
  (plan, download of ROMs and a 3 MiB CHD, remote files unchanged, files present for a
  new plan, interrupted transfer without a partial file); command-level jobs (Download &
  Play with the plan's request, no launch after a failure, retry, one download at a
  time, cancellation, serialization); interface tests of the plan, progress,
  cancellation, retry, outcomes, passphrase and identity prompts, installation changes
  and the unchanged Play button.
- End-to-end (ignored test `rom_manager_end_to_end`, run on Windows with MAME 0.280):
  the real `-listxml` catalog (48,893 machines) imported in a temporary directory,
  plans for `pacman`, `kof98a` (parent and `neogeo` BIOS) and `kinst` (disk), a
  synthetic repository served over SFTP without the parent (availability warning),
  download and `-rompath` composition of the prepared launch. MAME was not started.
- Not verified: a real NAS, macOS/Linux runs (CI runs the same tests), and a game
  started by Download & Play in the application window.

#### Limitations

- No audit: present and downloaded files are not checked against MAME's ROM data.
- Only `.zip` archives and `.chd`/`.wav` files are downloaded (local `.7z` and `.flac`
  are recognized as present).
- Software requirements stated only in free-form features are not followed.
- No resume of partial transfers, no parallel downloads, no persistence of plans or
  downloads across restarts.
- On file systems without hard links, finalization checks then renames (small race).

## Cross-platform considerations

- Shared modules must not contain platform-specific behavior. When unavoidable, it
  is isolated with `cfg` attributes in dedicated, minimal functions.
- File system paths are always handled with `std::path` types, never string
  concatenation.
- MAME executable names and default locations differ per platform and are resolved
  in `src-tauri/src/mame/platform.rs` only.
- Cross-platform compatibility is enforced by continuous integration
  (`.github/workflows/ci.yml`), which runs all checks and compiles the application
  on Windows, macOS and Linux for every change.

## Build and quality infrastructure — Implemented

- Toolchains are pinned: Node.js in `.node-version`, pnpm in the `packageManager`
  field of `package.json`, Rust in `rust-toolchain.toml`.
- Dependency versions are pinned exactly and locked by `pnpm-lock.yaml` and
  `src-tauri/Cargo.lock`.
- `pnpm check` is the single entry point for all quality checks (TypeScript,
  ESLint, Prettier, Vitest, rustfmt, Clippy, cargo test), used both locally and in CI.
- CI compiles the full application (`pnpm tauri build --debug --no-bundle`) on each
  platform. Release packaging and publishing are not automated yet.
