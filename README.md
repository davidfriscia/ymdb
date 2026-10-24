# Your MAME DB (YMDB)

Your MAME DB (YMDB) is an open source, cross-platform desktop frontend for
[MAME](https://www.mamedev.org/). It aims to catalog, explore, filter, manage and
launch every system supported by the emulator: arcade machines, consoles, home and
personal computers, other devices, and software lists.

> **Status:** early development. MAME configuration, catalog import, search, browsing
> and launching work; missing ROM, CHD and sample files can be downloaded from your own
> SFTP repositories. See [Current status](#current-status).

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
- Optional remote repositories (SFTP now, HTTPS planned) to download missing files on
  demand.
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

## MAME installations

YMDB uses MAME installations that already exist on your computer. It never downloads
or installs MAME, and it does not require administrator privileges. Portable
installations and custom builds are supported, as long as they support the
`-version` option and print the standard MAME version line. This was verified with
MAME 0.240, 0.252 and 0.280; older releases such as MAME 0.162 do not support it.

You can register any number of installations, including several builds of the same
version: each one is identified by an internal id, never by its version. Each
installation has its own catalog and its own launch options; media sources and
interface preferences are shared by all of them. One installation is **active**: the
catalog browser shows its catalog and new launches use its executable.

The **MAME** page has a navigation on the left and the selected section on the right.
The navigation starts with **Add Installation**, followed by one group per registered
installation (its name, version, whether a catalog is imported, and an **Active**
marker on the active one). Each group expands to four sections of that installation:
**Status**, **Catalog**, **ROM Manager** and **Launch Options**. The page opens on the
Status of the active installation; choosing a section never changes the active
installation. Sections keep their unsaved changes while you look at other sections.
Arrow keys, Home and End move through the navigation; on narrow windows it collapses
behind a toggle above the content.

- **Add Installation → Add installation…** opens the native file picker. Choose the MAME program file
  (`mame.exe` on Windows, usually `mame` on macOS and Linux). YMDB validates it and
  registers it only if validation succeeds. Adding an installation never changes the
  others, and its catalog is not imported until you ask for it. The same executable
  cannot be registered twice (paths are compared with the operating system's rules:
  case-insensitively on Windows and macOS, after resolving links). The new
  installation is shown right away.
- **Add Installation → Detect installations** looks for MAME in the directories listed in `PATH` and in
  a few standard installation directories (non-recursively), then validates each
  candidate. Detection never changes the registered installations: choose **Add** to
  register a detected executable.
- **Status** shows the executable, version, verification status and whether the
  installation is active, with these actions:
  - **Set as active** makes it the active installation (MAME is not started).
  - **Verify** runs its executable again and shows the detected version, the
    validation status, the time of the check and the raw output of the version
    command. A valid result updates the recorded version.
  - **Rename…** gives it a name; an empty name shows the version again.
  - **Remove…** asks for confirmation and unregisters it. You choose whether to also
    delete its imported catalog (checked by default: it is derived data that can be
    imported again) and its launch options (unchecked by default). The MAME
    executable and MAME's own files (ROMs, CHDs, artwork, INI files) are never
    deleted. Removal is refused while a catalog import of the installation or a MAME
    session started from it is running. When the active installation is removed,
    the first remaining installation in registration order becomes active.
- **Catalog** imports and shows that installation's catalog (see below).
- **ROM Manager** and **Launch Options** are described below.

The executable of an installation never changes. To use another executable (for
example a new MAME release in another folder), add it as a new installation. If an
executable is updated in place, the installation shows that it changed: verify it
and import its catalog again.

When YMDB starts, it shows the registered installations without running MAME, and
warns about executables that no longer exist or have changed.

### Validation

Validation runs `<executable> -version` directly (never through a shell), with a
20-second timeout, and accepts the executable only if it exits with code 0 and
prints a MAME version line such as `0.280 (mame0280)`. A program that runs
successfully but prints anything else is not accepted. Each failure is reported with
a specific reason: file not found, not a file, not executable, failed to start, timed
out, unexpected exit code, or unrecognized output. See
[ARCHITECTURE.md](ARCHITECTURE.md#mame-integration--implemented) for the exact rules.

### Where installations are stored

The registered installations and the active one are stored in `installations.json`
in the application configuration directory:

| Platform | Location                                                                       |
| -------- | ------------------------------------------------------------------------------ |
| Windows  | `%APPDATA%\io.github.davidfriscia.ymdb\installations.json`                     |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/installations.json` |
| Linux    | `~/.config/io.github.davidfriscia.ymdb/installations.json`                     |

Each installation's launch options are in `installations/<id>/launch.json` next to it
(with its ROM Manager settings in `remote-sources.json`),
and its catalog in `installations/<id>/catalog.sqlite` in the local data directory
(see below), with the copy of the support file classifications for that catalog in
`classifications.sqlite` (see [Support files](#support-files-and-saved-filters)).

### Upgrading from a single installation

Earlier versions of YMDB managed one executable (`settings.json`), one catalog and one
`launch.json`. On the first start, YMDB registers that executable as an installation
and gives it the existing catalog (moved, not imported again) and launch options
(copied). The old `settings.json` and `launch.json` are left untouched as a backup.
MAME is not run: the version comes from the catalog. If the catalog was imported
from another executable than the configured one, it is given to a separate
installation for that executable. If the upgrade cannot complete (for example because
another program has the catalog open), nothing is deleted and YMDB retries on the next
attempt. See [ARCHITECTURE.md](ARCHITECTURE.md#migration-from-the-single-installation-layout).

## MAME catalog

Every installation has its own catalog. The **Catalog of …** panel of the **MAME**
page imports, for the installation selected there, the machines and the software
lists known to its MAME build:

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
  partial import. One import runs at a time; it stays attached to its installation
  even if you change the active installation meanwhile.
- The panel shows the MAME version, import date and a summary for machines and for
  software lists.
- If the installation's executable changes (the same file updated), the panel warns
  that the catalog no longer matches it. Reimport to update it. Reimporting machines
  starts a new catalog without software lists; import the software lists again
  afterwards.

Imports are atomic: a new catalog is built in a separate file and replaces the
previous one only when the import succeeds. If MAME fails, the output cannot be
parsed, the database cannot be written, or the import is cancelled, the previous
catalog (machines and software lists) stays available. Importing one installation
never changes another installation's catalog. If another program keeps the catalog
file open, Windows prevents its replacement: the import fails with a message asking to
close that program, and the previous catalog is kept.

Catalogs are stored in the application local data directory, one per installation:

| Platform | Location                                                                                      |
| -------- | --------------------------------------------------------------------------------------------- |
| Windows  | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb\installations\<id>\catalog.sqlite`                |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb/installations/<id>/catalog.sqlite` |
| Linux    | `~/.local/share/io.github.davidfriscia.ymdb/installations/<id>/catalog.sqlite`                |

A catalog contains only data derived from MAME and can be deleted at any time; it is
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

The **Catalog** page (the start page) browses the imported machines or software of
the active MAME installation:

- Choose **Machines** or **Software**, type a text (the search starts when you pause
  typing, or press Enter or **Search**), pick a sort order and direction.
- The **Filters** panel lists, with counts, the manufacturers, years, driver and
  emulation statuses and software lists of the machines (publishers, lists, years,
  support statuses and part interfaces for software), plus parent/clone, BIOS,
  device, mechanical, runnable and "declares ROMs/CHDs" filters. With imported
  [support files](#support-files-and-saved-filters), machines can also be filtered by
  **Genre**, **Category** (a tree: a category includes its subcategories), **Series**
  and **Version Added** (a From/To range in MAME version order, or exact versions).
  Values of one filter are alternatives; different filters combine. Long lists scroll
  on their own, have a value search, **Show more** and **Browse all**, a window with
  every value, a search, the number of selected values and **Clear selection**:
  **Apply** changes the filter, **Cancel** (or Escape) leaves it as it was. Active filters appear as chips above the results; remove
  them one by one or with **Clear all filters**. The **Filters** button hides the
  panel to give the results the whole width; the filters stay active.
- The **View** button opens the **View Options** panel on the right. Its **MAME
  Version** menu lists the registered installations (with their folder when two have
  the same name, and "no catalog" for those not imported yet) and switches the
  active installation without importing anything: the results, filters and details
  then come from that installation's catalog, and new launches use its executable.
  The search text, filters and sort order are kept and the results return to the
  first page; filter values that do not exist in the new catalog match nothing
  (a notice offers to clear them). Open details are closed, because they belong to
  the previous catalog. Results of two catalogs are never shown together. **View mode**
  switches between **Classic Cards** (text cards; **Card size**) and **Media Grid**
  (artwork tiles; **Tile size**, **Artwork** category, **Image fit**, and whether to
  show the title, the technical name and the status badge). Changing the view never
  changes the search, filters, sort or page. Both side panels can be hidden
  independently; the results then use the whole width. The number of columns adapts
  to the window. Pages hold 25 to 200 results. Only the results scroll: the
  toolbar stays at the top, each side panel scrolls on its own and the pagination
  bar (**Previous**/**Next**, page numbers, "Page N of M", **Per page** and **Go to**)
  stays visible below the results. A new page, search, filter or sort shows the
  results from the top, without moving the filters.
- **Saved Filters**, in View Options, saves the current search and filters under a
  name and loads them again (see [Saved filters](#saved-filters)).
- Click a card or tile (or press Enter on it) to open its detail: machines show general data, relations, emulation
  status, ROMs, CHDs, devices, displays, controls, chips and software lists;
  software shows its list, parts, ROMs and disks and the machines declaring its list;
  software lists show their software and machines. Parents, clones, devices and
  software lists are links. Machine and software details share one layout: a header
  with **Back to results** and **Previous** / position / **Next**; the artwork (see
  [Artwork and screenshots](#artwork-and-screenshots)); a compact bar with the title,
  the short information, the download actions, **Play** and the profile; the status
  of the last launch, when there is one; the collapsed **Advanced** launch options;
  and the technical sections. With support files, the Overview of a machine shows its
  **Classification** (genre, category, series, version added); each value, and each
  level of a category, shows the catalog filtered by it. **Back** returns to the previous detail and **Back to
  results** to the results, with the search, filters, sort, view, page size, page and
  scroll position unchanged; **Previous**/**Next** move through the current page.
- ROMs, CHDs and disks are what MAME declares; YMDB does not check files on disk yet,
  and a software list declared by a machine does not guarantee compatibility.

The **MAME** page holds the MAME installations and everything configured per
installation (see above). The **Settings** page uses the same layout for the global
settings: **Appearance** (the **Light**, default, or **Dark** theme, applied
immediately to the whole application), **Media Sources**, **Remote Connections**,
**Favorites** (export and import, see [Favorites](#favorites)), **Support Files** (see
[Support files](#support-files-and-saved-filters)), **About** (product name and version) and **Diagnostics** (a backend connection check).
It opens on Appearance and keeps the chosen section while you visit other pages.
Theme, panel visibility, view mode, the
options of each view and the command line preview are saved in the application's
web storage, separately from the MAME, launch and media configuration; they are the
same for every MAME installation. The active installation is saved with the
installations, not with these preferences.

## Favorites

Every card of the catalog, in **Classic Cards** and **Media Grid**, has a bookmark in
its top right corner: faint while the machine or software is not a favorite, solid
when it is. Click it (or focus it and press Enter or Space) to add or remove the
favorite; the card does not open. The same bookmark sits next to the title of a
machine or software detail page. A change shows at once; if it cannot be saved, the
bookmark returns to its previous state and an error explains why.

Favorites belong to YMDB, not to a MAME installation. A machine is identified by its
MAME short name (`pacman`), a software by its software list and short name
(`nes`/`smb`), never by a catalog id, a description or a ROM file name. So:

- a favorite added with MAME 0.252 is a favorite with MAME 0.280 too, when that
  catalog has the same short name;
- a favorite missing from the active catalog is kept and shows up again with an
  installation that has it;
- switching, updating, re-importing or removing an installation never changes the
  favorites, and a renamed short name is a different machine (YMDB never matches
  titles by description).

**Favorites only**, in the **View Options** panel, shows only the favorites of the
active catalog, for machines or software. It combines with the search text, the
filters, the sort order and the pages: counts, facets and pages cover the matching
favorites only, and a chip above the results turns it off. Removing a favorite
while it is on removes it from the results. The option is an interface preference;
it never changes the favorites themselves.

**Settings › Favorites** shows how many favorites you have and exports them to a JSON
file, or imports such a file. An import first shows how many entries are new,
already present or duplicated, then **Merge** (add the new ones, remove nothing) or
**Replace** (keep exactly those of the file, after a confirmation that says how many
will be removed). Imports are checked completely and applied at once: an invalid
file changes nothing. The file holds only MAME names, so it works on another
computer and with any MAME version:

```json
{
  "format": "ymdb-lists",
  "version": 1,
  "lists": [
    {
      "id": "favorites",
      "kind": "system",
      "entries": [
        { "kind": "machine", "name": "1942" },
        { "kind": "software", "listName": "nes", "name": "smb" }
      ]
    }
  ]
}
```

Favorites are saved in `lists.json`, in the configuration directory (see
[Where installations are stored](#where-installations-are-stored)).

## Support files and saved filters

MAME itself does not classify machines as shooters or platformers. Third-party
_support files_ do (for example those of [progettoSNAPS](https://www.progettosnaps.net/)).
YMDB imports five of them, from files you download yourself:

| File          | Provides                                                         |
| ------------- | ---------------------------------------------------------------- |
| `catver.ini`  | **Category** (`[Category]`) and **Version Added** (`[VerAdded]`) |
| `genre.ini`   | **Genre**                                                        |
| `catlist.ini` | **Category** (an alternative to `catver.ini`)                    |
| `version.ini` | **Version Added** (an alternative to `catver.ini`)               |
| `series.ini`  | **Series** (a machine can belong to several series)              |

**Settings › Support Files** imports them: **Import support file…** opens the file
picker, YMDB recognizes the file by its header and structure (not only its name),
checks it and shows a summary (declared MAME version, machines, values, warnings such
as invalid lines or duplicates) before anything is stored; importing a file of a
format that is already imported replaces it after confirmation. Each imported file
shows its name, declared version, import date, machines, values and whether it is
used; it can be reimported or removed (after confirmation; the file on disk is never
modified or deleted). Each classification uses one source: the first file imported
for it, or the one selected in **Classifications**. Removing the source in use leaves
the classification without one until you select or import another; YMDB never
switches source on its own. Nothing is downloaded or updated automatically.

Support files are shared by every MAME installation. Each catalog keeps its own copy
of the classifications of its machines, matched by short name, and brings it up to
date the next time the catalog is opened after a change (or after the catalog is
imported again); machines a catalog does not have are simply ignored. Imported data
is stored in `support-files.sqlite` (local data directory) and the copy of each
catalog in `installations/<id>/classifications.sqlite`, next to the catalog.

In the **Filters** panel, Genre, Category, Series and Version Added appear for
machines when the catalog has them; otherwise a note leads to Settings. Values of one
classification are alternatives, different classifications and the other filters
combine, and the counts, sorting, pages and **Favorites only** work as for every other
filter.

### Saved filters

**Saved Filters**, in the **View Options** panel, keeps named search and filter
configurations: **Save current filters** stores the catalog type, the search text,
every filter (including classifications and version ranges) and **Favorites only**,
not the view (renderer, sizes, panels, theme) or the sort. Clicking a saved filter
applies it at once and returns to the first page. After a change the saved filter is
marked **Modified** until you **Update** it; it never changes on its own. Saved
filters can be renamed and deleted, belong to YMDB (not to an installation) and apply
to the catalog of the active installation. Criteria that match nothing in that
catalog (a series it does not have, or a classification without source) are kept and
reported above the results: they narrow the results, they are never dropped or
replaced. Saved filters are stored in `saved-filters.json` in the configuration
directory; filters saved by a newer YMDB are kept unchanged but cannot be loaded.

## Artwork and screenshots

The Media Grid shows images from directories you already have, for example an
artwork collection or MAME's own `snap` folder. Configure them in **Settings → Media
Sources**: each category (Snapshots, Title screens, Flyers, Cabinets, Marquees) takes
one or more directories, searched from top to bottom. Local disks and network shares
mounted by the operating system work; YMDB only reads them and never copies or
changes your files.

Images are found by name:

- machines: `pacman.png` (also `.jpg`, `.jpeg`, `.webp`) in the directory, or
  the numbered screenshots saved by MAME in `pacman/` (`0000.png`, `0001.png`, …);
- software: `<list>/<software>.png`, for example `nes/smb.png`, so a directory of
  software list snapshots (such as `snap_SL`) can be added to Snapshots.

When a title has no image, its tile shows a placeholder: YMDB never shows the image
of another title, not even of the parent of a clone.

The detail page of a machine or software shows a large carousel of its images with
the categories that have images in a two-column grid on its right (Snapshots, Title screens, Flyers,
Cabinets, Marquees; categories without an image are left out). The first category
is selected; click another to browse its images only. A category can hold several
images: the image of each configured directory and, for machines, every screenshot
MAME saved. Move with the arrow buttons, the dots or the arrow keys. Images keep
their proportions (a flyer stays vertical, a marquee wide). Click the image to
enlarge it: the arrow keys (or the arrow buttons) move between the images of the
category, **Esc** or **Close** closes it. When there is no image at all, a one-line
note replaces the gallery. On narrow windows the categories are shown as a row
below the carousel. Zipped collections are not supported yet. Media sources are
shared by all MAME installations: switching the MAME version uses the same
directories (a title missing from one version's catalog is simply not listed).

## Remote repositories

YMDB can be told where your ROM, CHD and sample collections live on another machine,
for example a NAS, so that the ROM Manager can download the files a launch is missing.
Remote repositories are optional; without them YMDB works as before, fully offline.

### Remote connections

**Settings → Remote Connections** lists the connections, shared by every MAME
installation, below the **Add connection…** button. A connection has a name and a provider; the only provider is **SFTP**
(over SSH):

- host name or IP address, port (22 by default) and user name;
- a **private key file**, chosen with the native file picker. Only its path is saved:
  the key is never copied. OpenSSH (ed25519, ECDSA, RSA), PEM/PKCS#8 and PuTTY keys
  work. Passwords are not supported and no SSH agent is needed;
- keys protected by a **passphrase** work: YMDB asks for the passphrase when it
  connects and never saves it, so it is asked again for later connections;
- **host key verification**, always on. With "Approve the server identity in YMDB",
  the first connection shows the server's fingerprint (`SHA256:…`, as printed by
  `ssh-keygen -lf`) and asks you to approve it; YMDB then remembers it. With "Use
  OpenSSH known_hosts, then approval in YMDB", the identity in your
  `~/.ssh/known_hosts` is used when present (YMDB only reads that file). An identity
  that changes is refused, never approved automatically; if the change is expected
  (for example a reinstalled server), forget the approved identity and approve the
  new one.

**Test Connection** really connects: it resolves the host, performs the SSH handshake,
verifies the server identity, authenticates with the key and opens SFTP. **Browse…**
lists remote directories (read only) to check your paths. A connection used by an
installation's ROM Manager cannot be removed; the message lists where it is used.
Connections are saved in `remote-connections.json` in the configuration directory.

### ROM Manager settings of an installation

The **ROM Manager** section of each installation on the MAME page configures, for that
installation, five independent categories: Machine ROMs, Machine CHDs, Software List ROMs,
Software List CHDs and Samples. Configure only the ones you need. Each one has:

- **Enabled**;
- **Remote connection** (one of the connections above; one connection can serve
  several categories and installations);
- **Remote base path**, an absolute path on the server such as
  `/volume1/mame/0280/roms` (any layout of your choice);
- **Collection organization**: Merged, Split or Non-merged for ROMs and CHDs, ZIP
  archives or Directories for samples;
- **Local destination**: one of the directories MAME already uses (`rompath` for ROMs
  and CHDs, `samplepath` for samples, read from `mame.ini` or from the installation's
  global launch options; choose one you can write to), or a **custom directory**.

A custom directory is added to MAME's search path when YMDB launches MAME: YMDB passes
`-rompath`/`-samplepath` with the directories that apply to the launch (from your
launch options or from `mame.ini`) followed by the custom directory, so nothing that
MAME already finds is lost. The command line preview shows these options as coming
from the ROM Manager. `mame.ini` is never modified. Two installations may use the same
local directory; YMDB warns you, because files with the same name can differ between
MAME versions.

The settings are saved per installation in `installations/<id>/remote-sources.json`
(configuration directory) and never shared between installations.

### Downloading missing files (ROM Manager)

The launch bar of a machine or software has two more buttons, shown as icons (hover
them for a description):

- **Prepare Download** builds a _download plan_ for the launch as it is configured
  (installation, machine, software, mounted parts, profile) and shows it. Nothing is
  downloaded yet.
- **Download & Play** builds the same plan; after you confirm, YMDB downloads the
  available missing files and then starts MAME with the same installation and options.

The plan, the progress and the result are shown in a window over the page. Closing
it never cancels a download: while files are transferred, **Hide** (or **Esc**)
only hides the window, and a **Downloading …%** button next to the actions shows it
again. Use **Cancel download** to stop a transfer.

The plan works with **whole sets**, never with the ROMs inside archives. From the
catalog, YMDB lists the files the launch may need:

- the machine's ROM archive, its parents and BIOS sets (following `cloneof` and
  `romof`, also indirectly) and the archives of its devices that have ROMs (also
  devices of devices);
- the machine's disks (`<set>/<disk>.chd`) and those of its parents;
- its samples (`<sample set>.zip`, or one `.wav` file per sample for collections
  organized as directories);
- for a software: the software archive (`<list>/<software>.zip`), its parent's
  archive, the disks of the mounted parts (`<list>/<software>/<disk>.chd`) and those
  of the parent, plus everything the machine running it needs.

This is deliberately conservative: a parent or BIOS archive may be downloaded even when
the collection would not strictly need it. For each file the plan shows its category,
why it is included, whether MAME already finds it locally (in the directories MAME
will search for this launch: `rompath`/`samplepath` from your launch options or
`mame.ini`, plus the ROM Manager destinations), whether it exists in the remote
repository (one lookup per file, no listing of the whole server), its size and where
it would be written.

Missing files never block the plan: a file that is not on the server is a **warning**,
and the files that exist are still downloaded. Some absences are expected and shown as
information only (for example the archive of a clone in a merged collection). A
category without a remote source, a lookup that fails for a technical reason (timeout,
refused login, unapproved server identity) or a file name that cannot be used locally
is reported as such, never as "not found". When a key needs a passphrase or a server
identity must be approved, the plan asks for it and checks again; passphrases are used
for that check and that download only, never saved.

Downloads run in the background, one file at a time, with progress and **Cancel
download**. Each file is written to a temporary file next to its destination
(`<name>.ymdb-download`), its size is checked and only then is it given its final
name. **Existing files are never replaced**, remote files are never modified, and a
cancelled or failed transfer leaves no partial file. You can leave the page during a
download; coming back shows it again. Failed transfers can be retried (completed files
are kept). If YMDB is closed during a download, the transfer stops; its temporary file
is replaced by the next attempt.

The result says whether the download completed, completed with warnings, failed or was
cancelled. **Download & Play** starts MAME only when no transfer failed; candidate files
that were not available do not prevent it: MAME decides whether the files present are
enough. A present or downloaded file is not verified against MAME's data, so YMDB never
claims that a game will work. The normal **Play** button is unchanged and never
requires the ROM Manager.

## Launching MAME

YMDB starts the executable of the active MAME installation for a machine or a
software, with that installation's launch options. It needs a catalog imported from
that same executable (reimport it after updating MAME in place). YMDB does not check that ROMs, CHDs or software media are present: when files
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
- **Play**, the **Profile**, **Prepare Download** and **Download & Play** are always
  visible in the launch bar below the artwork. The **Advanced** section (collapsed when a detail opens; its counter shows
  the number of options set for the title) holds **Options for this machine** or
  **Options for this software** and **Command line**, which shows the exact command
  that **Play** runs (executable, working directory and the origin of every option).
  Closing the section keeps unsaved options.
- While MAME runs, the detail page shows the session and the installation it was
  started from; **Stop** terminates MAME after a confirmation (unsaved progress and
  NVRAM changes are lost). Closing YMDB or switching the MAME version never closes
  MAME, and the same machine can run from two installations at once.

### Launch options

YMDB passes only the options you set; everything else follows MAME's own
configuration, which YMDB never modifies. Each MAME installation has its own options,
profiles and overrides (in the **Launch Options** section of each installation on
the MAME page), because option, device and machine names can change between
versions; they are never shared or copied between installations. Options are set at
four levels, the later ones taking precedence:

1. **Global options** (Launch Options of the installation).
2. **Launch profiles** (Launch Options of the installation): named sets of options, chosen with the
   **Profile** selector when launching; the default profile applies otherwise.
3. **Options for this machine** (machine detail page).
4. **Options for this software** (software detail page).

The available options cover display, performance, sound, search paths and output
directories, BIOS and RAM size and session options (for example `seconds_to_run`).
Values are checked before saving. They are saved in the installation's `launch.json`
(`installations/<id>/launch.json` in the configuration directory). When the ROM Manager
uses custom local directories, YMDB extends `-rompath`/`-samplepath` after these levels
(see [Remote repositories](#remote-repositories)).

### Controller profiles

MAME keeps configuring inputs in its own menus and saving them; YMDB never edits
MAME's `cfg` files and has no button editor. Three things have similar names:

- a **launch profile** is a set of launch options (see above);
- a **controller profile** is an input configuration: with it, MAME reads and saves
  its configuration (`default.cfg`, `<machine>.cfg`) in a directory of YMDB for that
  profile and MAME installation, optionally starting from a MAME controller
  configuration file (`.cfg`, used as the baseline: `-ctrlr`);
- a **controller** is the logical identity of one of your devices (name, kind, MAME
  device class, optional device ID and provider).

The built-in **MAME configuration** controller profile, the initial default, passes
nothing to MAME and keeps MAME's own configuration, exactly as before.

**Settings → Controllers** manages controller profiles (create, empty or from a
controller file; rename; import, replace or remove the baseline file; delete when no
saved assignment uses it), controllers, the default controller profile and the
saved assignments of machines and software.

On a detail page, the **Controls** selector of the launch bar shows the controller
profile that applies and where it comes from (for example "Arcade Six Buttons —
specific to this machine"). Choosing another profile applies to the launches from
that page only; **Save as preference** saves it for the machine, the software or the
software on the selected machine. **Advanced → Controls** shows each saved level with
**Reset to inherited**. Precedence, from the weakest: default, machine, software,
software on a given machine, then the temporary selection.

To assign buttons, use **Advanced → Controls → Configure Controls in MAME**: YMDB
starts the machine or software with the controller profile, and you open MAME's menu
(normally **Tab**) → **Input Settings** → **Input Assignments (this system)** for the
current machine, or **Input Assignments (general)** for every machine. MAME saves the
changes when you exit it normally; stopping MAME from YMDB discards them. The next
launch with the same profile finds them; other profiles are not affected.

Current limits: controllers are not bound to physical devices yet (no device
detection or `mapdevice`), so player assignments are stored preferences only. MAME's
configuration files also hold DIP switches, slot, mixer and video settings, so these
are per controller profile and MAME installation too: a new profile starts from
MAME's defaults. Two sessions using the same profile get a warning, since the last
one to exit keeps its general settings.

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
- Multi-version MAME installation management: any number of installations (also of
  the same version) added through the native file picker or detection, validated
  through `-version`, verified, renamed, removed and selected as active from the MAME
  page or the catalog's **MAME Version** menu; each with its own catalog and launch
  options, and an automatic upgrade of the single-installation configuration (see
  [MAME installations](#mame-installations)).
- Installations persisted as JSON in the platform configuration directory.
- MAME catalog: streaming import of machines (`-listxml`) and software lists
  (`-listsoftware`) into SQLite with versioned schema and migrations, version
  consistency checks, atomic replacement, cancellation, progress and summaries
  (see [MAME catalog](#mame-catalog)).
- Catalog search engine: FTS5 full-text search, combinable filters, facets with
  counts, sorting and pagination (see [Catalog search](#catalog-search)).
- Catalog browser with filters, facets, card results, a persistent pagination bar and machine,
  software and software list detail views (see [Browsing the catalog](#browsing-the-catalog)).
- Global favorites for machines and software: bookmarks on every card and detail
  page, a **Favorites only** view applied by the search engine, and JSON export and
  import with merge or replace (see [Favorites](#favorites)).
- MAME support files (catver.ini, genre.ini, catlist.ini, version.ini, series.ini):
  manual import into a global repository, one projection per catalog synchronized when
  the catalog is opened, Genre, Category, Series and Version Added filters and facets,
  and saved filter presets (see
  [Support files and saved filters](#support-files-and-saved-filters)).
- Media Grid view and detail page galleries (with a lightbox) of local artwork and
  screenshots from configurable media
  sources (see [Artwork and screenshots](#artwork-and-screenshots)).
- MAME launcher: machines and software, with machine selection, part assignment,
  command line preview, global options, launch profiles, machine and software
  options per installation, and session monitoring across installations (see
  [Launching MAME](#launching-mame)).
- Controller profiles: controllers, controller profiles with an isolated MAME
  configuration directory per installation and an optional imported controller file,
  assignments by machine and software with inheritance, a temporary Controls
  selection at launch and a guided configuration in MAME's own menus (see
  [Controller profiles](#controller-profiles)).
- Remote repositories: global remote connections (SFTP with private keys, passphrases
  and host key verification), connection test and remote browsing, and per-installation
  ROM Manager settings for five categories with local destinations added to the MAME
  search paths at launch (see [Remote repositories](#remote-repositories)).
- ROM Manager downloads: set-based download plans (parents, BIOS, devices, machine and
  software list ROMs and CHDs, samples) with local and remote availability, warnings,
  sequential downloads with progress, cancellation and retry, and Download & Play (see
  [Downloading missing files](#downloading-missing-files-rom-manager)).
- A `check_health` Tauri command that verifies frontend-backend communication and
  reports the application status, operating system, architecture and version.
- Linting, formatting and test tooling for both the frontend and the Rust core.
- Continuous integration on Windows, macOS and Linux.

Not implemented yet: everything else, including ROM auditing (verifying the content of
archives), an HTTPS provider, input device detection and binding (`mapdevice`), and
artwork download. See
[ARCHITECTURE.md](ARCHITECTURE.md).

No releases have been published yet.

## License

Your MAME DB is licensed under the [Apache License, Version 2.0](LICENSE).

Third-party dependencies are distributed under their own licenses (mostly MIT and/or
Apache-2.0, plus BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0 for a few Rust crates).
Binary distributions must include the license notices of bundled dependencies.

MAME is a trademark of its respective owners. YMDB is an independent project and is
not affiliated with or endorsed by the MAME team.
