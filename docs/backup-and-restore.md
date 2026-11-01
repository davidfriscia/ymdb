# Application data, backup and restore

## Application data

**Settings → Application Data** lists every file and directory YMDB uses on this
computer, with its purpose, absolute path, presence, size, number of files and
whether it is part of a backup. **Open location** shows it in the file manager and
**Copy path** copies its path. Files created only on first use are shown as "Not
created yet". Data referenced by the configuration but not owned by YMDB (MAME
executables, media directories, ROM destinations, private keys) is listed separately
with its availability. The section never deletes files.

The directories themselves are described in
[Installing YMDB](installation.md#application-data).

## Exporting a backup

**Export backup…** writes one archive (`ymdb-<date>-<time>.ymdb-backup.zip`) with
every file YMDB owns: installations, launch configurations, ROM Manager settings,
controllers and controller profile files (including the MAME configuration
directories of profiles), favorites and lists, saved filters, media sources, remote
connections, catalogs, the support files repository and the interface preferences.

Databases are copied with the SQLite backup API, and the export waits for changes in
progress and refuses new ones while it copies, so the archive is one consistent
snapshot. The archive is verified (checksums included) before it gets its final name.

Not included: ROMs, CHDs, samples, MAME installations and their files, media files,
private keys, launch logs, generated controller files, classification copies (rebuilt
from the catalogs and support files) and the window state. YMDB stores no password,
passphrase or key, so a backup contains no secrets: remote connections keep only the
path of the private key and the approved public server identity.

## Restoring a backup

**Import backup…** checks the archive first (format and version, paths, sizes,
checksums, no links or unexpected files, decompression limits, free space), extracts
it to a staging directory and loads every file with the code that reads it, without
changing the current data. A preview then shows the backup (date, YMDB version,
platform, contents), the current data it replaces and the external paths missing on
this computer.

After confirmation (**Full restore — replace current application data**, then
**Replace data and restart**) YMDB restarts and replaces its data before opening it.
The previous data is kept aside and put back if anything fails, and a restore
interrupted by a crash is rolled back at the next start. External files are never
copied or overwritten; paths that do not exist on this computer remain in the
configuration and can be corrected afterwards (for example the MAME executable or
the media directories on a new computer).

Rules:

- Export and import are refused while MAME, a catalog import or a ROM download runs.
- Backups of the same or an older YMDB version can be imported (their data is
  upgraded when read); backups of a newer version are refused.
- A backup from another operating system is accepted with a note; its paths are kept
  as they are and shown as missing.

## Window state

YMDB reopens its main window as it was closed: maximized if it was maximized,
otherwise at the same position and size. The state is saved in `window-state.json`
when the window closes. At startup it is checked against the monitors available: a
size larger than the screen is reduced, a window partly off screen is moved onto its
monitor, and a window whose monitor is no longer connected (or a missing or damaged
file) opens on the primary monitor.

See [ARCHITECTURE.md](../ARCHITECTURE.md#application-data-management--implemented) for
the inventory, the archive format and the restore procedure.
