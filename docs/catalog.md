# Catalog

Every MAME installation has its own catalog: a local SQLite database built from the
information MAME itself provides about the machines and software lists it supports.
The catalog holds only data derived from MAME; it can be deleted at any time and
imported again.

## Importing the catalog

On the **MAME** page, open the **Catalog** section of an installation.

1. **Import catalog** (later **Reimport machines**) runs `<mame> -listxml`, reads its
   output as a stream and stores the machines. With MAME 0.280 this stores 48,893
   entries (including devices) in a few seconds.
2. **Import software lists** (later **Update software lists**) runs
   `<mame> -listsoftware` and adds the software lists: with MAME 0.280, 720 lists and
   139,122 software, with their parts, media (ROMs and disks), checksums and
   metadata. MAME itself needs from about 15 seconds to a minute to produce this list.
   Software lists can only be imported from the same, unchanged executable the
   machines were imported from.

While importing, the panel shows a progress bar and the number of entries processed.
**Cancel import** stops MAME and discards the partial import. One import runs at a
time; it stays attached to its installation even if you change the active
installation meanwhile. When it completes, the panel shows the MAME version, the
import date and a summary of machines and software lists.

Imports are atomic: the new catalog is built in a separate file and replaces the
previous one only when the import succeeds. If MAME fails, the output cannot be
parsed or the import is cancelled, the previous catalog stays available. On Windows,
if another program keeps the catalog file open, the import fails with a message asking
to close that program.

Reimporting machines starts a new catalog without software lists: import the software
lists again afterwards. After updating MAME, see
[Updating MAME](mame-installations.md#updating-mame).

Catalogs imported by earlier YMDB versions are upgraded and indexed automatically the
first time they are opened. Catalogs are stored in the local data directory, in
`installations/<id>/catalog.sqlite` (see
[Application data](installation.md#application-data)).

## Browsing

The **Catalog** page (the start page) browses the machines or software of the active
installation.

- Choose **Machines** or **Software** and type a text: the search starts when you
  pause typing, or press Enter or **Search**. Pick a sort order and direction.
- The **Filters** panel lists, with counts, manufacturers, years, driver and
  emulation statuses and software lists of the machines (publishers, lists, years,
  support statuses and part interfaces for software), plus parent/clone, BIOS,
  device, mechanical, runnable and "declares ROMs/CHDs" filters. With imported
  [support files](support-files.md), machines can also be filtered by **Genre**,
  **Category**, **Series** and **Version Added**.
- Values of one filter are alternatives; different filters combine. Long value
  lists scroll on their own and have a value search, **Show more** and **Browse all**
  (a window with every value, where **Apply** changes the filter and **Cancel** leaves
  it unchanged). Active filters appear as chips above the results; remove them one
  by one or with **Clear all filters**.
- The **Filters** and **View** toolbar buttons hide or show the side panels; the
  filters stay active when their panel is hidden.
- Results are paginated in the backend: 25, 50, 100 or 200 per page, with
  **Previous**/**Next**, page numbers, **Per page** and **Go to** below the results.

Text search matches every word, in any indexed field (machines: name, description,
manufacturer, year; software: name, description, publisher, year, list name), as a
word prefix, ignoring case and accents. An exact short name (`sf2`) ranks first.
Search operators are treated as plain text; there is no fuzzy matching.

Declared ROMs and CHDs are what MAME lists for a machine; the catalog does not check
the files on your disk.

## View options

The **View** button opens the **View Options** panel:

- **MAME Version** switches the active installation (see below).
- **View mode**: **Classic Cards** (text cards, **Card size**) or **Media Grid**
  (artwork tiles: **Tile size**, **Artwork** category, **Image fit**, and whether to
  show the title, the technical name and the status badge). The Media Grid needs
  [media sources](media.md).
- **Favorites only** and **Lists** restrict the results to your favorites or to one
  custom list (see [Favorites and lists](lists-and-favorites.md)).
- **Saved Filters** saves and loads named searches (see
  [Saved filters](lists-and-favorites.md#saved-filters)).

Changing the view never changes the search, filters, sort or page. The theme
(**Light**, the default, **Dark** or **Neon 80s**; see [Themes](themes.md)) is chosen
in **Settings → Appearance**. View options and the theme are interface preferences,
the same for every installation.

### Switching MAME version

**View → MAME Version** lists the registered installations (with "no catalog" for
those not imported yet) and switches the active installation without importing
anything. The search text, filters and sort order are kept and the results return to
the first page; filter values that do not exist in the new catalog match nothing (a
notice offers to clear them). Open details are closed, because they belong to the
previous catalog. Results of two catalogs are never shown together.

## Details

Click a card or tile (or press Enter on it) to open its detail page. Machine and
software details share one layout:

1. a header with **Back to results** and **Previous** / position / **Next** (through
   the current result page);
2. the artwork gallery (see [Media](media.md#detail-gallery));
3. a launch bar with the title, the short information, **Play**, the launch
   **Profile**, the **Controls** selector, **Prepare Download** and **Download &
   Play** (see [Launching games](launcher.md));
4. the status of the last launch, when there is one;
5. the collapsed **Advanced** section (launch options, controls, command line);
6. the technical sections.

Machines show general data, relations (parent, clones, devices), emulation status,
ROMs, CHDs, devices, displays, controls, chips and declared software lists; with
support files, the Overview shows its **Classification**. Software shows its list,
parts, ROMs and disks and the machines declaring its list (this is not a
compatibility guarantee). Software lists show their software and machines. Parents,
clones, devices and software lists are links; **Back** returns to the previous
detail, **Back to results** to the results with the search, filters, page and scroll
position unchanged.

## Limitations

- The relation between machines and software lists is shown as MAME declares it; it
  does not guarantee that a software works on a machine.
- There is no combined update of machines and software lists in one operation.
- Search uses prefix matching, not substring or fuzzy matching.
- YMDB does not audit ROM sets: it never checks the contents of your archives.

Implementation details (schema, import strategy, search engine and measured
performance) are in [ARCHITECTURE.md](../ARCHITECTURE.md#catalog--implemented-machines-and-software-lists).
