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
│ Local MAME install  │
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
- Implemented features:
  - `mame-installation`: the MAME Installation panel (see below).
  - `health-check`: calls `check_health` to verify frontend-backend communication.

Planned: catalog browser, search and filter panels, machine and software detail
views, collections, launch profiles, navigation between screens.

### Application Core — Implemented (partially)

The Rust crate in `src-tauri/`.

- `lib.rs` builds the Tauri application, registers plugins, managed state and commands.
- `commands/` contains Tauri command adapters (`health.rs`, `mame.rs`). Commands
  extract what they need from the Tauri runtime and delegate to plain functions that
  are unit-tested without Tauri. Blocking work (process execution, native dialogs)
  runs on the blocking thread pool so the UI never freezes. Failures are returned as
  a structured `CommandError` (`settings`, `notConfigured`, `internal`).
- `health.rs` builds the health report returned by `check_health`.
- `settings.rs` persists application settings (see [Local storage](#local-storage--implemented-partially)).
- `mame/` implements the MAME integration (see below).

### MAME integration — Implemented (installation management only)

Implemented in `src-tauri/src/mame/`, as independent modules reusable by the
planned catalog importer and launcher:

- `process.rs`: runs a program directly (never through a shell) with closed stdin,
  captured and size-bounded output, and a timeout after which the process is killed.
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
(select, verify, detect), always with the fixed argument `-version`. Loading the
configuration at startup never runs MAME. The frontend cannot pass arbitrary
arguments.

Planned:

- Read MAME's machine and software list information (`-listxml`, `-getsoftlist`,
  or equivalent data files).
- Build command lines and launch machines and software, supporting the full set of
  MAME command-line options, launch profiles and per-machine configuration.
- Verify ROM sets, BIOS and CHD requirements.

### Catalog — Planned

- Parse and normalize MAME machine and software list data.
- Model machines, devices, software lists, software items and their relationships
  (parent/clone, BIOS, device and ROM dependencies).
- Provide full-text search and combinable filters over the whole catalog.
- Manage favorites, user collections, screenshots, artwork and metadata.

### Local storage — Implemented (partially)

Implemented: application settings in a JSON file, `settings.json`, in the
platform configuration directory resolved by Tauri (`app_config_dir`) for the
bundle identifier `io.github.davidfriscia.ymdb`:

| Platform | Location                                                                               |
| -------- | -------------------------------------------------------------------------------------- |
| Windows  | `%APPDATA%\io.github.davidfriscia.ymdb\settings.json`                                  |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/settings.json`              |
| Linux    | `$XDG_CONFIG_HOME/io.github.davidfriscia.ymdb/settings.json` (default `~/.config/...`) |

Format:

```json
{
  "schemaVersion": 1,
  "mame": { "executablePath": "C:\Emulators\MAME\mame.exe" }
}
```

- `mame` is `null` or absent when no executable is configured. The detected MAME
  version is deliberately not stored; it is read again on every verification.
- Writes are atomic: a temporary file in the same directory is written, synced and
  renamed over `settings.json`.
- A missing file means default settings. An invalid file is reported to the user
  without crashing and is replaced the next time the configuration is saved.
- Access to the file is serialized by a mutex held in Tauri managed state.

Planned: user-configurable ROM, CHD and artwork directories.

### SQLite — Planned

- Single local SQLite database for the indexed catalog, user data (favorites,
  collections, profiles) and cache metadata.
- Full-text search through SQLite FTS.
- Versioned schema migrations.

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
