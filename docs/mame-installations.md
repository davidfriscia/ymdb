# MAME installations

YMDB uses MAME installations that already exist on your computer. It never downloads,
installs or updates MAME, and it never modifies MAME's files (`mame.ini`, `cfg`,
`nvram`, ROMs, artwork).

## Getting MAME

Download MAME from the official site, <https://www.mamedev.org/>, or install it with
your operating system's package manager. Any directory works, including portable
installations and custom builds, as long as the executable supports the `-version`
option and prints the standard MAME version line. This was verified with MAME 0.240,
0.252 and 0.280; older releases such as MAME 0.162 do not support `-version` and are
refused.

An example of a portable layout, with the executable, an optional `mame.ini` (MAME
creates one with `mame -createconfig`) and the directories MAME uses by default (some
of them are only created by MAME when it first needs them):

```text
MAME/
  mame.exe
  mame.ini        # optional, MAME's own configuration
  roms/           # ROM sets, BIOS sets and CHD directories (rompath)
  samples/        # samplepath
  artwork/
  cfg/
  nvram/
  snap/           # screenshots saved by MAME
```

YMDB always starts MAME from the directory of its executable, so relative paths in
`mame.ini` behave exactly as when you run MAME there.

The catalog that YMDB imports describes what a MAME build supports. It contains no
game data: to play a title, MAME still needs its ROM sets (and CHDs or samples when
the title uses them) in its search paths. YMDB does not include any of them; see
[Remote repository](remote-repository.md) for downloading them from your own server.

## Registering an installation

Open the **MAME** page. Its navigation starts with **Add Installation**, followed by
one group per registered installation (name, version, whether a catalog is imported
and an **Active** marker).

- **Add installation…** opens the native file picker. Choose the MAME program file
  (`mame.exe` on Windows, usually `mame` on macOS and Linux). YMDB validates it and
  registers it only if validation succeeds. The same executable cannot be registered
  twice.
- **Detect installations** looks for MAME in the directories listed in `PATH` and in
  a few conventional directories (non-recursively; for example `C:\mame` or Scoop and
  Chocolatey locations on Windows, `/opt/homebrew/bin` and `/usr/local/bin` on macOS,
  `/usr/games`, `/usr/bin` and `/snap/bin` on Linux), then validates each candidate.
  Detection never registers anything: choose **Add** next to a detected executable.

The first installation you add becomes the **active** one: the catalog browser shows
its catalog and **Play** uses its executable.

### Validation

Validation runs `<executable> -version` directly (never through a shell), from the
executable's directory, with a 20-second timeout. The executable is accepted only if
it exits with code 0 and prints a MAME version line such as `0.280 (mame0280)`. Each
failure has a specific reason: file not found, not a file, not executable, failed to
start, timed out, unexpected exit code or unrecognized output. On Windows only `.exe`
files are accepted (batch files are refused).

## Managing installations

Each installation group has four sections:

- **Status** shows the executable, version, verification status and whether the
  installation is active:
  - **Set as active** makes it the active installation (MAME is not started).
  - **Verify** runs the executable again and shows the detected version, the
    validation status, the time of the check and the raw output.
  - **Rename…** gives it a name; an empty name shows the version again.
  - **Remove…** unregisters it after a confirmation. You choose whether to also
    delete its imported catalog (checked by default: it can be imported again) and
    its launch options and ROM Manager settings (unchecked by default). MAME and its
    files are never deleted. Removal is refused while a catalog import of the
    installation or a MAME session started from it is running.
- **Catalog** imports the catalog of that installation (see
  [Catalog](catalog.md#importing-the-catalog)).
- **ROM Manager** configures downloads of missing files (see
  [Remote repository](remote-repository.md)).
- **Launch Options** sets the MAME options YMDB passes (see
  [Launching games](launcher.md#launch-options)).

Choosing a section never changes the active installation. Sections keep unsaved
changes while you look at other sections.

You can also switch the active installation from the catalog: **View → MAME Version**
in the catalog browser (see [Catalog](catalog.md#switching-mame-version)).

## Several MAME versions

You can register any number of installations, including several builds of the same
version: each one is identified by an internal id, never by its version. Each
installation has its own catalog, launch options, launch profiles, ROM Manager
settings and controller profile configuration directories; these are never shared or
copied between installations. Media sources, favorites, custom lists, saved filters,
support files, remote connections and interface preferences are global.

## Updating MAME

The executable of an installation never changes. There are two ways to update:

- **New MAME release in another directory** (recommended): add it as a new
  installation, import its catalog and set it as active. The previous installation
  keeps working until you remove it. Favorites, lists and saved filters reference
  machines by MAME short name, so they apply to the new catalog automatically.
  Launch options are per installation and must be set again for the new one.
- **Executable replaced in place**: YMDB notices the change (file size and
  modification time) and marks the installation as changed. Choose **Verify**, then
  import the catalog again (**Reimport machines**, then **Update software lists**).
  Launches are refused while the catalog does not match the executable.

## Upgrading from a single installation

Early versions of YMDB managed one executable (`settings.json`), one catalog and one
`launch.json`. On the first start of a newer version, YMDB registers that executable
as an installation and gives it the existing catalog (moved, not imported again) and
launch options (copied), without running MAME. The old `settings.json` and
`launch.json` are left untouched as a backup. If the upgrade cannot complete (for
example because another program has the catalog open), nothing is deleted and YMDB
retries at the next attempt.

See [ARCHITECTURE.md](../ARCHITECTURE.md#mame-installations--implemented) for the
model, the files of an installation and the migration in detail.
