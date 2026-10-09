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
- `src/shared/ipc.ts` provides `invokeCommand`, the single place where `invoke` is
  called, and maps command failures to the `CommandError` contract.
- Implemented features:
  - `mame-installation`: the MAME Installation panel (see below).
  - `catalog`: the MAME Catalog panel (catalog summary, import, progress, cancel).
  - `health-check`: calls `check_health` to verify frontend-backend communication.

Planned: catalog browser, search and filter panels, machine and software detail
views, collections, launch profiles, navigation between screens.

### Application Core — Implemented (partially)

The Rust crate in `src-tauri/`.

- `lib.rs` builds the Tauri application, registers plugins, managed state and commands.
- `commands/` contains Tauri command adapters (`health.rs`, `mame.rs`, `catalog.rs`).
  Commands extract what they need from the Tauri runtime and delegate to plain
  functions that are unit-tested without Tauri. Blocking work (process execution,
  native dialogs) runs on the blocking thread pool, and long-running work (catalog
  import) on a dedicated thread, so the UI never freezes. Failures are returned as a
  structured `CommandError` (`settings`, `notConfigured`, `importInProgress`,
  `internal`).
- `health.rs` builds the health report returned by `check_health`.
- `settings.rs` persists application settings (see [Local storage](#local-storage--implemented-partially)).
- `mame/` implements the MAME integration (see below).
- `catalog/` implements the machine and software list catalog (see [Catalog](#catalog--implemented-machines-and-software-lists)).

### MAME integration — Implemented (installation management only)

Implemented in `src-tauri/src/mame/`, as independent modules reusable by the
planned catalog importer and launcher:

- `process.rs`: runs a program directly (never through a shell) with closed stdin.
  `run_with_timeout` captures size-bounded output and kills the process after a
  timeout (used for `-version`). `spawn_streaming` hands the standard output to the
  caller for incremental reading, captures standard error (bounded), and runs a
  watchdog that kills the process when a cancellation flag is set or when no output
  is produced for an inactivity timeout (used for `-listxml`). Dropping a streaming
  process always terminates and reaps it.
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
(select, verify, detect, import catalog, import software lists), always with fixed
arguments (`-version`, `-listxml`, `-listsoftware`). Loading the configuration or the
catalog at startup never runs MAME. The frontend cannot pass arbitrary arguments.

Planned:

- Build command lines and launch machines and software, supporting the full set of
  MAME command-line options, launch profiles and per-machine configuration.
- Verify ROM sets, BIOS and CHD requirements.

### Catalog — Implemented (machines and software lists)

Implemented in `src-tauri/src/catalog/`:

- `parser.rs`: streaming parser for `mame -listxml` (quick-xml, event by event,
  reusable buffers). Memory use is bounded by one `<machine>` entry; the document
  is never loaded as a whole.
- `software_parser.rs`: streaming parser for `mame -listsoftware`; memory use is
  bounded by one `<software>` entry.
- `xml.rs`: helpers shared by both parsers (attributes, text, entities, numbers).
- `model.rs`, `software_model.rs`: plain data models, close to the XML.
- `schema.rs`: versioned SQLite schema (migrations tracked in `PRAGMA user_version`).
- `store.rs`: catalog database files, inserts and atomic replacement.
- `import.rs`: orchestration (validate, run MAME, parse, store, swap). Machines and
  software lists share `stream_mame_output`, built on `mame::process::spawn_streaming`.
- `freshness.rs`: whether the catalog matches the configured executable.

#### The `-listxml` format

Analyzed on the real output of MAME 0.280 (302 MB, 48,893 entries) and on the DTDs of
MAME 0.77 to 0.278:

- Each entry is a `<machine>` element (`<game>` before MAME 0.162; both are
  accepted). Entries include devices (`isdevice="yes"`, all `runnable="no"`; 6,802
  in 0.280), BIOS sets (`isbios="yes"`, 78) and mechanical machines
  (`ismechanical="yes"`, 15,683). Devices have no year, manufacturer or driver.
- The format does not say whether a machine is an arcade game, a console or a
  computer, so YMDB does not classify machines; only the XML flags are stored.
- `cloneof` names the parent; `romof` names the machine whose ROMs are inherited
  (the parent for clones, the BIOS for parents that need one); both always
  reference existing entries, sometimes listed later in the document.
  `sampleof` names a sample set, which is not always a machine (14 of 76 in 0.280).
- `device_ref` lists the devices a machine instantiates, with repetitions (one per
  instance); ROM names can repeat within a machine.
- Text values are not always numeric (years such as `199?`; control `ways` such as
  `3 (half4)`), and new attribute values appear over time (feature types,
  display types). Attributes were added over time (`feature` in 0.191, driver flags
  and `softwarelist@tag` in 0.229), so all of them are optional for the parser.
- MAME 0.280 on Windows with a decimal-comma locale (for example Italian)
  sporadically prints `refresh` rates with a comma (`61,419025`), nondeterministically
  across runs; the parser accepts a single comma as the decimal separator for
  decimal values.

#### The `-listsoftware` format

Analyzed on the real output of MAME 0.280 (102 MB; MAME needs 12-66 s to produce
it depending on system load, with continuous output and pauses below 2 s) and on
its DTD:

- The root `<softwarelists>` carries no version; the build is identified by running
  `-version` on the same executable.
- 720 `<softwarelist>` elements (name, description, optional `notes`) containing
  139,122 `<software>` elements: description, year, publisher, optional notes,
  `info` and `sharedfeat` name/value pairs, and `part` elements.
- Software names are unique within a list but not across lists (13,306 names
  appear in more than one list), so a software is identified by list and name.
- `cloneof` references a software of the same list; 4,656 parents are listed after
  their clones. `supported` is `yes` (default), `partial` (2,005) or `no` (39,054).
- A software can have several parts (23,272 do); each part has a name, an
  interface, `feature` name/value pairs, `dataarea` elements (name, size, optional
  `databits`/`endian`) with `rom` entries, and `diskarea` elements with `disk`
  entries. Parts and data areas can be many per software.
- `rom` entries without a name (482) are loading instructions (`loadflag` such as
  `reload` or `fill`, with an offset); `crc`/`sha1` are absent for `nodump` ROMs.
- `info` names can repeat within a software (153 cases).
- Compatibility between machines and software is expressed by MAME through the
  `filter` attribute of a machine's `<softwarelist>` reference (for example `NTSC`,
  `2,3`, `!DVC`) and the `compatibility` shared feature or part feature of software.
  Both are stored verbatim; YMDB does not evaluate them yet.
- Machines of 0.280 reference 733 list names, all 720 provided lists among them;
  13 referenced names are not provided by MAME (for example `sorcerer_quik`).
- The DTD allows part DIP switches and list notes; MAME 0.280 outputs neither.

#### Schema (version 2)

| Table                             | Content                                                                                                                                                                                                                                                                             |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `catalog_metadata`                | Single row: MAME release/revision/build, `mameconfig`, executable path/size/mtime, import time and duration, machine count, status                                                                                                                                                  |
| `machines`                        | One row per entry: name, description, year, manufacturer, source file, `is_bios`/`is_device`/`is_mechanical`/`is_runnable`, `clone_of`, `rom_of`, `sample_of`, sound channels, input summary (players, coins, service, tilt), driver status/emulation/cocktail/save state and flags |
| `machine_roms`                    | Name, BIOS set, size, CRC, SHA-1, merge name, region, offset, dump status, optional                                                                                                                                                                                                 |
| `machine_disks`                   | CHDs: name, SHA-1, merge name, region, index, writable, dump status, optional                                                                                                                                                                                                       |
| `machine_bios_sets`               | BIOS set name, description, default flag                                                                                                                                                                                                                                            |
| `machine_device_refs`             | Devices instantiated by a machine, with instance count                                                                                                                                                                                                                              |
| `machine_samples`                 | Sample names                                                                                                                                                                                                                                                                        |
| `machine_chips`                   | CPU/audio chips: type, name, tag, clock                                                                                                                                                                                                                                             |
| `machine_displays`                | Type, tag, rotation, flip, width, height, refresh rate                                                                                                                                                                                                                              |
| `machine_controls`                | Control type, player, buttons, required buttons, ways                                                                                                                                                                                                                               |
| `machine_features`                | Imperfect/unemulated features: type, status, overall                                                                                                                                                                                                                                |
| `machine_media_devices`           | Media devices (cartridge, floppy, ...): type, tag, interfaces, instance names, mandatory/fixed image                                                                                                                                                                                |
| `machine_media_device_extensions` | Accepted file extensions per media device                                                                                                                                                                                                                                           |
| `machine_software_lists`          | Software lists referenced by a machine: list name, tag, status, filter                                                                                                                                                                                                              |
| `machine_ram_options`             | RAM options: name, size, default flag                                                                                                                                                                                                                                               |

Software list tables (migration 2):

| Table                       | Content                                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------------------- |
| `software_catalog_metadata` | Single row: MAME release/revision, executable path/size/mtime, import time and duration, counts, status |
| `software_lists`            | Name (`UNIQUE`), description, notes                                                                     |
| `software`                  | List name, name (`UNIQUE` per list), clone of, supported, description, year, publisher, notes           |
| `software_info`             | `info` name/value pairs                                                                                 |
| `software_shared_features`  | `sharedfeat` name/value pairs (for example `compatibility`)                                             |
| `software_parts`            | Part name and interface                                                                                 |
| `software_part_features`    | Part `feature` name/value pairs                                                                         |
| `software_data_areas`       | Name, size, data bits, endianness                                                                       |
| `software_roms`             | Name (nullable), size, length, CRC, SHA-1, offset, value, dump status, load flag                        |
| `software_disk_areas`       | Name                                                                                                    |
| `software_disks`            | Name, SHA-1, dump status, writeable                                                                     |

Design rules:

- All tables are `STRICT`. Flags are `0`/`1` with `CHECK` constraints.
- Machines are referenced by their MAME short name (`machines.name`, `UNIQUE`),
  which is stable across MAME versions. `machines` keeps an integer rowid so that a
  future FTS5 index can use it as `content_rowid`.
- `clone_of`, `rom_of` and `machine_device_refs.device_name` are foreign keys
  `DEFERRABLE INITIALLY DEFERRED`, checked at commit, because the XML references
  entries listed later. Child rows reference `machines(name)` with `ON DELETE CASCADE`.
- Child tables have no uniqueness constraints, because MAME can repeat items.
- Values whose set grows with MAME versions are stored as text without `CHECK`.
- Indexes exist on every foreign key and on `clone_of`, `rom_of`,
  `machine_device_refs.device_name` and `machine_software_lists.list_name`.
- Software are identified by `(list_name, name)`. `software.clone_of` is a deferred
  composite foreign key to a software of the same list. Parts, areas, ROMs, disks
  and metadata hang off integer ids with `ON DELETE CASCADE`.
- Machines are linked to software lists by name: `machine_software_lists.list_name`
  = `software_lists.name`, joined through the existing index. It is not a foreign
  key because machines reference lists that MAME does not provide; the summary
  counts them. A list is stored once, however many machines use it, and the
  machine-side attributes (`status` `original`/`compatible`, `filter`, `tag`) stay
  on the relation. A relation does not mean that every software of the list runs
  on every configuration of the machine.
- `info`, `sharedfeat` and `feature` names are free-form metadata defined by each
  list; they are stored as name/value rows, not interpreted as categories.
- Data bits and endianness are stored with the DTD defaults (8, `little`) applied.

Not stored in this version (deferred): for machines, DIP switches and their
locations/values, configuration switches, input ports and analog masks, adjusters,
slots and slot options, conditions, display timing details (pixel clock, blanking),
analog control parameters (minimum, maximum, sensitivity, key delta, reverse), and
the legacy pre-0.191 driver attributes (`color`, `sound`, `graphic`, `protection`).
These matter for launching and configuring machines and will be added with the
launcher. For software, part DIP switches (allowed by the DTD, not output by MAME
0.280). Everything else in `-listsoftware` is stored.

#### Import strategy

The catalog is a separate database file, `catalog.sqlite`, containing only data
derived from MAME. An import:

1. Validates the configured executable with `-version` and records its release,
   revision, path, size and modification time.
2. Creates `catalog.sqlite.partial` from scratch (journaling off, foreign keys on),
   applies the migrations and opens a single transaction.
3. Runs `<mame> -listxml` from the executable's directory through `spawn_streaming`
   (inactivity timeout 120 s; no total timeout, no output size limit) and parses
   its standard output incrementally, inserting each machine with cached prepared
   statements. Progress is an atomic counter of processed machines.
4. After the end of the document, waits for MAME and requires exit code 0.
5. Checks that the number of stored machines equals the number parsed, runs
   `PRAGMA foreign_key_check` (reporting the first violations by name), writes the
   metadata row and commits (deferred foreign keys are enforced here).
6. Closes and syncs the partial file and renames it over `catalog.sqlite` while
   holding the lock that also guards catalog reads.

Failure handling: a parse error, a MAME failure (non-zero exit, inactivity), a
database error, an integrity violation or a cancellation before step 6 discards the
partial file and leaves the previous catalog untouched and readable. A crash leaves
at most a partial file, which the next import deletes. Re-importing (for example
after updating MAME) follows the same steps and replaces the catalog only on
success. Cancellation sets a flag that the parser checks before each machine and the
watchdog uses to kill MAME; it is honored until the final commit.

A **software list import** uses the same replacement mechanism:

1. Validates the configured executable and requires a machine catalog built from
   the same MAME release and revision, read from the same unchanged executable
   (path, size and modification time); otherwise it fails with `catalogMismatch`
   (or `machineCatalogRequired`) before running `-listsoftware`.
2. Copies `catalog.sqlite` to `catalog.sqlite.partial` (under the file lock), empties
   the software list tables of the copy and opens a single transaction. Machine
   data is copied unchanged.
3. Streams `<mame> -listsoftware` through the same `stream_mame_output` as machines
   (same watchdog, inactivity timeout and exit code check), inserting each list and
   software with cached prepared statements. Progress counts software.
4. Checks the stored list and software counts, checks again that the machine
   catalog in the file comes from the same build, runs `PRAGMA foreign_key_check`,
   writes `software_catalog_metadata` and commits.
5. Syncs and renames the file over `catalog.sqlite`, as for machines.

Consequences: an interrupted, failed or cancelled software import keeps both the
machines and the previous software lists. Re-importing software lists replaces them
entirely. Re-importing machines starts from an empty file, so the new catalog has no
software lists until they are imported again from the new build: software lists of
one build are never attached to machines of another. A future "full update" can
write machines and software lists into the same partial file with the same
`CatalogWriter` before a single swap.

Only one import (of either kind) runs at a time. On Windows a file cannot be
renamed over while a handle to it is open: every connection to `catalog.sqlite` is
opened and closed under the store's file lock, which the rename also holds, so the
application never blocks its own replacement. If another program keeps the file
open, the rename fails with a message asking to close it, and the previous catalog
is kept.

User data (future favorites, collections, launch profiles) must not live in
`catalog.sqlite`, which is replaced on every import; it will be stored separately and
reference machines by short name and software by list and short name.

#### MAME versions

A catalog is tied to the MAME build it was imported from (`catalog_metadata`); its
software lists, when present, record their own build in `software_catalog_metadata`
and are only accepted from the same build and executable. The catalog status
compares the catalog, without running MAME, with the configured executable:
`current`, `executableChanged` (same path, different size or modification time,
for example after an in-place update), `differentExecutable`, `executableMissing` or
`mameNotConfigured`. The catalog is never treated as valid for another executable;
the UI asks to import again, and software lists can only be imported when the
status is `current`.

Schema versions: a catalog created by an older release is upgraded in place when it
is read (migrations only add tables, so existing machine data stays valid; version
1 catalogs gain empty software list tables). A catalog from a newer release is
reported as unreadable and must be imported again.

#### Interface

The MAME Catalog panel shows, for machines, whether a catalog exists, its MAME
version, import date and statistics (entries, runnable systems, clones, BIOS sets,
devices, mechanical, ROMs, disks, referenced software lists, database size, import
duration) and the freshness warning; for software lists, whether they are imported,
their MAME version, import date and statistics (lists, software, clones, partially
supported, unsupported, parts, ROMs, disks, referenced lists not provided by MAME,
import duration). Buttons import or reimport machines, import or update software
lists (enabled only when the catalog matches the configured executable) and cancel
the running import. While importing it polls an in-memory status every 500 ms and
shows an indeterminate progress bar with the number of machines or software
processed: totals are not known in advance, so no percentage is shown. Summaries are
read from the database only when an import ends.

#### Limitations

- No catalog browser, search or filters yet; no FTS5 index.
- Machine/software compatibility (`filter` and `compatibility` values) is stored
  but not evaluated.
- Re-importing machines discards the software lists; there is no combined "full
  update" yet.
- Deferred XML data is listed above.
- Imports cannot be cancelled during the final commit and file replacement.

Planned: full catalog update (machines and software lists in one operation),
full-text search and combinable filters, favorites, collections, screenshots,
artwork and metadata.

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
  "mame": { "executablePath": "C:\\Emulators\\MAME\\mame.exe" }
}
```

- `mame` is `null` or absent when no executable is configured. The detected MAME
  version is deliberately not stored; it is read again on every verification.
- Writes are atomic: a temporary file in the same directory is written, synced and
  renamed over `settings.json`.
- A missing file means default settings. An invalid file is reported to the user
  without crashing and is replaced the next time the configuration is saved.
- Access to the file is serialized by a mutex held in Tauri managed state.

Implemented: the catalog database, `catalog.sqlite`, in the platform local data
directory resolved by Tauri (`app_local_data_dir`), because it is large,
machine-specific derived data that should not roam:

| Platform | Location                                                                                   |
| -------- | ------------------------------------------------------------------------------------------ |
| Windows  | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb\catalog.sqlite`                                |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/catalog.sqlite`                 |
| Linux    | `$XDG_DATA_HOME/io.github.davidfriscia.ymdb/catalog.sqlite` (default `~/.local/share/...`) |

Planned: user-configurable ROM, CHD and artwork directories.

### SQLite — Implemented (catalog database)

- SQLite is embedded through `rusqlite` with the `bundled` feature, so every
  platform uses the same SQLite version, without system libraries.
- Explicit SQL, no ORM. Versioned migrations in `catalog/schema.rs`.
- Implemented: the catalog database (see [Catalog](#catalog--implemented-machines-and-software-lists)).
- Planned: a separate user database (favorites, collections, profiles, cache
  metadata) that survives catalog re-imports; full-text search with FTS5.

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
