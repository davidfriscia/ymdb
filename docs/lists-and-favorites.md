# Favorites, lists and saved filters

Favorites, custom lists and saved filters belong to YMDB, not to a MAME installation.
They reference machines by MAME short name (`pacman`) and software by software list
and short name (`nes`/`smb`), never by a catalog id, a description or a ROM file
name. So:

- an entry added with one MAME version applies to every catalog that has the same
  short name;
- an entry missing from the active catalog is kept and shows up again with an
  installation that has it;
- switching, updating, re-importing or removing an installation never changes them,
  and YMDB never matches titles approximately (a renamed short name is a different
  machine).

They are saved in `lists.json` and `saved-filters.json` in the configuration
directory and are part of the [application backup](backup-and-restore.md).

## Favorites

Every card of the catalog, in **Classic Cards** and **Media Grid**, has a bookmark in
its top right corner: faint while the machine or software is not a favorite, solid
when it is. Click it (or focus it and press Enter or Space) to add or remove the
favorite; the card does not open. The same bookmark sits next to the title of a detail
page. If a change cannot be saved, the bookmark returns to its previous state and an
error explains why.

**Favorites only**, in **View Options**, shows only the favorites of the active
catalog, for machines or software. It combines with the search text, the filters, the
sort order and the pages, and a chip above the results turns it off.

**Settings → Favorites** shows how many favorites you have and exports them to a JSON
file (**Export favorites…**), or imports such a file. An import first shows how many
entries are new, already present or duplicated, then **Merge** (add the new ones,
remove nothing) or **Replace** (keep exactly those of the file, after a confirmation
that says how many will be removed). An invalid file changes nothing. The file holds
only MAME names, so it works on another computer and with any MAME version:

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

## Custom lists

Custom lists organize **machines** in collections of your own. A machine can be in
any number of lists. Favorites stays separate: it has its own bookmark and is never
shown or changed as a custom list.

The **Lists** page shows your lists, with a search and a sort (name, most machines,
recently changed, recently created):

- **New list** creates an empty list. Names are unique ignoring case and may
  contain `/`.
- Selecting a list shows its machines, a page at a time, with their description and
  whether the active catalog has them ("Not in this catalog" otherwise). Search them,
  remove machines, or add machines found in the catalog.
- **Rename…** keeps the list, its machines and the saved filters that use it. A name
  already used by another list is refused: renaming never merges lists.
- **Delete…** asks for confirmation and removes only that list.

In the catalog, every machine card and machine detail page has a list button next to
its bookmark. It opens **Manage Lists**: the lists of that machine as checkboxes, with
a search and **Create list**. Nothing changes until **Save**; **Cancel** discards the
changes.

**View Options → Lists** shows only the machines of one list ("All items" shows
everything). It combines with the search, the filters and **Favorites only**, can be
saved in a saved filter, and applies to machines only. If the selected list is
deleted, the catalog says so and shows nothing until you choose another list or
**Show all items**.

This version manages lists of machines; software entries in `lists.json` are kept but
not managed.

### Importing folder files (`.ini`)

**Import INI…** on the Lists page imports the folder files used by MAME frontends,
such as the `.ini` folder files distributed by
[Progetto-SNAPS](https://www.progettosnaps.net/):

```ini
[FOLDER_SETTINGS]
RootFolderIcon mame

;; EXAMPLE.ini 0.288 ;;

[ROOT_FOLDER]
1942abl
1942iti

[Arcade / Pinball]
aliencr
```

- `[FOLDER_SETTINGS]` and comments are ignored. The machines of `[ROOT_FOLDER]` become
  a list with the import name you choose (proposed from the file name); an empty root
  creates no list.
- Every other section is a subfolder. Choose how its lists are named: **By subfolder
  name** (`Arcade / Pinball`) or **With the import name first**
  (`Category / Arcade / Pinball`). Empty subfolders are ignored.
- A list whose name already exists (ignoring case) is **merged**: its machines stay
  and only the missing ones are added. Importing the same file twice with the same
  choices therefore changes nothing.
- Lines that are not machine short names are skipped and reported.
- Before anything changes, a preview shows the lists to create and to update, the
  machines to add, those already present, the empty subfolders, duplicates and
  skipped lines, and every resulting list. **Import** applies everything at once, or
  nothing if it fails.
- Files can be UTF-8 (with or without BOM) or Windows-1252, with any line endings.

Folder files become lists; they are different from the [support files](support-files.md)
(`catver.ini`, `genre.ini`, …), which become classifications and filters.

## Saved filters

**Saved Filters**, in **View Options**, keeps named search configurations:

- **Save current filters** stores the catalog type, the search text, every filter
  (classifications, version ranges and the selected list included) and **Favorites
  only**. It does not store the view (renderer, sizes, panels, theme) or the sort.
- Clicking a saved filter applies it at once and returns to the first page.
- After a change the saved filter is marked **Modified** until you **Update** it; it
  never changes on its own. Saved filters can be renamed and deleted.
- Criteria that match nothing in the active catalog (a series it does not have, a
  classification without a source) are kept and reported above the results; they are
  never dropped or replaced.
- Saved filters written by a newer YMDB are kept unchanged but cannot be loaded.

See [ARCHITECTURE.md](../ARCHITECTURE.md#user-lists-favorites-and-custom-lists--implemented)
for the data model and the import rules.
