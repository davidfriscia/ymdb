# Troubleshooting

Symptoms and what to check. For remote repository problems (SSH, SFTP, downloads), see
[Remote repository → Troubleshooting](remote-repository.md#troubleshooting).

## MAME installations

| Symptom                                       | What to check                                                                                                 |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| The executable is refused as "not executable" | On Windows choose the `.exe` (not a `.bat` or shortcut); on macOS/Linux the file needs the execute permission |
| Validation reports an unexpected exit code    | The MAME release may not support `-version` (for example 0.162); use a newer MAME                             |
| Validation reports unrecognized output        | The program is not MAME, or a build that prints a non-standard version line                                   |
| **Detect installations** finds nothing        | Detection only looks in `PATH` and a few standard directories, not recursively: use **Add installation…**     |
| The installation is marked as changed         | The executable was replaced in place: choose **Verify**, then import the catalog again                        |
| The executable is marked as missing           | The file was moved or deleted; add the new location as a new installation                                     |

## Catalog

| Symptom                                                  | What to check                                                                               |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| The Catalog page says there is no catalog                | Import it in **MAME → <installation> → Catalog**; the active installation may have none yet |
| Software are not available                               | Choose **Import software lists** for that installation                                      |
| Software lists are refused                               | They must come from the same, unchanged executable as the machines: reimport machines first |
| Software lists disappeared after **Reimport machines**   | Expected: reimporting machines starts a new catalog; import the software lists again        |
| The import fails on Windows because the file is in use   | Close programs that keep `catalog.sqlite` open (for example a database browser)             |
| A warning says the catalog does not match the executable | The executable changed after the import; reimport the catalog                               |
| Filter values match nothing after switching MAME version | The values do not exist in that catalog; clear them with the notice above the results       |

## Media

| Symptom                                 | What to check                                                                                                       |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Every tile shows a placeholder          | The category chosen in **View → Artwork** has a directory in **Settings → Media Sources** with status **Available** |
| Some images are missing                 | File names must be MAME short names (`pacman.png`), lowercase on Linux; no parent image is used for clones          |
| Software images are missing             | The source must contain one folder per list (`nes/smb.png`); add the parent folder, not `nes` itself                |
| Images from a zipped pack are not shown | Archives are not read: extract them                                                                                 |
| Some files are ignored                  | Only `png`, `jpg`, `jpeg` and `webp` files up to 32 MiB are shown                                                   |
| A network share shows **Not found**     | Mount or reconnect the share in the operating system; YMDB keeps the directory and finds images again later         |

## Launching

| Symptom                                          | What to check                                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| MAME exits at once and the status shows an error | Read MAME's message in the launch status: usually missing ROM, BIOS or CHD files, or ROMs from another MAME version |
| **Play** is refused because of the catalog       | The catalog does not match the installation's executable: reimport it                                               |
| A second launch of the same title is refused     | A session for that title is already running from that installation                                                  |
| An option has no effect                          | Check **Advanced → Command line**: it shows every argument and the level it comes from                              |
| Input changes were lost                          | MAME saves inputs when it exits normally; **Stop** kills MAME and discards them                                     |
| A software runs badly on the chosen machine      | The machine list shows what MAME declares, not a compatibility guarantee: try another machine                       |

## Controllers

| Symptom                                | What to check                                                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Detect devices** fails               | It runs MAME: the selected installation must be valid. A failure is reported as an error, not as an empty list                                               |
| Players swap after replugging gamepads | Controllers with a port/order binding follow the USB port or enumeration order                                                                               |
| A mapped launch is refused             | All mapped controllers must use the same joystick provider; with the MAME configuration profile, a `ctrlr` from an INI file other than `mame.ini` is refused |

## Application data

| Symptom                                     | What to check                                                                                                     |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| A configuration file is reported as invalid | YMDB never replaces it implicitly; restore it from a backup or fix it, then restart                               |
| Export or import of a backup is refused     | Wait until MAME sessions, catalog imports and downloads are finished                                              |
| A backup is refused                         | It was made by a newer YMDB version                                                                               |
| Paths are missing after a restore           | MAME executables, media directories and private keys are not part of a backup; correct the paths on this computer |
| Data must stay separate for a test          | Start YMDB with `YMDB_DATA_DIR` (see [Installing YMDB](installation.md#separate-data-directory))                  |

**Settings → Diagnostics** checks the connection between the interface and the Rust
core and reports the application version, operating system and architecture.
