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
- Interface preferences have a versioned key; a format change gets a new key and a
  migration from the previous one in `src/app/preferences.ts`. The active MAME
  installation is not an interface preference: it is saved in `installations.json`.
- Catalog views read the installation they show from `InstallationScope`
  (`features/installations/installationsContext.ts`), pass it to every request and
  tag responses with it, so results of two installations are never shown together.
- Images reach the webview only through the `ymdb-media` protocol, by category and
  MAME names; never enable the asset protocol or send file paths to be read. Media
  files are only read: never copied, modified or stored in the catalog.
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
