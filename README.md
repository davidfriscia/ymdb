# Your MAME DB (YMDB)

Your MAME DB (YMDB) is an open source, cross-platform desktop frontend for
[MAME](https://www.mamedev.org/). It aims to catalog, explore, filter, manage and
launch every system supported by the emulator: arcade machines, consoles, home and
personal computers, other devices, and software lists.

> **Status:** early development. Only the project bootstrap is in place; no MAME
> features are implemented yet. See [Current status](#current-status).

YMDB does not include ROMs, BIOS files, CHDs or any other copyrighted content.

## Supported operating systems

YMDB targets **Windows**, **macOS** and **Linux**. Every change is checked and compiled
on all three platforms by [continuous integration](#continuous-integration). The
application is developed primarily on Windows (x64), where it is also run and bundled
locally; macOS and Linux are currently verified through CI builds only.

## Goals

- Detect and configure a local MAME installation.
- Import and index the complete MAME catalog, including software lists.
- Fast full-text search and combinable filters across the whole catalog.
- Favorites and user-defined collections.
- Screenshots, artwork and metadata management.
- Launch machines and software through the local MAME, with full support for MAME
  command-line options, launch profiles and per-machine configuration.
- ROM, BIOS, CHD and dependency management with a configurable local cache.
- Optional remote repositories (HTTPS, SFTP) to download missing files on demand.
- Fully usable offline, without any remote repository configured.

## Technology stack

| Area             | Technology                                  |
| ---------------- | ------------------------------------------- |
| Desktop shell    | [Tauri 2](https://tauri.app/)               |
| Application core | Rust (stable)                               |
| Frontend         | React, TypeScript (strict), Vite            |
| Package manager  | pnpm                                        |
| Quality          | ESLint, Prettier, rustfmt, Clippy           |
| Testing          | Vitest, React Testing Library, `cargo test` |

## Prerequisites

- [Node.js](https://nodejs.org/) 22 (see `.node-version`)
- [pnpm](https://pnpm.io/) 12 (pinned via the `packageManager` field; Corepack works)
- [Rust](https://rustup.rs/) via rustup (the toolchain is pinned in `rust-toolchain.toml`)
- Tauri platform prerequisites: <https://tauri.app/start/prerequisites/>
  - **Windows:** Microsoft C++ Build Tools (MSVC) and the WebView2 Runtime
  - **macOS:** Xcode Command Line Tools
  - **Linux:** WebKitGTK 4.1 and the related system libraries. On Debian/Ubuntu:

    ```sh
    sudo apt-get install build-essential file libayatana-appindicator3-dev \
      librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev
    ```

## Installation

```sh
git clone <repository-url> ymdb
cd ymdb
pnpm install
```

## Development

Start the application in development mode (Vite dev server with hot reload plus the
Tauri window):

```sh
pnpm tauri dev
```

Build a release bundle for the current platform:

```sh
pnpm tauri build
```

Bundles are written to `src-tauri/target/release/bundle/`.

## Configuring MAME

YMDB uses a MAME installation that already exists on your computer. It never
downloads or installs MAME, and it does not require administrator privileges. Portable
installations and custom builds are supported, as long as they support the
`-version` option and print the standard MAME version line. This was verified with
MAME 0.240 and 0.280; older releases such as MAME 0.162 do not support it.

In the **MAME Installation** panel:

- **Select executable…** opens the native file picker. Choose the MAME program file
  (`mame.exe` on Windows, usually `mame` on macOS and Linux). YMDB validates it and
  saves it only if validation succeeds.
- **Detect installations** looks for MAME in the directories listed in `PATH` and in
  a few standard installation directories (non-recursively), then validates each
  candidate. Detection never changes your configuration: choose
  **Use this installation** to adopt a detected executable.
- **Verify** runs the configured executable again and shows the detected version,
  the validation status, the time of the check and the raw output of the version
  command.
- **Remove configuration** forgets the configured executable.

When YMDB starts, it shows the saved executable path without running it, and warns
if the file no longer exists.

### Validation

Validation runs `<executable> -version` directly (never through a shell), with a
20-second timeout, and accepts the executable only if it exits with code 0 and
prints a MAME version line such as `0.280 (mame0280)`. A program that runs
successfully but prints anything else is not accepted. Each failure is reported with
a specific reason: file not found, not a file, not executable, failed to start, timed
out, unexpected exit code, or unrecognized output. See
[ARCHITECTURE.md](ARCHITECTURE.md#mame-integration--implemented-installation-management-only)
for the exact rules.

### Settings location

Settings are stored in `settings.json` in the application configuration directory:

| Platform | Location                                                                  |
| -------- | ------------------------------------------------------------------------- |
| Windows  | `%APPDATA%\io.github.davidfriscia.ymdb\settings.json`                     |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/settings.json` |
| Linux    | `~/.config/io.github.davidfriscia.ymdb/settings.json`                     |

The file only contains the path of the selected MAME executable. The MAME version is
read again on every verification and is not stored.

## MAME catalog

Once MAME is configured, the **MAME Catalog** panel imports the list of all machines
known to that MAME build:

- **Import catalog** (or **Reimport catalog**) runs `<mame> -listxml`, reads its
  output as a stream and stores the machines in a local SQLite database. With MAME
  0.280 this takes a few seconds and stores 48,893 entries (including devices).
- While importing, the panel shows an indeterminate progress bar and the number of
  machines processed. **Cancel import** stops MAME and discards the partial import.
- The panel shows the MAME version the catalog was built from, the import date and
  a summary (machines, runnable systems, clones, BIOS sets, devices, mechanical
  machines, ROMs, disks, software lists, database size).
- If the configured MAME executable changes (another file, or the same file updated),
  the panel warns that the catalog no longer matches it. Reimport to update it.

Imports are atomic: a new catalog is built in a separate file and replaces the
previous one only when the import succeeds. If MAME fails, the output cannot be
parsed, the database cannot be written, or the import is cancelled, the previous
catalog stays available.

The catalog is stored in `catalog.sqlite` in the application local data directory:

| Platform | Location                                                                   |
| -------- | -------------------------------------------------------------------------- |
| Windows  | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb\catalog.sqlite`                |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/catalog.sqlite` |
| Linux    | `~/.local/share/io.github.davidfriscia.ymdb/catalog.sqlite`                |

The file contains only data derived from MAME and can be deleted at any time; it is
rebuilt by the next import. See
[ARCHITECTURE.md](ARCHITECTURE.md#catalog--implemented-machines-from--listxml) for
the schema, the imported and deferred information, and the import strategy.

## Testing and quality checks

| Command             | Description                               |
| ------------------- | ----------------------------------------- |
| `pnpm typecheck`    | TypeScript type checking                  |
| `pnpm lint`         | ESLint (zero warnings allowed)            |
| `pnpm format:check` | Prettier formatting check                 |
| `pnpm format`       | Apply Prettier formatting                 |
| `pnpm test`         | Frontend tests (Vitest + Testing Library) |
| `pnpm rust:fmt`     | rustfmt check                             |
| `pnpm rust:lint`    | Clippy with warnings treated as errors    |
| `pnpm rust:test`    | Rust unit tests                           |
| `pnpm check`        | Run all of the above (except `format`)    |

Run `pnpm check` before committing. To also verify that the full Tauri application
compiles (production frontend build plus Rust binary, without creating installers):

```sh
pnpm tauri build --debug --no-bundle
```

Optional end-to-end tests validate a real MAME executable and import its full catalog
(printing the duration and the catalog summary). They are ignored by default because
they need a local MAME installation:

```sh
YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test --release --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture
```

## Continuous integration

The [CI workflow](.github/workflows/ci.yml) runs on every push to `main`, on pull
requests and on manual dispatch. It uses a matrix of `ubuntu-latest`,
`windows-latest` and `macos-latest`, and on each runner:

1. Installs the Tauri system dependencies (Linux only).
2. Sets up pnpm (from `packageManager`), Node.js (from `.node-version`) and Rust
   (from `rust-toolchain.toml`), with caching for the pnpm store and Cargo.
3. Runs `pnpm install --frozen-lockfile`.
4. Runs `pnpm check`.
5. Compiles the application with `pnpm tauri build --debug --no-bundle`.

The workflow does not publish releases or use any secrets.

## Project structure

```text
.
├── src/                    # React frontend
│   ├── app/                # Application shell and global styles
│   ├── features/           # Feature modules (one directory per feature)
│   └── test/               # Test setup
├── src-tauri/              # Tauri application core (Rust)
│   ├── src/                # Rust modules
│   ├── capabilities/       # Tauri permission capabilities
│   └── tauri.conf.json     # Tauri configuration
├── .github/workflows/      # Continuous integration
├── ARCHITECTURE.md         # Planned architecture
├── CLAUDE.md               # Instructions for AI-assisted development sessions
└── LICENSE                 # Apache License 2.0
```

## Current status

Implemented:

- Tauri 2 + React + TypeScript project skeleton.
- Start screen showing the application name and version.
- MAME installation management: manual selection through the native file picker,
  automatic detection, validation through `-version`, version display, and saving,
  verifying and removing the configuration (see [Configuring MAME](#configuring-mame)).
- Application settings persisted as JSON in the platform configuration directory.
- MAME machine catalog: streaming import of `-listxml` into SQLite with versioned
  schema, atomic replacement, cancellation, progress and a catalog summary
  (see [MAME catalog](#mame-catalog)).
- A `check_health` Tauri command that verifies frontend-backend communication and
  reports the application status, operating system, architecture and version.
- Linting, formatting and test tooling for both the frontend and the Rust core.
- Continuous integration on Windows, macOS and Linux.

Not implemented yet: everything else, including the catalog browser, search and
filters, software list import, launcher, ROM management and remote repositories. See [ARCHITECTURE.md](ARCHITECTURE.md).

No releases have been published yet.

## License

Your MAME DB is licensed under the [Apache License, Version 2.0](LICENSE).

Third-party dependencies are distributed under their own licenses (mostly MIT and/or
Apache-2.0, plus BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0 for a few Rust crates).
Binary distributions must include the license notices of bundled dependencies.

MAME is a trademark of its respective owners. YMDB is an independent project and is
not affiliated with or endorsed by the MAME team.
