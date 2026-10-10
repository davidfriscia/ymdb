# Your MAME DB (YMDB)

Your MAME DB (YMDB) is an open source, cross-platform desktop frontend for
[MAME](https://www.mamedev.org/). It aims to catalog, explore, filter, manage and
launch every system supported by the emulator: arcade machines, consoles, home and
personal computers, other devices, and software lists.

> **Status:** early development. MAME configuration, catalog import, search, browsing
> and launching work; ROM management does not yet. See [Current status](#current-status).

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

To run YMDB with separate, empty data (for tests or a portable setup), set
`YMDB_DATA_DIR` to a directory: settings, catalog, launch and media configuration are
then stored in its `config/` and `data/` subdirectories instead of the user's.

## Configuring MAME

YMDB uses a MAME installation that already exists on your computer. It never
downloads or installs MAME, and it does not require administrator privileges. Portable
installations and custom builds are supported, as long as they support the
`-version` option and print the standard MAME version line. This was verified with
MAME 0.240 and 0.280; older releases such as MAME 0.162 do not support it.

On the **MAME** page, in the **MAME Installation** panel:

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

Once MAME is configured, the **MAME Catalog** panel of the **MAME** page imports the machines and the
software lists known to that MAME build:

- **Import catalog** (or **Reimport machines**) runs `<mame> -listxml`, reads its
  output as a stream and stores the machines in a local SQLite database. With MAME
  0.280 this stores 48,893 entries (including devices) in a few seconds.
- **Import software lists** (or **Update software lists**) runs
  `<mame> -listsoftware` and adds the software lists to the catalog: with MAME 0.280,
  720 lists and 139,122 software, with their parts, media (ROMs and disks),
  checksums and metadata. MAME itself needs from about 15 seconds to a minute to produce this list.
  Software lists can only be imported from the same, unchanged MAME executable the
  machines were imported from.
- Each software list is stored once and linked to the machines that reference it,
  keeping the kind of relation (original or compatible) and the machine's filter.
- While importing, the panel shows an indeterminate progress bar and the number of
  machines or software processed. **Cancel import** stops MAME and discards the
  partial import.
- The panel shows the MAME version, import date and a summary for machines and for
  software lists.
- If the configured MAME executable changes (another file, or the same file updated),
  the panel warns that the catalog no longer matches it. Reimport to update it.
  Reimporting machines starts a new catalog without software lists; import the
  software lists again afterwards.

Imports are atomic: a new catalog is built in a separate file and replaces the
previous one only when the import succeeds. If MAME fails, the output cannot be
parsed, the database cannot be written, or the import is cancelled, the previous
catalog (machines and software lists) stays available. If another program keeps the
catalog file open, Windows prevents its replacement: the import fails with a message
asking to close that program, and the previous catalog is kept.

The catalog is stored in `catalog.sqlite` in the application local data directory:

| Platform | Location                                                                   |
| -------- | -------------------------------------------------------------------------- |
| Windows  | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb\catalog.sqlite`                |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/catalog.sqlite` |
| Linux    | `~/.local/share/io.github.davidfriscia.ymdb/catalog.sqlite`                |

The file contains only data derived from MAME and can be deleted at any time; it is
rebuilt by the next import. See
[ARCHITECTURE.md](ARCHITECTURE.md#catalog--implemented-machines-and-software-lists) for
the schema, the imported and deferred information, and the import strategy.

## Catalog search

The backend provides full-text search with combinable filters, facets, sorting and
pagination over the imported catalog, through the commands
`search_catalog_machines` and `search_catalog_software` (typed wrappers in
`src/features/search/searchApi.ts`). Machines and software are searched separately.

- Text: every word must match, in any indexed field (machines: name, description,
  manufacturer, year; software: name, description, publisher, year, list name), as a
  word prefix, ignoring case and accents. An exact short name (`sf2`) ranks first.
  Search operators typed by the user are treated as plain text.
- Filters, facets with counts, relevance and field sorting, and server-side
  pagination (up to 200 results per page) are described in
  [ARCHITECTURE.md](ARCHITECTURE.md#catalog-search--implemented), with measured
  timings: most searches on the MAME 0.280 catalog take less than 5 ms.
- The [catalog browser](#browsing-the-catalog) is built on this engine.

Catalogs imported by earlier versions are indexed automatically the first time they
are opened.

## Browsing the catalog

The **Catalog** page (the start page) browses the imported machines or software:

- Choose **Machines** or **Software**, type a text (the search starts when you pause
  typing, or press Enter or **Search**), pick a sort order and direction.
- The **Filters** panel lists, with counts, the manufacturers, years, driver and
  emulation statuses and software lists of the machines (publishers, lists, years,
  support statuses and part interfaces for software), plus parent/clone, BIOS,
  device, mechanical, runnable and "declares ROMs/CHDs" filters. Values of one
  filter are alternatives; different filters combine. Large lists have a value
  search and **Show more**. Active filters appear as chips above the results; remove
  them one by one or with **Clear all filters**. The **Filters** button hides the
  panel to give the results the whole width; the filters stay active.
- The **View** button opens the **View Options** panel on the right. **View mode**
  switches between **Classic Cards** (text cards; **Card size**) and **Media Grid**
  (artwork tiles; **Tile size**, **Artwork** category, **Image fit**, and whether to
  show the title, the technical name and the status badge). Changing the view never
  changes the search, filters, sort or page. Both side panels can be hidden
  independently; the results then use the whole width. The number of columns adapts
  to the window. Pages hold 25 to 200 results, with page numbers and **Go to**.
- Click a card or tile (or press Enter on it) to open its detail: machines show general data, relations, emulation
  status, ROMs, CHDs, devices, displays, controls, chips and software lists;
  software shows its list, parts, ROMs and disks and the machines declaring its list;
  software lists show their software and machines. Parents, clones, devices and
  software lists are links. **Back** returns to the previous detail and **Back to
  results** to the results, with the search, filters, page and scroll position
  unchanged; **Previous**/**Next** move through the current page.
- ROMs, CHDs and disks are what MAME declares; YMDB does not check files on disk yet,
  and a software list declared by a machine does not guarantee compatibility.

The **MAME** page holds the MAME installation and catalog import panels. The
**Settings** page selects the **Light** (default) or **Dark** theme, applied
immediately to the whole application, holds the MAME launch options and shows the
version and a backend connection check. Theme, panel visibility, view mode, the
options of each view and the command line preview are saved in the application's
web storage, separately from the MAME, launch and media configuration.

## Artwork and screenshots

The Media Grid shows images from directories you already have, for example an
artwork collection or MAME's own `snap` folder. Configure them in **Settings → Media
Sources**: each category (Snapshots, Title screens, Flyers, Cabinets, Marquees) takes
one or more directories, searched from top to bottom. Local disks and network shares
mounted by the operating system work; YMDB only reads them and never copies or
changes your files.

Images are found by name:

- machines: `pacman.png` (also `.jpg`, `.jpeg`, `.webp`) in the directory, or
  `pacman/0000.png` as saved by MAME;
- software: `<list>/<software>.png`, for example `nes/smb.png`, so a directory of
  software list snapshots (such as `snap_SL`) can be added to Snapshots.

When a title has no image, its tile shows a placeholder: YMDB never shows the image
of another title, not even of the parent of a clone. Zipped collections are not
supported yet.

## Launching MAME

YMDB starts the configured MAME executable for a machine or a software. It needs a
catalog imported from that same executable (reimport it after changing or updating
MAME). YMDB does not check that ROMs, CHDs or software media are present: when files
are missing, MAME refuses to start and YMDB shows its message and output.

- **Machines:** open a machine and press **Play**. YMDB runs
  `mame <machine> [options]` from the MAME directory, so MAME reads its own
  `mame.ini` and relative paths exactly as when you run it there.
- **Software:** open a software, choose the machine and press **Play**. The machine
  list contains the machines declaring the software list, with the relation MAME
  declares (original or compatible list), whether the list filter (for example NTSC
  or PAL) matches the software, and whether the machine has a media device for it.
  A machine is preselected only when it is the only one that is original and
  matching; otherwise you choose. This information comes from MAME's catalog and
  does not guarantee that a software works on a machine. Each part of the software
  (for example the disks of a multi-disk game) is mounted on a media device, for
  example `mame apple2e -flop1 apple2_flop_orig:ultima4:flop1 -flop2
apple2_flop_orig:ultima4:flop2`; the assignment can be changed before launching.
- **Command line** shows the exact command that **Play** runs, with the origin of
  every option.
- While MAME runs, the detail page shows the session; **Stop** terminates MAME after
  a confirmation (unsaved progress and NVRAM changes are lost). Closing YMDB never
  closes MAME.

### Launch options

YMDB passes only the options you set; everything else follows MAME's own
configuration, which YMDB never modifies. Options are set at four levels, the later
ones taking precedence:

1. **Global options** (Settings page).
2. **Launch profiles** (Settings page): named sets of options, chosen with the
   **Profile** selector when launching; the default profile applies otherwise.
3. **Options for this machine** (machine detail page).
4. **Options for this software** (software detail page).

The available options cover display, performance, sound, search paths and output
directories, BIOS and RAM size and session options (for example `seconds_to_run`).
Values are checked before saving. They are saved in `launch.json`, next to
`settings.json`. Controller and input device settings are not managed by YMDB yet.

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
they need a local MAME installation. Set `YMDB_TEST_CATALOG_DIR` to keep the imported
catalog:

```sh
YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test --release --manifest-path src-tauri/Cargo.toml real_mame -- --ignored --nocapture
```

The search benchmark measures representative searches on an imported catalog and
prints their query plans:

```sh
YMDB_BENCH_CATALOG=/path/to/catalog.sqlite cargo test --release --manifest-path src-tauri/Cargo.toml search_benchmark -- --ignored --nocapture
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
├── ARCHITECTURE.md         # Architecture (implemented and planned)
├── CLAUDE.md               # Instructions for AI-assisted development sessions
└── LICENSE                 # Apache License 2.0
```

## Current status

Implemented:

- Tauri 2 + React + TypeScript project skeleton.
- Application shell with Catalog, MAME and Settings pages; light and dark themes.
- MAME installation management: manual selection through the native file picker,
  automatic detection, validation through `-version`, version display, and saving,
  verifying and removing the configuration (see [Configuring MAME](#configuring-mame)).
- Application settings persisted as JSON in the platform configuration directory.
- MAME catalog: streaming import of machines (`-listxml`) and software lists
  (`-listsoftware`) into SQLite with versioned schema and migrations, version
  consistency checks, atomic replacement, cancellation, progress and summaries
  (see [MAME catalog](#mame-catalog)).
- Catalog search engine: FTS5 full-text search, combinable filters, facets with
  counts, sorting and pagination (see [Catalog search](#catalog-search)).
- Catalog browser with filters, facets, card results, pagination and machine,
  software and software list detail views (see [Browsing the catalog](#browsing-the-catalog)).
- Media Grid view with local artwork and screenshots from configurable media
  sources (see [Artwork and screenshots](#artwork-and-screenshots)).
- MAME launcher: machines and software, with machine selection, part assignment,
  command line preview, global options, launch profiles, machine and software
  options, and session monitoring (see [Launching MAME](#launching-mame)).
- A `check_health` Tauri command that verifies frontend-backend communication and
  reports the application status, operating system, architecture and version.
- Linting, formatting and test tooling for both the frontend and the Rust core.
- Continuous integration on Windows, macOS and Linux.

Not implemented yet: everything else, including ROM management, input device
configuration, artwork download and remote repositories. See [ARCHITECTURE.md](ARCHITECTURE.md).

No releases have been published yet.

## License

Your MAME DB is licensed under the [Apache License, Version 2.0](LICENSE).

Third-party dependencies are distributed under their own licenses (mostly MIT and/or
Apache-2.0, plus BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0 for a few Rust crates).
Binary distributions must include the license notices of bundled dependencies.

MAME is a trademark of its respective owners. YMDB is an independent project and is
not affiliated with or endorsed by the MAME team.
