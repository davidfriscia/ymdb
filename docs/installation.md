# Installing YMDB

This page explains how to get YMDB running on your computer and where it keeps its
data. For the complete first-time setup (MAME, catalog, media, remote repository),
follow [Getting started](getting-started.md).

## Supported operating systems

YMDB targets **Windows**, **macOS** and **Linux**. Every change is checked and
compiled on all three platforms by continuous integration (see
[Development](development.md#continuous-integration)). The application is developed
primarily on Windows (x64), where it is also run and bundled locally; macOS and Linux
are currently verified through CI builds and tests only.

YMDB does not require administrator privileges and does not include MAME, ROMs, BIOS
files, CHDs or media of any kind.

## Getting YMDB

No release has been published yet, so there is no prebuilt installer to download.
YMDB is currently built from source.

### Prerequisites

- [Node.js](https://nodejs.org/) 22 (the exact version is in `.node-version`).
- [pnpm](https://pnpm.io/) 12 (pinned in the `packageManager` field of `package.json`;
  Corepack works).
- [Rust](https://rustup.rs/) through rustup. The toolchain is pinned in
  `rust-toolchain.toml` and installed automatically by rustup.
- The Tauri platform prerequisites (<https://tauri.app/start/prerequisites/>):
  - **Windows:** Microsoft C++ Build Tools (MSVC) and the WebView2 Runtime.
  - **macOS:** Xcode Command Line Tools.
  - **Linux:** WebKitGTK 4.1 and the related system libraries. On Debian/Ubuntu (the
    same packages CI installs):

    ```sh
    sudo apt-get install build-essential file libayatana-appindicator3-dev \
      librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev
    ```

No other system library is needed: SQLite is compiled into YMDB, and the SSH/SFTP
client is pure Rust (no OpenSSL, libssh2, `ssh` program or SSH agent is used).

### Building

```sh
git clone https://github.com/davidfriscia/ymdb.git
cd ymdb
pnpm install
```

Then either run YMDB in development mode (Vite dev server with hot reload plus the
application window):

```sh
pnpm tauri dev
```

or build a release bundle for the current platform:

```sh
pnpm tauri build
```

Bundles are written to `src-tauri/target/release/bundle/`, in the formats Tauri
produces for the platform you build on (`bundle.targets` is `all` in
`src-tauri/tauri.conf.json`). Release bundling is not automated and has only been
exercised locally on Windows. See [Development](development.md) for the quality
checks and the other build commands.

### Runtime dependencies

YMDB renders its interface with the system web view: WebView2 on Windows (included in
current Windows versions, otherwise installed with the WebView2 Runtime), WKWebView on
macOS and WebKitGTK 4.1 on Linux.

To launch games you need a MAME installation of your own (see
[MAME installations](mame-installations.md)). Everything else (media directories,
support files, remote repositories) is optional.

## First start

On its first start YMDB opens the **Catalog** page with an empty state, because no
MAME installation is registered yet. YMDB never runs MAME at startup: MAME is executed
only when you ask for something that needs it (adding or verifying an installation,
importing a catalog, playing a game, detecting controllers).

The main navigation has four pages:

| Page         | Purpose                                                                       |
| ------------ | ----------------------------------------------------------------------------- |
| **Catalog**  | Browse, search and filter the catalog of the active installation; play titles |
| **Lists**    | Custom lists of machines                                                      |
| **MAME**     | MAME installations and everything configured per installation                 |
| **Settings** | Global settings shared by every installation                                  |

**Settings** has these sections: **Appearance**, **Media Sources**, **Remote
Connections**, **Controllers**, **Favorites**, **Support Files**, **Application
Data**, **About** and **Diagnostics**. Each MAME installation on the **MAME** page has
**Status**, **Catalog**, **ROM Manager** and **Launch Options**.

Continue with [Getting started](getting-started.md).

## Application data

YMDB keeps its data in two platform directories for the bundle identifier
`io.github.davidfriscia.ymdb`: a configuration directory for small user files and a
local data directory for large, machine-specific derived data.

| Platform | Configuration directory                                      | Local data directory                                            |
| -------- | ------------------------------------------------------------ | --------------------------------------------------------------- |
| Windows  | `%APPDATA%\io.github.davidfriscia.ymdb`                      | `%LOCALAPPDATA%\io.github.davidfriscia.ymdb`                    |
| macOS    | `~/Library/Application Support/io.github.davidfriscia.ymdb`  | `~/Library/Application Support/io.github.davidfriscia.ymdb`     |
| Linux    | `$XDG_CONFIG_HOME/io.github.davidfriscia.ymdb` (`~/.config`) | `$XDG_DATA_HOME/io.github.davidfriscia.ymdb` (`~/.local/share`) |

Main files:

| File                                        | Directory     | Content                                         |
| ------------------------------------------- | ------------- | ----------------------------------------------- |
| `installations.json`                        | configuration | Registered MAME installations, the active one   |
| `installations/<id>/launch.json`            | configuration | Launch options of one installation              |
| `installations/<id>/remote-sources.json`    | configuration | ROM Manager settings of one installation        |
| `remote-connections.json`                   | configuration | Remote connections (no passwords or keys)       |
| `media.json`                                | configuration | Media sources                                   |
| `lists.json`                                | configuration | Favorites and custom lists                      |
| `saved-filters.json`                        | configuration | Saved filters                                   |
| `custom-themes.json`                        | configuration | Custom themes                                   |
| `controllers.json`                          | configuration | Controllers, controller profiles, assignments   |
| `window-state.json`                         | configuration | Position and size of the main window            |
| `installations/<id>/catalog.sqlite`         | local data    | Catalog of one installation (derived from MAME) |
| `installations/<id>/classifications.sqlite` | local data    | Support file classifications for that catalog   |
| `support-files.sqlite`                      | local data    | Imported support files                          |
| `launch-logs/`                              | local data    | Output of the most recent MAME sessions         |

Interface preferences (theme, panels, view options) are stored in the web view's
storage. **Settings → Application Data** lists every file and directory with its
absolute path and can export and import a full backup (see
[Backup and restore](backup-and-restore.md)).

### Separate data directory

When the environment variable `YMDB_DATA_DIR` is set to a directory, YMDB stores its
configuration in `<dir>/config` and its local data in `<dir>/data` instead of the
platform directories. Use it for tests or for a separate setup that must not touch
your usual data. Interface preferences remain in the web view's storage, which is
not moved by this variable; on Windows, `WEBVIEW2_USER_DATA_FOLDER` can point the web
view to a separate folder too.
