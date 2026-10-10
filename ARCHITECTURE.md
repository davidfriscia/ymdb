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
│  │ MAME          │  │ Catalog      │  │ ROM cache         │  │
│  │ integration   │  │              │  │                   │  │
│  └───────┬───────┘  └──────┬───────┘  └─────────┬─────────┘  │
│          │                 │                    │            │
│          │          ┌──────▼───────┐  ┌─────────▼─────────┐  │
│          │          │ SQLite       │  │ Storage providers │  │
│          │          │              │  │ (local / remote)  │  │
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
- Implemented features:
  - `mame-installation`: the MAME Installation panel (see below).
  - `catalog`: the MAME Catalog panel (catalog summary, import, progress, cancel).
    It notifies the shell when an import replaced the catalog.
  - `search`: typed wrappers of the search commands (`searchApi.ts`).
  - `browser`: the catalog browser and detail views (see
    [Catalog browser](#catalog-browser--implemented)).
  - `media`: media sources in the Settings page and `MediaProvider`, which shares the
    sources and their version with the Media Grid (see
    [Media sources](#media-sources--implemented)).
  - `launcher`: launch panels in the machine and software details, the launch options
    section of the Settings page and `LauncherProvider`, which shares the launch
    configuration and the session statuses (see [Launcher](#launcher--implemented)).
  - `health-check`: calls `check_health` to verify frontend-backend communication.

#### Application shell

`App.tsx` shows a navigation rail with three pages: **Catalog** (the browser, the
start page), **MAME** (MAME Installation and MAME Catalog panels) and **Settings**
(theme, version, backend connection check). Pages stay mounted and are hidden with
the `hidden` attribute, so the browser keeps its query, results and scroll position
while the user visits another page. There is no router. Below 640 px the rail
becomes a top bar.

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
  `--shadow-elevated`). Stylesheets use only these tokens.
- `src/app/theme/themes.css` defines the complete token set of each theme under
  `:root[data-theme="<id>"]`; `themes.ts` registers the themes (`light`, `dark`) and
  `applyTheme` sets `data-theme` on `<html>`. Light is the default and also applies
  before any `data-theme` is set. The operating system preference is not followed.
- Adding a theme means defining its tokens in `themes.css` and registering it in
  `THEMES`; the Settings page lists registered themes automatically.
- `main.tsx` applies the saved theme before the first render to avoid a flash.

#### Interface preferences

`src/app/preferences.ts` stores the interface preferences as JSON in the webview's
`localStorage` under the versioned key `ymdb.preferences.v2`: theme, visibility of
the Filters and View Options panels, selected renderer, the options of every
renderer (kept separately, so switching renderer keeps each one's sizes and
settings) and the command line preview. They are separate from `installations.json`
(MAME installations and the active one), the launch configurations, `media.json`
(media sources) and the catalogs, and are the same for every MAME installation.
Every field is validated when read: unknown themes or renderers, wrong types and
malformed JSON fall back to the default of that field, and renderer options are
normalized against the renderer definitions (ranges clamped to their bounds and
steps, unknown choices replaced). Version 1 data (`ymdb.preferences.v1`: one card
size and the `cards` renderer) is migrated when there is no version 2 yet, and
removed at the next save. Unavailable storage only means the preferences last for
the session.

Planned: collections, input devices.

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
  backend), a value search on large facets (debounced) and "Show more" up to 200
  values. Selected values missing from the returned values stay listed, with "—"
  as count. Active filters are shown as removable chips with "Clear all filters";
  the Filters button shows their number. Declared ROMs/CHDs are what MAME lists,
  not files on disk.
- **Layout.** Three areas: the Filters panel (left), the results (center) and the
  View Options panel (right). The **Filters** and **View** toolbar buttons
  (`aria-pressed`) show or hide each panel independently; the body grid only has
  columns for visible panels, so the results take the freed width. Hiding a panel
  keeps filters, search, page and scroll, and never searches again. A new search,
  filter, sort or page scrolls the results back to the top. Below 1100 px
  both panels narrow; below 800 px they stack above the results.
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
- **Pagination.** Page numbers around the current page, previous/next, page size
  (25, 50, 100, 200) and "Go to".
- **Details.** Opening a result pushes a detail entry on the stack; links inside a
  detail push further entries. "Back" pops one entry, "Back to results" clears the
  stack. Results stay mounted (hidden) so the query and results are unchanged; the
  results scroll position is saved when a detail opens and restored on return.
  Previous/Next move through the items of the current result page. Each detail view
  has a side tab list (top tabs below 800 px) and keeps the selected tab when moving
  to the previous or next result.
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
  `media.rs`). Commands extract what they need from the Tauri runtime and delegate
  to plain functions that are unit-tested without Tauri. Blocking work (process
  execution, native dialogs, searches) runs on the blocking thread pool, and
  long-running work (catalog import) on a dedicated thread, so the UI never freezes.
  Failures are returned as a structured `CommandError` (`settings`,
  `installationNotFound`, `duplicateInstallation`, `removalNotAllowed`,
  `migrationFailed`, `importInProgress`, `invalidRequest`, `catalogNotImported`,
  `catalogUnreadable`, `softwareListsNotImported`, `catalogMismatch`, `notFound`,
  `alreadyRunning`, `launchFailed`, `internal`). A problem with one installation
  (missing executable, unreadable catalog) is reported for that installation only.
- `health.rs` builds the health report returned by `check_health`.
- `atomic_file.rs` writes small files atomically (`installations.json`, `launch.json`,
  `media.json`).
- `installations/` manages the registered MAME installations, the active one and the
  migration from the single-installation layout (see
  [MAME installations](#mame-installations--implemented)).
- `settings.rs` reads the `settings.json` of the single-installation layout, only for
  the migration.
- `mame/` implements the MAME integration (see below).
- `catalog/` implements the machine and software list catalog (see [Catalog](#catalog--implemented-machines-and-software-lists)).
- `launcher/` builds and runs MAME command lines (see [Launcher](#launcher--implemented)).
- `media/` finds and serves local artwork (see [Media sources](#media-sources--implemented)).

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
- `platform.rs`: all OS-specific details (executable names, search directories,
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
another one (MAME page or the **MAME Version** menu of the catalog's View Options)
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
**Settings → MAME launch options** chooses the installation to configure (the active
one by default).

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
configuration only when requested (default: no); kept directories are reported. Files
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

- Machine detail: a launch panel with **Play**, the profile, **Command line**
  (the backend preview: executable, working directory, each argument with its role
  or the level its option comes from, and the full command line), **Options for
  this machine** (the machine override) and the status of the last session (running
  with Stop and a confirmation, finished, failed with MAME's exit code, message and
  output, or stopped). Devices and other non-runnable entries have no Play button.
- Software detail: the same panel with a machine selector listing the candidates
  with their relation, filter result and whether they can mount the software
  (machines that cannot are disabled), a note on the limits of the catalog data,
  the media assignment (one selector per compatible device) and **Options for this
  software**.
- Settings: the installation to configure, then its global options, profiles
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

| Record   | Path in a source directory                            | Origin of the convention                                    |
| -------- | ----------------------------------------------------- | ----------------------------------------------------------- |
| Machine  | `<machine>.<ext>`, e.g. `snap/pacman.png`             | artwork collections, one folder per category                |
| Machine  | `<machine>/0000.png`                                  | first screenshot saved by MAME (`snapname` default `%g/%i`) |
| Software | `<list>/<software>.<ext>`, e.g. `snap_SL/nes/smb.png` | software list artwork packs and MAME's internal interface   |

- Extensions, in order: `png`, `jpg`, `jpeg`, `webp` (displayed by WebView2,
  WebKitGTK and WKWebView). Other files (`bmp`, `gif`, archives) are ignored.
- Directories are searched in order, then extensions; the first existing file wins.
  The rule is deterministic: an earlier directory wins even with a less preferred
  format.
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
- Requests contain identifiers only: `<category>/machine/<machine>` or
  `<category>/software/<list>/<software>` (`mediaUrl` in `mediaApi.ts`, built with
  `convertFileSrc`, plus `?v=<version>` that changes when the sources are saved).
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

The **Catalog of …** panel of the MAME page, for the installation selected there,
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
  media devices with extensions, software lists, RAM options and samples. Parent
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

#### Facets

Requested facets return `(value, count)` pairs: machines by manufacturer, year,
driver status, emulation status and software list; software by list, publisher,
year, support status and interface.

- Counts use disjunctive semantics: each facet applies the search text and all
  filters except its own. Selecting a manufacturer therefore keeps the other
  manufacturers visible with their counts (to add alternatives), while the other
  facets count only the matching results.
- Machines referencing a list, and software with several parts on one interface,
  are counted once per value.
- Years are listed chronologically; other facets by decreasing count, then value.
  `null` values come last.
- Each facet returns at most `limit` values (default 50, at most 200) with
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
- `media.json`: the media sources, shared by all installations (see
  [Media sources](#media-sources--implemented)).
- `settings.json` and `launch.json`: files of the single-installation layout. They are
  only read by the migration and never written again, so they remain as a backup.

Local data directory:

- `installations/<id>/catalog.sqlite`: the catalog of each installation (see
  [Catalog](#catalog--implemented-machines-and-software-lists)).
- `launch-logs/`: the output of MAME sessions (the 20 most recent are kept).

All JSON files are written atomically (a temporary file in the same directory is
written, synced and renamed over the target) and validated when read. Access is
serialized by mutexes held in Tauri managed state. `YMDB_DATA_DIR` replaces both
directories with one directory (`config/` and `data/`), for isolated test runs.

Implemented: interface preferences (theme, panels, result renderer and options,
command line preview) in the webview `localStorage` (see
[Interface preferences](#interface-preferences)). They live in the webview data
directory and are shared by all MAME installations.

Planned: user-configurable ROM, CHD and artwork directories.

### SQLite — Implemented (catalog database)

- SQLite is embedded through `rusqlite` with the `bundled` feature, so every
  platform uses the same SQLite version, without system libraries.
- Explicit SQL, no ORM. Versioned migrations in `catalog/schema.rs`.
- Implemented: the catalog database (see [Catalog](#catalog--implemented-machines-and-software-lists)),
  including FTS5 full-text indexes (see [Catalog search](#catalog-search--implemented)).
- Planned: a separate user database (favorites, collections, profiles, cache
  metadata) that survives catalog re-imports.

### Remote storage provider abstraction — Planned

- A small storage provider interface (list, check existence, download) with
  initial HTTPS and SFTP implementations.
- Providers are optional and configured explicitly by the user. The application must
  work normally with no provider configured and no network connection.
- Additional providers can be added later behind the same interface.

### ROM cache — Planned

- Configurable local cache (location and size limits) for files obtained from remote
  providers.
- Resolves required files for a launch: local directories first, then cache, then
  configured remote providers (when available).
- Integrity verification of cached files.

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
