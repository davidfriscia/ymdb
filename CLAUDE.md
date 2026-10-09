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
- `src-tauri/src/settings.rs`: the JSON settings file. Extend `Settings` with
  `#[serde(default)]` fields so older files keep loading; bump `SCHEMA_VERSION` only
  for incompatible changes.
- Frontend IPC types mirror the serialized Rust types (camelCase fields, enums tagged
  with `kind`); update both sides together and keep a serialization test in Rust.
  Feature API modules call `invokeCommand` from `src/shared/ipc.ts`.
- Long-running operations (such as the catalog import) run on a dedicated thread,
  expose a cheap in-memory status that the frontend polls, and support cancellation.
  Never emit per-item events to the frontend.
- `src-tauri/src/catalog/`: `catalog.sqlite` holds only data derived from MAME and is
  rebuilt (not migrated) on every import, then swapped atomically. Never store user
  data in it; user data must reference machines by their MAME short name.
- Schema changes to a database go through a new migration in its `schema.rs`;
  never edit a released migration.
- Do not classify machines (arcade, console, computer) beyond what MAME's data states.
- MAME is executed only in response to explicit user actions; never at startup.
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
