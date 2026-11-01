# Development

This page is for contributors. The architecture is described in
[Architecture](architecture.md) and, in full detail, in
[ARCHITECTURE.md](../ARCHITECTURE.md). The conventions followed by AI-assisted
sessions are in [CLAUDE.md](../CLAUDE.md); they apply to every contribution.

## Toolchain

| Tool    | Version source                                                         | Notes                             |
| ------- | ---------------------------------------------------------------------- | --------------------------------- |
| Node.js | `.node-version` (22)                                                   | `engines.node` in `package.json`  |
| pnpm    | `packageManager` in `package.json` (12)                                | Corepack works                    |
| Rust    | `rust-toolchain.toml` (with rustfmt, Clippy)                           | Installed automatically by rustup |
| Tauri   | `@tauri-apps/cli` in `package.json`, `tauri` in `src-tauri/Cargo.toml` | Tauri 2                           |

Also install the Tauri platform prerequisites listed in
[Installing YMDB](installation.md#prerequisites).

| Area             | Technology                                        |
| ---------------- | ------------------------------------------------- |
| Desktop shell    | Tauri 2                                           |
| Application core | Rust (edition 2024)                               |
| Frontend         | React, TypeScript (strict), Vite                  |
| Storage          | SQLite (`rusqlite`, bundled), JSON files          |
| SSH/SFTP         | `russh`, `russh-sftp`                             |
| Quality          | ESLint, Prettier, rustfmt, Clippy (`-D warnings`) |
| Testing          | Vitest, React Testing Library, `cargo test`       |

Dependency versions are pinned in `package.json`, and both lockfiles
(`pnpm-lock.yaml`, `src-tauri/Cargo.lock`) are committed.

## Setup and running

```sh
git clone https://github.com/davidfriscia/ymdb.git
cd ymdb
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the Vite dev server with hot reload plus the application
window. To keep your own YMDB data untouched while developing or testing, set
`YMDB_DATA_DIR` to a temporary directory (see
[Separate data directory](installation.md#separate-data-directory)); on Windows, also
set `WEBVIEW2_USER_DATA_FOLDER` to a separate folder.

## Checks

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

Run `pnpm check` before committing. Prettier also checks the Markdown documentation.

## Building

Compile the full application (production frontend build plus Rust binary) without
creating installers, as CI does:

```sh
pnpm tauri build --debug --no-bundle
```

Build release bundles for the current platform:

```sh
pnpm tauri build
```

Bundles are written to `src-tauri/target/release/bundle/`. Release packaging and
publishing are not automated, and no release has been published yet.

## Optional tests with real MAME data

These tests are ignored by default because they need local resources.

End-to-end tests that validate a real MAME executable and import its full catalog
(printing the duration and the catalog summary); set `YMDB_TEST_CATALOG_DIR` to keep
the imported catalog:

```sh
YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test --release --manifest-path src-tauri/Cargo.toml real_mame -- --ignored --nocapture
```

The search benchmark measures representative searches on an imported catalog and
prints their query plans:

```sh
YMDB_BENCH_CATALOG=/path/to/catalog.sqlite cargo test --release --manifest-path src-tauri/Cargo.toml search_benchmark -- --ignored --nocapture
```

The SFTP provider and the ROM Manager are tested against an in-process SSH/SFTP
server (`src-tauri/src/remote/test_server.rs`) with the fixture keys in
`src-tauri/src/remote/testdata/`; they need no network.

## Continuous integration

The [CI workflow](../.github/workflows/ci.yml) runs on every push to `main`, on pull
requests and on manual dispatch, on `ubuntu-latest`, `windows-latest` and
`macos-latest`. On each runner it:

1. installs the Tauri system dependencies (Linux only);
2. sets up pnpm (from `packageManager`), Node.js (from `.node-version`) and Rust
   (from `rust-toolchain.toml`), with caching;
3. runs `pnpm install --frozen-lockfile`;
4. runs `pnpm check`;
5. compiles the application with `pnpm tauri build --debug --no-bundle`.

The workflow does not publish releases or use any secrets. New checks must be added
to `pnpm check` so that they run both locally and in CI.

## Repository layout

```text
.
├── src/                    # React frontend
│   ├── app/                # Application shell, theme, interface preferences
│   ├── features/<feature>/ # One directory per feature (components, IPC wrappers, tests)
│   ├── shared/             # Shared components, IPC entry point, hooks
│   └── test/               # Test setup
├── src-tauri/              # Tauri application core (Rust)
│   ├── src/                # Rust modules (see Architecture)
│   ├── capabilities/       # Tauri permission capabilities
│   └── tauri.conf.json     # Tauri configuration
├── docs/                   # User and developer documentation
├── .github/workflows/      # Continuous integration
├── ARCHITECTURE.md         # Detailed architecture reference
├── CLAUDE.md               # Project conventions for AI-assisted sessions
└── LICENSE                 # Apache License 2.0
```

## Git conventions

Every commit uses one identity for author and committer,
`david <davidfriscia@gmail.com>`, set in the repository's local Git configuration
(never the global one):

```sh
git config --local user.name "david"
git config --local user.email "davidfriscia@gmail.com"
```

Commit messages are written in English.

## Documentation

User guides live in `docs/`; the README stays a short introduction. When a change
alters user-visible behavior, update the matching guide; when a component moves from
planned to implemented, update [ARCHITECTURE.md](../ARCHITECTURE.md).
