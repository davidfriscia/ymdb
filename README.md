# YMDB — Your MAME DB

Your MAME DB (YMDB) is an open source desktop application for Windows, macOS and
Linux that helps you browse, search, organize and launch everything your
[MAME](https://www.mamedev.org/) installations support: arcade machines, consoles,
computers, other devices and their software lists.

YMDB is a frontend: it does not emulate anything and does not include MAME, ROMs,
BIOS files, CHDs or media. MAME runs the games; YMDB helps you find them and start
them.

> **Status:** early development (version 0.1.0). The features below are implemented;
> no prebuilt release has been published yet, so YMDB is currently built from source.

## Why YMDB?

A recent MAME release knows about tens of thousands of machines and over a hundred
thousand software list entries. YMDB turns that information into a catalog you can
actually use:

- **Navigate** a very large catalog quickly, with full-text search, filters with
  counts and paginated results.
- **Organize** it your way with favorites, custom lists, saved filters and community
  classifications (genre, category, series).
- **Enjoy** it visually with your own artwork collections.
- **Launch** machines and software with the right MAME options, input configuration
  and players.
- **Combine** local and remote resources: keep working offline, or fetch missing
  files from your own server when you need them.

## Features

- **Multiple MAME installations** side by side, each validated, with its own catalog
  and launch options; switch between versions at any time.
- **Catalog** of machines and software lists imported from MAME into SQLite, with
  full-text search, facets, filters, sorting and machine, software and software list
  details.
- **Classic Cards** and **Media Grid** views, image galleries from your local media
  directories, light and dark themes.
- **Launcher** with launch options, profiles, per-machine and per-software options,
  an exact command line preview and session tracking.
- **Controllers**: controller profiles, device detection, player assignments mapped
  through MAME's own `mapdevice` configuration.
- **Favorites, custom lists and saved filters**, including the import of frontend
  folder `.ini` files.
- **Support files** (`catver.ini`, `genre.ini`, `catlist.ini`, `version.ini`,
  `series.ini`) for Genre, Category, Series and Version Added filters.
- **Personal remote repository over SSH/SFTP**: the ROM Manager downloads missing ROM,
  CHD and sample files on demand, with **Download & Play**.
- **Backup and restore** of all application data, and window state persistence.

Not implemented: ROM auditing, providers other than SFTP, and any download of artwork
or support files. See the [documentation](docs/index.md) for details and limits.

## The MAME ecosystem

YMDB is a companion frontend for the existing MAME ecosystem, not a replacement for
it. It focuses on navigation, search, cataloging, organization, access to content
and launching titles. It does not replace MAME as an emulator, the documentation and
preservation work behind it, the projects that produce catalogs and media resources,
or the tools and communities that maintain the ecosystem.

YMDB is an independent project. It is not an official MAME project and is not
affiliated with or endorsed by the MAME team or any of the projects below.

### MAME

[MAME](https://www.mamedev.org/) is the foundation YMDB builds on. Everything YMDB
knows about machines, devices, ROMs, disks and software lists comes from the data
MAME itself publishes (`-listxml`, `-listsoftware`), the result of decades of
development, documentation and preservation work by the MAME team and its
contributors. YMDB also relies on MAME for everything that happens when a title runs,
including input configuration.

### Progetto-SNAPS

[Progetto-SNAPS](https://www.progettosnaps.net/) produces and maintains artwork,
snapshots and support files for the MAME ecosystem. Its image packs make the Media
Grid and the detail galleries come alive (YMDB's image naming conventions were
checked against its packs), and its support files and folder `.ini` files can be
imported by YMDB as genre, category, series and version classifications and as
custom lists. YMDB does not distribute any of these resources and does not
require them: you download them yourself from Progetto-SNAPS and point YMDB to them.

### Rights and content

YMDB's source code is licensed under Apache-2.0. MAME, the media, the support files,
ROMs, CHDs and any other third-party content are separate works under their own
terms; YMDB acquires no rights over them. Use only content you are entitled to use.

## Getting started

1. **Get YMDB.** Build it from source (Node.js 22, pnpm, Rust and the Tauri
   prerequisites):

   ```sh
   git clone https://github.com/davidfriscia/ymdb.git
   cd ymdb
   pnpm install
   pnpm tauri build
   ```

2. **Get MAME** from <https://www.mamedev.org/> and put your ROM sets in its `roms`
   directory.
3. **Register MAME** on the **MAME** page with **Add installation…** or **Detect
   installations**.
4. **Import the catalog**: in the installation's **Catalog** section, choose
   **Import catalog**, then **Import software lists**.
5. **Configure media** (optional) in **Settings → Media Sources**.
6. **Play**: find a title on the **Catalog** page, open it and choose **Play**.

The complete tutorial, including support files, a personal remote repository and
backups, is in [Getting started](docs/getting-started.md).

## Documentation

- [Documentation index](docs/index.md)
- [Getting started](docs/getting-started.md) ·
  [Installation](docs/installation.md) ·
  [MAME installations](docs/mame-installations.md) ·
  [Catalog](docs/catalog.md) ·
  [Media](docs/media.md) ·
  [Launching games](docs/launcher.md)
- [Remote repository](docs/remote-repository.md) ·
  [Controllers](docs/controllers.md) ·
  [Favorites, lists and saved filters](docs/lists-and-favorites.md) ·
  [Support files](docs/support-files.md) ·
  [Backup and restore](docs/backup-and-restore.md) ·
  [Troubleshooting](docs/troubleshooting.md)
- For developers: [Development](docs/development.md) ·
  [Architecture overview](docs/architecture.md) ·
  [ARCHITECTURE.md](ARCHITECTURE.md)

## Technology stack

| Area             | Technology                                       |
| ---------------- | ------------------------------------------------ |
| Desktop shell    | [Tauri 2](https://tauri.app/)                    |
| Application core | Rust (toolchain pinned in `rust-toolchain.toml`) |
| Frontend         | React, TypeScript (strict), Vite                 |
| Storage          | SQLite (bundled), JSON files                     |
| Package manager  | pnpm                                             |
| Quality          | ESLint, Prettier, rustfmt, Clippy                |
| Testing          | Vitest, React Testing Library, `cargo test`      |

Every change is checked and compiled on Windows, macOS and Linux by
[continuous integration](.github/workflows/ci.yml).

## Contributing

Contributions are welcome. Read [Development](docs/development.md) for the setup, the
checks (`pnpm check` must pass) and the Git conventions, and
[ARCHITECTURE.md](ARCHITECTURE.md) and [CLAUDE.md](CLAUDE.md) for the design rules
the code follows.

## License

Your MAME DB is licensed under the [Apache License, Version 2.0](LICENSE).

Third-party dependencies are distributed under their own licenses (mostly MIT and/or
Apache-2.0, plus BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0 for a few Rust crates).
Binary distributions must include the license notices of bundled dependencies.

MAME is a trademark of its respective owners.
