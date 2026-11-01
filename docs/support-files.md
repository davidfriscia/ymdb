# Support files and classifications

MAME's own data describes machines technically: manufacturer, year, driver status,
devices, software lists. It does not say whether a machine is a shooter or a
platformer. Community _support files_, such as those published by
[Progetto-SNAPS](https://www.progettosnaps.net/), add that information. YMDB can
import five of them, from files you download yourself:

| File          | Provides                                                         |
| ------------- | ---------------------------------------------------------------- |
| `catver.ini`  | **Category** (`[Category]`) and **Version Added** (`[VerAdded]`) |
| `genre.ini`   | **Genre**                                                        |
| `catlist.ini` | **Category** (an alternative to `catver.ini`)                    |
| `version.ini` | **Version Added** (an alternative to `catver.ini`)               |
| `series.ini`  | **Series** (a machine can belong to several series)              |

Classifications apply to machines only. YMDB stores them as the files state them and
never derives its own classification of machines or software.

| Data                                   | Origin                             | Where it is stored                                                                             |
| -------------------------------------- | ---------------------------------- | ---------------------------------------------------------------------------------------------- |
| Machines, software lists, statuses, …  | MAME (`-listxml`, `-listsoftware`) | `installations/<id>/catalog.sqlite`                                                            |
| Genre, Category, Series, Version Added | Support files you import           | `support-files.sqlite`, plus a copy per catalog in `installations/<id>/classifications.sqlite` |

## Importing

1. Download the files you want and keep them anywhere on your computer. YMDB never
   downloads or updates them.
2. Open **Settings → Support Files** and choose **Import support file…**.
3. YMDB recognizes the format by its header and structure (not only by the file
   name), checks it and shows a summary (declared MAME version, machines, values,
   warnings such as invalid lines or duplicates) before anything is stored.
4. Confirm the import. Importing a file of a format that is already imported replaces
   it after a confirmation.

Each imported file shows its name, declared version, import date, machines, values
and whether it is used. It can be reimported (for example after you download a newer
release) or removed; the file on disk is never modified or deleted.

## Choosing sources

Each classification uses one source at a time: the first file imported for it, or
the one selected in **Classifications**. For example, Category can come from
`catver.ini` or from `catlist.ini`, never from both. Removing the source in use leaves
the classification without one until you select or import another; YMDB never
switches source on its own.

## Using classifications

Support files are shared by every MAME installation. Each catalog keeps its own copy
of the classifications of its machines, matched by short name, and brings it up to
date the next time the catalog is opened after a change (or after the catalog is
imported again); machines a catalog does not have are ignored.

In the catalog **Filters** panel, **Genre**, **Category** (a tree: a category includes
its subcategories), **Series** and **Version Added** (a From/To range in MAME version
order, or exact versions) appear for machines when the catalog has them; otherwise a
note leads to Settings. The Overview of a machine detail shows its
**Classification**; each value shows the catalog filtered by it.

## Updating

When a new release of a support file is published, download it and import it again
(or use reimport on the existing entry). Catalog copies are refreshed automatically
the next time each catalog is opened.

See [ARCHITECTURE.md](../ARCHITECTURE.md#mame-support-files--implemented) for the
parsers, the repository and the per-catalog projections.
