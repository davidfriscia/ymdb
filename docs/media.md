# Media

YMDB shows artwork and screenshots from image directories you already have: an
artwork collection, the packs published by [Progetto-SNAPS](https://www.progettosnaps.net/)
or MAME's own `snap` folder. YMDB does not include, download, copy, convert or modify
any image; it only reads the files where they are. Media are optional: without them
the catalog works with **Classic Cards** and the detail pages show a short note
instead of the gallery.

## Categories

| Category      | Id         | Typical folder of an artwork collection |
| ------------- | ---------- | --------------------------------------- |
| Snapshots     | `snap`     | `snap`, `snap_SL` (software lists)      |
| Title screens | `titles`   | `titles`                                |
| Flyers        | `flyers`   | `flyers`                                |
| Cabinets      | `cabinets` | `cabinets`                              |
| Marquees      | `marquees` | `marquees`                              |

These are the only categories. Other kinds of material (artwork overlays for MAME's
own `artwork` path, control panels, manuals, videos, icons) are not shown by YMDB.

## Configuring media sources

Open **Settings → Media Sources**. Each category lists its directories:

1. Choose **Add directory…** under a category and pick a folder with the native folder
   picker. The change is saved at once.
2. Add more directories if needed (up to 16 per category). They are searched from
   top to bottom: reorder them with the ↑ and ↓ buttons, or **Remove** one.
3. Each directory shows its status: **Available**, **Not found**, **Not a directory**
   or **Not readable**. A missing directory (for example a network share that is
   offline) is kept and simply finds nothing.

Paths are absolute. Local disks and network shares already mounted by the operating
system work; no remote protocol is used for media. Media sources are global: every
MAME installation uses the same directories.

## Naming conventions

Images are found by file name only, never by content:

| Record   | File in a source directory                               |
| -------- | -------------------------------------------------------- |
| Machine  | `<machine>.<ext>`, for example `pacman.png`              |
| Machine  | `<machine>/0000.png`, `<machine>/0001.png`, … (numbered) |
| Software | `<list>/<software>.<ext>`, for example `nes/smb.png`     |

- `<machine>` is the MAME short name, `<list>` the software list name and
  `<software>` the software short name, all lowercase as MAME writes them. On
  case-sensitive file systems (Linux) file names must be lowercase.
- Supported extensions, in this order: `png`, `jpg`, `jpeg`, `webp`. Other formats
  (`bmp`, `gif`) are ignored, and zipped collections (such as `snap.zip`) are not
  read: extract them first.
- The numbered files are the screenshots MAME saves in its `snap` directory with its
  default `snapname` setting; add MAME's `snap` directory to **Snapshots** to see
  them.
- There is no parent/clone fallback: a clone without its own image shows a
  placeholder, never the parent's image. A machine image is never used for a
  software with the same name.
- Images larger than 32 MiB are not displayed.

## Example layouts

An artwork collection with one folder per category (the layout used by many packs):

```text
D:\MAME Artwork\
  snap\
    pacman.png
    galaga.png
  snap_SL\
    nes\
      smb.png
  titles\
    pacman.png
  flyers\
    pacman.jpg
  cabinets\
    pacman.png
  marquees\
    pacman.png
```

Configuration for this layout:

| Category      | Directories                                                       |
| ------------- | ----------------------------------------------------------------- |
| Snapshots     | `D:\MAME Artwork\snap`, `D:\MAME Artwork\snap_SL`, `C:\MAME\snap` |
| Title screens | `D:\MAME Artwork\titles`                                          |
| Flyers        | `D:\MAME Artwork\flyers`                                          |
| Cabinets      | `D:\MAME Artwork\cabinets`                                        |
| Marquees      | `D:\MAME Artwork\marquees`                                        |

Here `C:\MAME\snap` is MAME's own screenshot directory, so screenshots you take
while playing appear next to the collection's snapshots. Software list snapshots
work because each file sits in a folder named after its list: the `snap_SL`
directory itself is the source, not `snap_SL\nes`.

## Using Progetto-SNAPS packs

Progetto-SNAPS publishes artwork packs (snapshots, titles, flyers, cabinets,
marquees and more) for machines and for software lists. Packs are distributed as
archives and their internal structure can differ between packs and releases, so:

1. Download the packs you want from <https://www.progettosnaps.net/> and extract them
   to a folder of your choice.
2. Find, in the extracted files, the directory that directly contains the images
   named after machines (`pacman.png`), or, for software lists, the directory that
   contains one folder per list (`nes/smb.png`).
3. Add that directory to the matching category in **Settings → Media Sources**.
4. Open the catalog, switch to **Media Grid** and choose the **Artwork** category to
   check the result.

If a category shows only placeholders, check that you added the right level of the
folder tree and that the images have a supported extension. See also
[Troubleshooting](troubleshooting.md#media).

Progetto-SNAPS also publishes the support files YMDB can import for classifications;
see [Support files](support-files.md).

## Where media appear

- **Media Grid** (catalog **View → View mode**): one tile per result with the first
  image of the chosen **Artwork** category. Tiles without an image show a
  placeholder.
- **Detail gallery**: see below.

### Detail gallery

The detail page of a machine or software shows a carousel of its images, with the
categories that have at least one image next to it (Snapshots, Title screens,
Flyers, Cabinets, Marquees). A category can hold several images: the image of each
configured directory and, for machines, every numbered screenshot. Move with the
arrow buttons, the dots or the arrow keys; click the image to enlarge it (**Esc** or
**Close** closes it). Images keep their proportions.

Images reach the interface through YMDB's own `ymdb-media` protocol, by category and
MAME names only: the interface can never request an arbitrary file, and links
pointing outside a source directory are refused. See
[ARCHITECTURE.md](../ARCHITECTURE.md#media-sources--implemented) for details.
