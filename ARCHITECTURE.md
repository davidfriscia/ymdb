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

### Desktop UI — Implemented (bootstrap only)

React + TypeScript application rendered by the system WebView through Tauri.

- `src/app/` contains the application shell and global styles.
- `src/features/<feature>/` groups each feature's components, IPC wrappers and tests.
  IPC calls are wrapped in typed functions (for example `healthApi.ts`) so components
  never call `invoke` directly.
- Currently implemented: a start screen and the `health-check` feature, which calls
  the `check_health` command.

Planned: catalog browser, search and filter panels, machine and software detail
views, collections, settings, launch profiles.

### Application Core — Implemented (bootstrap only)

The Rust crate in `src-tauri/`.

- `lib.rs` builds the Tauri application and registers commands.
- `commands.rs` contains Tauri command adapters. Commands extract what they need from
  the Tauri runtime and delegate to plain functions that can be unit-tested without
  Tauri.
- `health.rs` builds the health report returned by `check_health`.

Planned: one module per domain area (MAME integration, catalog, storage, cache),
introduced only when the corresponding feature is implemented.

### MAME integration — Planned

- Locate and validate a local MAME executable and its version.
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

### Local storage — Planned

- Application data and configuration stored in the platform-specific application
  data directories resolved through Tauri, never hard-coded paths.
- User-configurable ROM, CHD and artwork directories.

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
  in the MAME integration module only.
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
