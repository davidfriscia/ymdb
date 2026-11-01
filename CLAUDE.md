# CLAUDE.md

Persistent instructions for AI-assisted development sessions on Your MAME DB (YMDB).

## Project identity

- Official name: Your MAME DB
- Short name: YMDB
- Technical name: `ymdb`
- Type: open source desktop application for Windows, macOS and Linux
- Bundle identifier: `io.github.davidfriscia.ymdb`
- License: Apache-2.0 (`LICENSE`); keep `package.json`, `Cargo.toml` and
  `tauri.conf.json` license metadata consistent.
- Do not use former names (TMDB, The MAME DB, MAME Desktop Manager).

## Language conventions

- All conversations with the user must be conducted in Italian.
- All explanations, questions, progress updates and final reports must be written in Italian.
- All source code must be written in English.
- All code comments must be written in English.
- All identifiers and technical nomenclature must be in English.
- All technical documentation must be written in English.
- All configuration files and API contracts must use English.
- All application-defined logs and error messages must be written in English.
- All UI text must be written in English.
- All Git commit messages must be written in English.
- Do not translate technical identifiers, commands or technology names.

## Engineering principles

- Keep the architecture cross-platform.
- Do not introduce Windows-specific behavior into shared modules.
- Prefer simple, maintainable solutions.
- Avoid unnecessary abstractions and overengineering.
- Do not implement features outside the requested scope.
- Always run relevant tests after code changes.
- Never claim that a build or test succeeded unless it was executed.
- Do not delete or overwrite user files without explicit approval.
- Keep the application usable without network connectivity.
- Remote ROM repositories must remain optional.
- Do not bundle copyrighted ROMs or BIOS files.

## Stack

- Tauri 2, Rust (pinned in `rust-toolchain.toml`), React, TypeScript (strict), Vite, pnpm.
- Quality: ESLint, Prettier, rustfmt, Clippy (`-D warnings`).
- Tests: Vitest + React Testing Library, `cargo test`.
- Dependency versions are pinned exactly in `package.json`; lockfiles
  (`pnpm-lock.yaml`, `src-tauri/Cargo.lock`) are committed.
- Do not weaken lint, type-checking or formatting rules to make checks pass.

## Project layout

- `src/app/`: application shell and global styles.
- `src/features/<feature>/`: one directory per feature (components, typed IPC
  wrappers, tests). Components never call `invoke` directly.
- `src-tauri/src/commands.rs`: thin Tauri command adapters delegating to plain,
  unit-testable functions in domain modules.
- `src-tauri/src/commands/<area>.rs`: commands for one area. Commands return
  `Result<T, CommandError>` (`commands/mod.rs`); expected domain outcomes (such as a
  failed MAME validation) are part of `T`, not errors. Blocking work (processes,
  native dialogs, slow I/O) goes through `run_blocking` in async commands.
- `src-tauri/src/mame/`: MAME integration. Run external programs only through
  `mame::process::run_with_timeout` (direct execution, never a shell, always a
  timeout). Keep every OS-specific detail in `mame/platform.rs`.
- `src-tauri/src/installations/`: the registered MAME installations
  (`installations.json`). Extend persisted structs with `#[serde(default)]` fields so
  older files keep loading; bump `SCHEMA_VERSION` only for incompatible changes.
  Installations are identified by their opaque id, never by their version; an
  installation's executable path never changes. `settings.rs` only reads the
  single-installation `settings.json` for the migration: never write it.
- Every command that reads a catalog or a launch configuration takes an explicit
  `installationId` and resolves it once with `InstallationsState::resolve`; never read
  the active installation in the middle of a request. Only the active installation
  decides what the frontend shows and launches by default.
- Per-installation data lives under `installations/<id>/` (catalog in the local data
  directory, `launch.json` in the configuration directory). Never share or copy a
  catalog, profiles or overrides between installations implicitly; media sources and
  interface preferences stay global. Only join ids found in the registry into paths.
- Frontend IPC types mirror the serialized Rust types (camelCase fields, enums tagged
  with `kind`); update both sides together and keep a serialization test in Rust.
  Feature API modules call `invokeCommand` from `src/shared/ipc.ts`.
- Long-running operations (such as the catalog import) run on a dedicated thread,
  expose a cheap in-memory status that the frontend polls, and support cancellation.
  Never emit per-item events to the frontend.
- `src-tauri/src/catalog/`: `catalog.sqlite` holds only data derived from MAME. Every
  change to it (machine import, software list import, future full update) builds a
  partial file through `CatalogWriter` and swaps it atomically; never write to the
  live file. Open connections to it only inside `CatalogStore` (under its file
  lock), otherwise Windows cannot replace it. Never store user data in it; user data
  must reference machines by MAME short name and software by list and short name.
- Software lists may only be combined with machines of the same MAME build and
  executable; keep that check whenever the catalog import flow changes.
- Run MAME commands with large output through `stream_mame_output` in
  `catalog/import.rs` (built on `mame::process::spawn_streaming`).
- Schema changes to a database go through a new migration in its `schema.rs`;
  never edit a released migration. Catalog migrations must stay additive, because
  older catalogs are upgraded in place when read.
- Do not classify machines or software (arcade, console, computer, genre) beyond
  what MAME's data states; store free-form metadata (`info`, `sharedfeat`, `feature`)
  verbatim.
- Catalog queries (summary, search) run through `CatalogStore::with_reader`, never on
  connections opened elsewhere.
- Search SQL (`catalog/search/`): concatenate only SQL fragments written in the code
  and selected through enums; bind every request value as a `?` parameter. Never pass
  user text to FTS5 syntax: go through `search::text::parse`.
- With search text, queries that join `machines_fts`, `machines` and a classification
  table start from the FTS5 index with `CROSS JOIN` (see `facet_query` in
  `search/machines.rs`); otherwise SQLite may evaluate the MATCH once per projection
  row. Keep the plan test when changing these queries.
- FTS5 tables are external-content indexes rebuilt by imports before commit; any new
  code that writes indexed tables must rebuild them too.
- Measure search changes with the `search_benchmark` ignored test on a real catalog
  and check `EXPLAIN QUERY PLAN` before adding indexes.
- MAME is executed only in response to explicit user actions; never at startup.
- Launches go through `launcher::prepare` and `LaunchPlan`: the same plan is
  previewed and executed, MAME runs from its own directory (so `mame.ini` applies),
  and the frontend sends structured requests, never arguments. YMDB never writes
  `mame.ini` or other MAME files; launch settings live in `launch.json`.
- New MAME options are added as definitions in `launcher/options.rs` (validated
  kinds, checked against `mame -showusage`); never accept free-form option names.
  Input/controller options belong to the future input devices feature.
- Game sessions have no timeout and are never killed implicitly (not even when YMDB
  exits); only an explicit, confirmed Stop terminates them.
- `src-tauri/src/controllers/`: controllers, controller profiles and assignments in the
  global `controllers.json` (same store rules as `lists.json`). MAME configures inputs
  and saves them; YMDB never writes `default.cfg`, `<machine>.cfg` or other MAME
  files and has no button editor. The system profile `mame` passes no option and
  must keep the launcher's command line unchanged (keep its regression test). A user
  profile owns `installations/<id>/controller-profiles/<profileId>/cfg` (passed as
  `-cfg_directory`, never shared between installations or profiles, never the
  installation's own `cfg`) and an optional verbatim `controller-profiles/<profileId>/base.cfg`
  (`-ctrlrpath`/`-ctrlr base`); without it, pass `-ctrlr ""` so that no INI `ctrlr`
  applies. `ctrlr`/`ctrlrpath` are never launch option definitions. Profile and
  players are resolved separately (default < machine < software < machine+software
  < request); launch profiles are unrelated. `prepare`/preview create nothing: the
  directory is created by `controls::materialize` right before MAME starts.
  Referenced profiles and controllers are not deleted; profile files are never
  deleted automatically (removing a `base.cfg` moves it aside, saves
  `controllers.json`, then deletes it, or puts it back on failure). A `base.cfg`
  needs a `<system name="default">` section and valid `mapdevice` controllers
  (`base_file::validate`); files imported before a stricter check are reported at
  launch and in Settings, never changed or deleted.
- Device mapping: a joystick controller with `deviceId`, `provider` and `binding`
  (`device` | `portOrder`) is mapped to its player's joystick index by a generated
  controller file (`controllers/ctrlr_file.rs`): a `<system name="default">` section
  of `mapdevice` inserted after `<mameconfig>` of the baseline, whose bytes are kept
  verbatim; baseline mappings that touch assigned devices or indexes are conflicts,
  never silently dropped. The file is named by its content
  (`<local data>/controller-files/ymdb-<hash>.cfg`), planned by `compose` (read-only)
  and written only by `materialize`. Mapped launches always pass the controllers'
  `-joystickprovider` (one per launch) and `-verbose`; launches without mapped
  players must keep their previous command line exactly (keep the regression tests).
  The `mame` profile keeps MAME's `cfg` directory, composes the `ctrlr` of `mame.ini`
  and refuses a `ctrlr` from any other INI file. Never infer a device identity:
  detection suggests `portOrder`, `device` is the user's choice, XInput slots are
  `portOrder` only, ids MAME could confuse (substring, case-insensitive) are refused,
  and controllers without `binding` (older files) stay unmapped. Detection
  (`controllers/discovery.rs`) runs only on an explicit action (`pong
-noreadconfig -verbose -video none`, temporary directory, timeout) and a failure
  is an error, never an empty list. Verification (`launcher/mapping_check.rs`)
  reports only what MAME's verbose output proves; a port order mapping is never
  presented as a physical identity, and no controller has a persistent connected
  state.
- The controllers interface (`features/controllers/`) never resolves precedence: it
  shows `resolve_controls` and the plan. Preferences are changed with
  `update_controller_assignment` (per-field `keep`/`set`/`inherit`), so changing one
  field never drops the other. The Controls and Players selections of the launch bar
  are temporary (`LaunchRequest.controls`, used by Play, the preview, Download & Play
  and Configure Controls) and saved only by an explicit "Save as preference".
  Saved changes increment the launcher's `controlsVersion`. Sessions carry the
  warnings of the plan they started with; never show preview warnings as a session's.
- `src-tauri/src/remote/`: remote repositories. The ROM manager talks to repositories
  only through `provider::RemoteRepository`; a new protocol is a new `ProviderConfig`
  and `Repository` variant (never a reinterpretation of existing connections), and
  optional operations are declared in `Capabilities` and fail with `unsupported`
  instead of being emulated. Remote paths are `RemotePath`/`RelativePath` (POSIX),
  never `PathBuf`; never join unchecked remote path text.
- Server identities are always verified (no insecure mode, no automatic approval of a
  changed key). Never store passwords, passphrases or keys in configuration files or
  logs, and never execute remote commands. Network operations run only on explicit
  user actions, with time limits.
- Remote sources (`installations/<id>/remote-sources.json`) reference global
  connections by id; a referenced connection cannot be removed and sources are never
  changed implicitly. ROM Manager destinations reach MAME only through
  `launcher/search_paths.rs` (`-rompath`/`-samplepath` composed with the resolved
  value or `mame.ini`), never by writing MAME files.
- `src-tauri/src/roms/`: the ROM manager works with whole files (archives, CHDs,
  samples) chosen from catalog relations; never inspect, audit, extract or rebuild
  archive contents, and never deduplicate across plans or versions. It reaches
  repositories only through `roms::Connector`/`RemoteRepository` (no protocol code).
  A missing candidate is a warning, never an error; a technical remote failure is
  `unresolved`, never "not found". Downloads go through temporary files and are
  finalized without ever replacing an existing file. Download plans are snapshots of
  their installation and request; Download & Play launches only that request.
- `src-tauri/src/lists/`: global user lists in `lists.json` (Favorites is the system
  list `favorites`; custom lists are `user` lists with an opaque immutable id, a name
  unique ignoring case (`name_key`) and dates). Favorites and custom lists stay
  functionally independent: custom list commands refuse system lists and Favorites
  is never shown as a custom list. Entries reference machines by MAME short name and
  software by list and short name only; entries missing from a catalog are kept,
  never matched approximately. Lists never go into a catalog: searches take
  `userLists` (each an AND condition, an empty list matches nothing, an unknown list
  is an error) and receive the entries through `ListMembers` (one JSON parameter read
  with `json_each`, cached until the next change). Changes are saved before being
  kept in memory; an invalid `lists.json` is reported, never replaced. Membership
  changes name their lists and set wanted states (never a whole-list replace from
  a frontend snapshot); renaming never merges. Imports are previewed, then planned
  again and applied atomically; folder `.ini` imports (`ini.rs`, `folder_import.rs`)
  never import `[FOLDER_SETTINGS]` or non short-name lines, merge into same-named
  lists as a set union, and must stay idempotent. The interface manages machine
  entries only; software entries are kept.
- `src-tauri/src/support_files/`: MAME support files (catver, genre, catlist,
  version, series), imported only by explicit user action (preview, then apply);
  never downloaded, never modified on disk. Each format has its own parser producing
  memberships or attributes (`VerAdded` is an attribute, not a category); identify
  formats by header and structure, never by file name alone. The global repository
  (`support-files.sqlite`) keeps one source per format and one active source per
  dimension: never combine sources for a dimension, never activate an alternative
  implicitly, and increment a dimension's revision only when its effective content
  changes. Support files never touch `lists.json`.
- Classifications reach a catalog only through its projection
  (`installations/<id>/classifications.sqlite`, `catalog/classification.rs`), never
  through `catalog.sqlite`. Commands that read classifications (machine search,
  machine detail, status) call `sync_catalog` first; projections are rebuilt in one
  transaction, only for dimensions whose repository revision, source or catalog token
  changed, with set-based `INSERT ... SELECT` over the attached repository and catalog
  (keep the `+m.machine_name IN (...)` form: without it SQLite probes per machine). A
  failed sync keeps the previous projection and must never block the catalog.
  Classification values missing from a projection match nothing.
- `saved-filters.json` (`saved_filters.rs`): presets with versioned criteria owned by
  the browser (`features/saved-filters/savedFilterCriteria.ts`), stored verbatim.
  A new kind of filter needs a new criteria version and a conversion; presets of an
  unknown version are kept, never rewritten. Presets never include presentation
  settings and are never updated implicitly.
- `src-tauri/src/app_data/`: inventory, backup and restore of application data. A
  new persistent file must be added to the inventory (`inventory.rs`) and either to the
  backup scope (`scope.rs`: `classify`, `CONFIG_FILES`, `replaced_names`) or documented
  as regenerable/transient. Archive entries are logical paths accepted by
  `scope::classify`, never absolute paths; categories are derived, never trusted.
  Databases go into a backup only through the SQLite backup API (`CatalogStore` via
  `with_reader`, `SupportRepository::backup_to`). Every command that changes
  application data, and every command starting an import, a download or a MAME
  session, holds `commands::data_change()` (the shared data gate) for the whole
  change, never across an `.await`. A restore never replaces files while YMDB runs:
  it stages and verifies, then `restore::recover` swaps at the next start with a
  rollback copy and a transaction file. Never store secrets in application data; if
  one becomes necessary, the backup policy must be decided first.
- Window placement lives in `window_state.rs` (`window-state.json`): the window is
  created hidden and shown after `plan_restore`; keep the normal geometry separate
  from the maximized state.
- Detail DTOs (`catalog/detail/`) are separate from the search DTOs. Do not embed
  unbounded one-to-many relations (clones, software of a list, machines of a list)
  in them: return a count and list them through the paginated search.

## Frontend conventions

- Colors come only from the semantic tokens defined per theme in
  `src/app/theme/themes.css`; never hardcode a color in a stylesheet or component.
  A new theme is a complete token set under `:root[data-theme="<id>"]` plus an entry
  in `THEMES` (`themes.ts`). The default theme is `light`.
- Interface preferences (theme, card size, filter panel, renderer) live in
  `localStorage` through `src/app/preferences.ts`, validated field by field with
  defaults. Never store MAME configuration or catalog data there.
- The catalog browser never searches, filters, sorts, counts or paginates in the
  frontend: it builds typed requests for the backend search. Keep results and facet
  requests separate so that paging and sorting do not recount facets.
- Result renderers implement `ResultRendererProps` and are registered in
  `features/browser/renderers/registry.ts` with their options (`range`, `toggle`,
  `choice` with defaults); the View Options panel and the preferences are generic and
  must not get renderer-specific code. Renderers only display the current page and
  never change the request. Do not register unimplemented renderers.
- In the catalog browser only `.browser__scroll` scrolls the results; the single
  `Pagination` lives in the results footer, outside renderers. Solve heights
  structurally (flex/grid with `min-height: 0`), never with fixed positioning or
  window-based heights.
- Favorite state comes only from `FavoritesProvider` and is changed only through
  `FavoriteBookmark` (cards and detail pages); never request membership per card.
  Bookmarks sit next to, never inside, the button that opens a card. "Favorites
  only" is an interface preference that adds `favorites` to the `userLists` filter
  of backend requests.
- Custom list summaries come only from `CustomListsProvider`; views that change a
  list call `changed()` so every view reloads. Memberships are changed through
  `ManageListsModal` (one implementation for cards and details, one
  `set_entry_memberships` per Save with only the changed lists) or the Lists page.
  The Lists filter is `listId` of the browser state (machines only, saved in saved
  filters by id); a deleted list is reported, never turned into "All items".
- Detail artwork goes through `MediaGallery` (same `mediaUrl` requests as the Media
  Grid, one per configured category); never add another way to look up image files.
- Interface preferences have a versioned key; a format change gets a new key and a
  migration from the previous one in `src/app/preferences.ts`. The active MAME
  installation is not an interface preference: it is saved in `installations.json`.
- Catalog views read the installation they show from `InstallationScope`
  (`features/installations/installationsContext.ts`), pass it to every request and
  tag responses with it, so results of two installations are never shown together.
- Images reach the webview only through the `ymdb-media` protocol, by category and
  MAME names; never enable the asset protocol or send file paths to be read. Media
  files are only read: never copied, modified or stored in the catalog.
- MAME and Settings are two-pane pages (`src/shared/TwoPaneLayout.tsx`): settings of
  one installation are a section of its group on the MAME page, global settings a
  section of Settings. Sections stay mounted once shown (`useVisited`), so a change of
  selection never discards unsaved forms; selecting never changes the active
  installation.
- Filters with many values use the shared pieces of `features/browser/filters/`
  (`FacetValueList`, `CategoryTree`, `FacetBrowserModal`): bounded lists that scroll
  on their own, a value search and "Browse all", whose selection is a draft applied
  with one `setFacetValues`. Classification facets are shown and requested only for
  the dimensions of the catalog's classification status. Saved filters are applied
  with one `applyCriteria` action.
- Asynchronous loads in components go through `src/shared/useLoader.ts` (stale
  responses ignored, memoized inputs).
- See `ARCHITECTURE.md` for planned components; update it when a component moves
  from planned to implemented.

## Commands

- `pnpm install`: install dependencies.
- `pnpm tauri dev`: run the app in development mode.
- `pnpm tauri build`: build a release bundle.
- `pnpm check`: run every check (typecheck, ESLint, Prettier, Vitest, rustfmt,
  Clippy, cargo test). Run it before declaring work complete.
- `pnpm tauri build --debug --no-bundle`: compile the full application without
  creating installers (same step as CI).

## Git conventions

- Every commit of this repository uses the identity `david <davidfriscia@gmail.com>`
  (author and committer), configured with `git config --local`; never change the
  global Git configuration of the machine.

## Continuous integration

- `.github/workflows/ci.yml` runs `pnpm check` and a Tauri compile on Windows,
  macOS and Linux. Keep it in sync when adding checks or system dependencies.
- New checks must be added to `pnpm check` so they run both locally and in CI.
- Do not add release/publishing workflows, secrets or tokens unless explicitly requested.
- Remote CI results can only be confirmed after the repository is pushed to GitHub;
  never report CI as passing based on local runs.

## Environment notes (Windows development machine)

- Node.js is managed by fnm. In a non-initialized shell, run
  `fnm env --shell powershell | Out-String | Invoke-Expression` first so that
  `node` and `pnpm` are available.
- End-to-end tests must not touch the user's YMDB data: run the app with
  `YMDB_DATA_DIR` set to a temporary directory and a separate
  `WEBVIEW2_USER_DATA_FOLDER`. If the user's own YMDB is running, build into a
  separate `CARGO_TARGET_DIR` instead of stopping it.
