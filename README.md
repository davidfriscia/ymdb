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
- Minimal start screen showing the application name and version.
- A `check_health` Tauri command that verifies frontend-backend communication and
  reports the application status, operating system, architecture and version.
- Linting, formatting and test tooling for both the frontend and the Rust core.
- Continuous integration on Windows, macOS and Linux.

Not implemented yet: everything else, including the MAME catalog, database, search,
launcher, ROM management and remote repositories. See [ARCHITECTURE.md](ARCHITECTURE.md).

No releases have been published yet.

## License

Your MAME DB is licensed under the [Apache License, Version 2.0](LICENSE).

Third-party dependencies are distributed under their own licenses (mostly MIT and/or
Apache-2.0, plus BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0 for a few Rust crates).
Binary distributions must include the license notices of bundled dependencies.

MAME is a trademark of its respective owners. YMDB is an independent project and is
not affiliated with or endorsed by the MAME team.
