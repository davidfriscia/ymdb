# Architecture overview

This page summarizes the implemented architecture of YMDB. The authoritative,
detailed reference, with data formats, rules and verification notes for every
component, is [ARCHITECTURE.md](../ARCHITECTURE.md) at the repository root.

## Layers

```text
┌──────────────────────────────────────────────────────────────────┐
│ Desktop UI: React + TypeScript in the system web view            │
│ src/app (shell), src/features/<feature>, src/shared (IPC, hooks) │
└───────────────┬───────────────────────────────┬──────────────────┘
                │ Tauri IPC commands            │ ymdb-media protocol
┌───────────────▼───────────────────────────────▼──────────────────┐
│ Application core: Rust (src-tauri/src)                           │
│  installations  catalog (+search, detail)  launcher  controllers │
│  media  remote (SFTP)  roms (ROM Manager)  lists  support_files  │
│  saved_filters  app_data  window_state                           │
└──────┬─────────────────┬───────────────────┬─────────────────────┘
       │ process         │ files             │ SSH / SFTP (optional)
┌──────▼──────┐  ┌───────▼─────────────────┐ ┌▼────────────────────┐
│ Local MAME  │  │ SQLite databases, JSON  │ │ Personal remote     │
│ (1 or more) │  │ files, media folders    │ │ repository          │
└─────────────┘  └─────────────────────────┘ └─────────────────────┘
```

Principles: cross-platform first (operating system details are isolated in
`mame/platform.rs` and a few `cfg` functions), offline first (remote repositories are
optional), a thin frontend over a rich Rust core, and no copyrighted content shipped.

## Tauri and the frontend

- `src-tauri/src/lib.rs` builds the Tauri application, its managed state and
  commands. `src-tauri/src/commands/<area>.rs` are thin adapters that delegate to
  plain, unit-tested functions; failures are a structured `CommandError`. Blocking
  work runs on the blocking thread pool; long jobs (catalog import, ROM downloads,
  backups) run on dedicated threads with an in-memory status the frontend polls, and
  support cancellation.
- The frontend never calls `invoke` directly: every feature has a typed API module
  over `src/shared/ipc.ts`, whose types mirror the serialized Rust types. The four
  pages (Catalog, Lists, MAME, Settings) stay mounted; MAME and Settings are two-pane
  pages.
- Colors come from semantic theme tokens (`src/app/theme/themes.css`, themes `light`,
  `dark`, `neon80s`; see [Themes](themes.md)); interface
  preferences are stored in the web view's `localStorage`, validated field by field.

## MAME integration and installations

- `mame/` runs MAME directly (never through a shell) with timeouts for management
  commands (`-version`, `-listxml`, `-listsoftware`) and without timeout for game
  sessions. MAME runs only on explicit user actions.
- `installations/` keeps the registered installations in `installations.json`. Each
  installation has an opaque id; every request names the installation it reads, so
  catalogs of two versions are never mixed.

## Catalog storage and search

- `catalog/` streams `-listxml` and `-listsoftware` into
  `installations/<id>/catalog.sqlite`, building a partial file that replaces the live
  one atomically. The catalog holds only MAME-derived data; migrations are additive.
- `catalog/search/` provides FTS5 full-text search, combinable filters, facets,
  sorting and pagination. The frontend never filters, sorts or paginates itself.
- `catalog/detail/` serves machine, software and software list details.
- `support_files/` keeps imported classifications in `support-files.sqlite`;
  `catalog/classification.rs` projects them per catalog into
  `classifications.sqlite`, never into the catalog.

## Global application data

User data lives outside the catalogs, in JSON files of the configuration directory:
`lists.json` (favorites and custom lists), `saved-filters.json`, `media.json`,
`remote-connections.json`, `controllers.json`, plus per installation `launch.json` and
`remote-sources.json`. Files are written atomically and an invalid file is reported,
never replaced implicitly. `app_data/` provides the inventory, consistent backups and
staged restores; `window_state.rs` restores the window placement.

## Launcher and controllers

- `launcher/` turns a structured launch request into a `LaunchPlan` (executable,
  working directory, arguments with their origin): the same plan is previewed and
  executed. MAME runs from its own directory and YMDB never writes `mame.ini`.
  Options come only from validated definitions (`launcher/options.rs`).
- `controllers/` manages controller profiles (a `-cfg_directory` per profile and
  installation), controller assignments and joystick mapping through generated
  `mapdevice` controller files.

## Media resolution

`media/` resolves images by category and MAME names in the configured directories and
serves them through the `ymdb-media` URI scheme. The interface never sends file paths;
the Tauri asset protocol is not enabled.

## Remote access and ROM Manager

- `remote/` defines remote connections (global), remote sources (per installation)
  and the `RemoteRepository` provider contract. The only provider is SFTP
  (`remote/sftp.rs`, built on `russh` and `russh-sftp`): key authentication, always
  verified server identities, read-only requests, time limits.
- `roms/` plans the whole files a launch may need from catalog relations, checks the
  local search paths, looks up missing files with one `stat` each and downloads them
  through temporary files that never replace an existing file. Download & Play then
  launches the plan's request. Local destinations reach MAME only through
  `-rompath`/`-samplepath` (`launcher/search_paths.rs`).

## Not implemented

ROM auditing (checking archive contents), HTTPS or other remote providers, resuming
partial downloads, mapping of light guns, mice and keyboards, slot configuration, and
any download of artwork or support files. See the _Limitations_ sections of
[ARCHITECTURE.md](../ARCHITECTURE.md) for each component.
