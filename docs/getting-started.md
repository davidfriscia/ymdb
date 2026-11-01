# Getting started

This tutorial takes you from nothing to a working YMDB setup: YMDB built and running,
a MAME installation registered, its catalog imported, artwork configured and a title
started. The optional steps add support files, a personal remote repository and a
backup. Each step links to the page with the details.

What you need:

- a computer with Windows, macOS or Linux;
- MAME (any recent official release);
- the ROM sets of the titles you want to play, which you must provide yourself;
- optionally, artwork images, support files and a server with your collection.

## Step 1: install YMDB

No prebuilt release is published yet, so build YMDB from source. Install Node.js 22,
pnpm 12, Rust (rustup) and the Tauri prerequisites of your platform (see
[Installing YMDB](installation.md#prerequisites)), then:

```sh
git clone https://github.com/davidfriscia/ymdb.git
cd ymdb
pnpm install
pnpm tauri build
```

Install or run the bundle written to `src-tauri/target/release/bundle/`, or start YMDB
directly from the sources with `pnpm tauri dev`.

## Step 2: get MAME

Download MAME from <https://www.mamedev.org/> (or install it with your package
manager) and extract it to a directory of your choice, for example `C:\MAME` on
Windows or `~/mame` on Linux and macOS. Put your ROM sets in its `roms` directory, or
in any directory listed in the `rompath` of your `mame.ini`.

Check that MAME itself works: `mame -version` must print a line such as
`0.280 (mame0280)`.

Remember the distinction: the YMDB catalog describes what MAME supports; the ROM sets
are what MAME needs to actually run a title. YMDB never provides ROM sets.

## Step 3: register MAME

1. Start YMDB and open the **MAME** page.
2. Choose **Add installation…** and select the MAME executable (`mame.exe` on
   Windows, `mame` on macOS and Linux), or choose **Detect installations** and **Add**
   one of the results.
3. YMDB runs `mame -version` and registers the installation if it is valid. The first
   installation becomes the active one; its **Status** section shows the version.

Details: [MAME installations](mame-installations.md).

## Step 4: import the catalog

1. In the installation's group on the **MAME** page, open **Catalog**.
2. Choose **Import catalog**. YMDB runs `mame -listxml` and stores the machines; this
   takes a few seconds.
3. Choose **Import software lists** to add the software lists (consoles, computers
   and their software). MAME needs up to about a minute to produce them.
4. Open the **Catalog** page: search for a title, try the **Filters** panel, and open
   a detail page.

Details: [Catalog](catalog.md).

## Step 5: configure media (optional)

If you have artwork (for example packs from [Progetto-SNAPS](https://www.progettosnaps.net/)):

1. Extract the packs to a folder, for example `D:\MAME Artwork\snap`,
   `D:\MAME Artwork\titles`, `D:\MAME Artwork\flyers`.
2. Open **Settings → Media Sources** and choose **Add directory…** under each
   category (**Snapshots**, **Title screens**, **Flyers**, **Cabinets**, **Marquees**).
   Pick the folder that directly contains `<machine>.png` files.
3. Optionally add MAME's own `snap` directory to **Snapshots**, so screenshots you
   take in MAME appear too.
4. In the **Catalog**, open **View** and choose **Media Grid**.

Details and example layouts: [Media](media.md).

## Step 6: add support files (optional)

To filter machines by genre, category, series or the MAME version that added them,
download `catver.ini`, `genre.ini`, `series.ini` (or the alternatives `catlist.ini`
and `version.ini`) and import them in **Settings → Support Files → Import support
file…**. The **Filters** panel then shows **Genre**, **Category**, **Series** and
**Version Added**.

Details: [Support files](support-files.md).

## Step 7: play a title

1. In the **Catalog**, open the detail of a machine whose ROM set is in MAME's
   `rompath`.
2. Choose **Play**. YMDB starts MAME from its own directory with the machine's short
   name, so MAME reads its usual `mame.ini`.
3. While MAME runs, the detail page shows the session; **Stop** terminates it after a
   confirmation. If MAME refuses to start (for example a missing ROM), the launch
   status shows MAME's message.

For software (a console cartridge, a computer floppy), open the software's detail,
choose the machine that runs it and choose **Play**.

To change MAME options for every launch, for one machine or for one software, use
**Launch Options** on the MAME page, or **Advanced** on the detail page. Details:
[Launching games](launcher.md). For gamepads and input configurations, see
[Controllers](controllers.md).

## Step 8: use a personal remote repository (optional)

If your collection lives on another computer reachable over SSH (a NAS, a home
server):

1. Prepare an SSH account with key authentication on the server and note its
   fingerprint.
2. Add the connection in **Settings → Remote Connections → Add connection…**, then
   **Test Connection** and approve the server identity.
3. In **MAME → <installation> → ROM Manager**, enable **Machine ROMs** (and the other
   categories you need), with the remote base path, the collection organization and
   a local destination, then **Save ROM Manager settings**.
4. On the detail page of a title whose files are missing locally, choose **Download &
   Play**: YMDB downloads the missing files to the local destination, then starts
   MAME.

The complete procedure, with server preparation, layout and troubleshooting:
[Remote repository](remote-repository.md).

## Step 9: organize your collection

- Click the bookmark on a card to add a **favorite**; **View → Favorites only**
  shows them.
- Create **custom lists** on the **Lists** page, or import folder `.ini` files with
  **Import INI…**.
- Save frequently used searches with **View → Saved Filters**.

Details: [Favorites, lists and saved filters](lists-and-favorites.md).

## Step 10: back up your setup

**Settings → Application Data → Export backup…** writes one archive with your
installations, catalogs, lists, filters, settings and support files (not ROMs, media
or keys). **Import backup…** restores it, also on another computer. Details:
[Backup and restore](backup-and-restore.md).

## When MAME is updated

Add the new MAME release as a new installation, import its catalog and software
lists, and set it as active (**Set as active**, or **View → MAME Version** in the
catalog). Favorites, lists, saved filters, media and support files carry over;
launch options and ROM Manager settings are per installation. See
[Updating MAME](mame-installations.md#updating-mame).

If something does not work, see [Troubleshooting](troubleshooting.md).
