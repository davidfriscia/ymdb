import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { machine, response } from "../../test/catalogFixtures";
import { installationsView, NEW_ID } from "../../test/installationFixtures";
import { commandError } from "../../test/controllerFixtures";
import { listSummary } from "../../test/listFixtures";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import { CustomListsProvider } from "./CustomListsProvider";
import { ListCombobox } from "./ListCombobox";
import type {
  FolderFileSummary,
  FolderImportPreview,
  ListEntriesPage,
  ListSummary,
} from "./listsApi";
import { ListsPage } from "./ListsPage";
import { ManageListsButton } from "./ManageListsButton";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

function backend(handlers: Partial<Record<string, Handler>>) {
  invokeMock.mockImplementation((command: string, args?: Args) => {
    const handler = handlers[command];
    if (!handler) return Promise.reject(new Error(`unexpected command ${command}`));
    try {
      return Promise.resolve(handler(args ?? {}));
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
}

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

/** A backend keeping custom lists in memory, like `lists.json`. */
function listsBackend(initial: ListSummary[], memberships: Record<string, string[]> = {}) {
  let lists = [...initial];
  const members = new Map(Object.entries(memberships).map(([id, names]) => [id, new Set(names)]));
  const summary = (list: ListSummary) => ({
    ...list,
    machines: members.get(list.id)?.size ?? list.machines,
    entries: members.get(list.id)?.size ?? list.entries,
  });
  return {
    get lists() {
      return lists;
    },
    handlers: {
      list_installations: () => installationsView(),
      list_user_lists: () => lists.map(summary),
      create_list: (args: Args) => {
        const created = listSummary(`id-${String(lists.length + 1)}`, args.name as string);
        lists = [...lists, created];
        return created;
      },
      rename_list: (args: Args) => {
        lists = lists.map((list) =>
          list.id === args.listId ? { ...list, name: args.name as string } : list,
        );
        return lists.find((list) => list.id === args.listId);
      },
      delete_list: (args: Args) => {
        lists = lists.filter((list) => list.id !== args.listId);
        return null;
      },
      get_entry_lists: (args: Args) => {
        const name = (args.entry as { name: string }).name;
        return [...members].filter(([, names]) => names.has(name)).map(([id]) => id);
      },
      set_entry_memberships: (args: Args) => {
        const name = (args.entry as { name: string }).name;
        for (const change of args.changes as { listId: string; member: boolean }[]) {
          if (!lists.some((list) => list.id === change.listId)) {
            throw commandError("notFound", {
              message: `the list '${change.listId}' does not exist`,
            });
          }
          const set = members.get(change.listId) ?? new Set();
          if (change.member) set.add(name);
          else set.delete(name);
          members.set(change.listId, set);
        }
        return [...members].filter(([, names]) => names.has(name)).map(([id]) => id);
      },
      get_list_entries: (args: Args): ListEntriesPage => {
        const list = lists.find((entry) => entry.id === args.listId);
        if (!list) throw commandError("notFound", { message: "missing" });
        const search = ((args.search as string | null) ?? "").toLowerCase();
        const names = [...(members.get(list.id) ?? [])]
          .filter((name) => name.includes(search))
          .sort();
        const page = args.page as number;
        const pageSize = args.pageSize as number;
        return {
          list: summary(list),
          installationId: args.installationId as string,
          items: names.slice(page * pageSize, (page + 1) * pageSize).map((name) => ({
            name,
            description: name === "notamachine" ? null : `Machine ${name}`,
            inCatalog: name !== "notamachine",
          })),
          matched: names.length,
          page,
          pageSize,
        };
      },
    } satisfies Partial<Record<string, Handler>>,
  };
}

function renderPage(handlers: Partial<Record<string, Handler>>) {
  backend(handlers);
  const user = userEvent.setup();
  render(
    <InstallationsProvider>
      <CustomListsProvider>
        <ListsPage />
      </CustomListsProvider>
    </InstallationsProvider>,
  );
  return user;
}

const navigation = () => within(screen.getByRole("navigation", { name: "Lists" }));
const navItems = () =>
  within(navigation().getByRole("list", { name: "Lists" }))
    .getAllByRole("button")
    .map((button) => button.textContent);

describe("Lists page", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("lists, searches and sorts the custom lists", async () => {
    const lists = listsBackend([
      listSummary("a", "Shooters", 3, { updatedAt: 5 }),
      listSummary("b", "arcade / Pinball", 10, { updatedAt: 9 }),
      listSummary("c", "Mazes", 1, { updatedAt: 1 }),
    ]);
    const user = renderPage(lists.handlers);

    await waitFor(() => {
      expect(navItems()).toEqual(["arcade / Pinball10", "Mazes1", "Shooters3"]);
    });
    expect(navigation().getByText("3 lists")).toBeInTheDocument();

    await user.selectOptions(navigation().getByRole("combobox", { name: "Sort by" }), "machines");
    expect(navItems()).toEqual(["arcade / Pinball10", "Shooters3", "Mazes1"]);
    await user.selectOptions(navigation().getByRole("combobox", { name: "Sort by" }), "updated");
    expect(navItems()[0]).toBe("arcade / Pinball10");

    await user.type(navigation().getByRole("searchbox", { name: "Search lists" }), "SHOOT");
    expect(navItems()).toEqual(["Shooters3"]);
    expect(navigation().getByText("1 of 3 lists")).toBeInTheDocument();
  });

  it("creates empty lists and refuses names in use, ignoring case", async () => {
    const lists = listsBackend([listSummary("a", "Shooters")]);
    const user = renderPage(lists.handlers);
    await user.click(await screen.findByRole("button", { name: "New list" }));

    const name = screen.getByLabelText("Name");
    await user.type(name, "shooters");
    expect(screen.getByText(/A list named "Shooters" already exists/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create list" })).toBeDisabled();

    await user.clear(name);
    await user.type(name, "Empty one");
    await user.click(screen.getByRole("button", { name: "Create list" }));

    expect(await screen.findByText('The list "Empty one" was created.')).toBeInTheDocument();
    expect(calls("create_list")).toEqual([{ name: "Empty one" }]);
    expect(await screen.findByText("This list is empty.")).toBeInTheDocument();
    expect(navItems()).toContain("Empty one0");
  });

  it("renames a list by id and never merges into another one", async () => {
    const lists = listsBackend([listSummary("a", "Shooters"), listSummary("b", "Mazes")], {
      a: ["1942"],
    });
    const user = renderPage(lists.handlers);
    await user.click(await navigation().findByRole("button", { name: /Shooters/ }));
    await user.click(await screen.findByRole("button", { name: "Rename…" }));

    const field = screen.getByLabelText("New name");
    await user.clear(field);
    await user.type(field, "MAZES");
    expect(screen.getByText(/Renaming never merges lists/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();

    await user.clear(field);
    await user.type(field, "Vertical shooters");
    await user.click(screen.getByRole("button", { name: "Rename" }));

    await waitFor(() => {
      expect(calls("rename_list")).toEqual([{ listId: "a", name: "Vertical shooters" }]);
    });
    expect(
      await screen.findByRole("heading", { level: 2, name: "Vertical shooters" }),
    ).toBeInTheDocument();
    // Same id, same machines.
    expect(screen.getByText("1942")).toBeInTheDocument();
  });

  it("deletes a list only after confirmation", async () => {
    const lists = listsBackend([listSummary("a", "Shooters", 2), listSummary("b", "Mazes")]);
    const user = renderPage(lists.handlers);
    await user.click(await navigation().findByRole("button", { name: /Shooters/ }));
    await user.click(await screen.findByRole("button", { name: "Delete…" }));

    const confirm = screen.getByRole("alertdialog", { name: "Delete list" });
    expect(
      within(confirm).getByText(/Favorites and the other lists are not changed/),
    ).toBeVisible();
    await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(calls("delete_list")).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Delete…" }));
    await user.click(screen.getByRole("button", { name: "Delete list" }));
    expect(await screen.findByText('The list "Shooters" was deleted.')).toBeInTheDocument();
    expect(calls("delete_list")).toEqual([{ listId: "a" }]);
    await waitFor(() => {
      expect(navItems()).toEqual(["Mazes0"]);
    });
  });

  it("shows the machines a page at a time, with missing ones, and removes them", async () => {
    const names = Array.from({ length: 60 }, (_, index) => `m${String(index).padStart(2, "0")}`);
    const lists = listsBackend([listSummary("a", "Big")], { a: [...names, "notamachine"] });
    const user = renderPage(lists.handlers);
    await user.click(await navigation().findByRole("button", { name: /Big/ }));

    expect(await screen.findByText("61 machines")).toBeInTheDocument();
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("row")).toHaveLength(51);
    expect(calls("get_list_entries").at(-1)).toEqual({
      listId: "a",
      installationId: NEW_ID,
      search: null,
      page: 0,
      pageSize: 50,
    });
    await user.click(screen.getByRole("button", { name: "Page 2" }));
    const missing = await screen.findByText("notamachine");
    const row = within(missing.closest("tr") ?? document.body);
    expect(row.getByText("Not in this catalog")).toBeInTheDocument();

    await user.click(row.getByRole("button", { name: "Remove notamachine from the list" }));
    await waitFor(() => {
      expect(calls("set_entry_memberships")).toEqual([
        {
          entry: { kind: "machine", name: "notamachine" },
          changes: [{ listId: "a", member: false }],
        },
      ]);
    });
    expect(await screen.findByText("60 machines")).toBeInTheDocument();
  });

  it("searches the machines of a list in the backend", async () => {
    const lists = listsBackend([listSummary("a", "Mazes")], { a: ["pacman", "mspacman", "dkong"] });
    const user = renderPage(lists.handlers);
    await user.click(await navigation().findByRole("button", { name: /Mazes/ }));
    await screen.findByText("3 machines");

    await user.type(
      screen.getByRole("searchbox", { name: "Search the machines of the list" }),
      "pac",
    );
    expect(await screen.findByText("2 machines match")).toBeInTheDocument();
    expect(calls("get_list_entries").at(-1)).toMatchObject({ search: "pac", page: 0 });
  });

  it("adds machines found in the catalog", async () => {
    const lists = listsBackend([listSummary("a", "Mazes")]);
    const user = renderPage({
      ...lists.handlers,
      search_catalog_machines: () =>
        response([machine("pacman", { description: "Pac-Man" })], { total: 1 }),
    });
    await user.click(await navigation().findByRole("button", { name: /Mazes/ }));
    await user.type(
      await screen.findByRole("searchbox", { name: "Search the catalog to add machines" }),
      "pac",
    );
    await user.click(await screen.findByRole("button", { name: "Add pacman to the list" }));

    expect(await screen.findByRole("button", { name: "Add pacman to the list" })).toHaveTextContent(
      "Added",
    );
    expect(calls("search_catalog_machines").at(-1)).toEqual({
      installationId: NEW_ID,
      request: { text: "pac", pageSize: 10 },
    });
    expect(await screen.findByText("1 machine")).toBeInTheDocument();
  });
});

const FILE: FolderFileSummary = {
  importId: 4,
  fileName: "category.ini",
  suggestedName: "Category",
  rootEntries: 0,
  folders: 3,
  emptyFolders: 1,
  entries: 7,
  duplicates: 0,
  warnings: [{ kind: "invalidLine", count: 2, examples: ["line 9: Name=x"] }],
};

function preview(name: string, naming: "subfolder" | "prefixed"): FolderImportPreview {
  const prefix = naming === "prefixed" ? `${name} / ` : "";
  return {
    importId: 4,
    fileName: "category.ini",
    name,
    naming,
    hasSubfolders: true,
    toCreate: 2,
    toMerge: 1,
    emptyFolders: 1,
    newEntries: 5,
    existingEntries: 2,
    duplicates: 0,
    warnings: FILE.warnings,
    lists: [
      { name: `${prefix}Arcade / Pinball`, action: "create", entries: 2, newEntries: 2 },
      { name: `${prefix}Maze / Collect`, action: "merge", entries: 3, newEntries: 1 },
      { name: `${prefix}Shooter`, action: "create", entries: 2, newEntries: 2 },
    ],
  };
}

describe("Folder file import", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("previews the lists for the chosen name and naming, then imports on confirmation", async () => {
    const lists = listsBackend([listSummary("m", "Maze / Collect")]);
    const user = renderPage({
      ...lists.handlers,
      choose_folder_import_file: () => FILE,
      preview_folder_import: (args) =>
        preview(args.name as string, args.naming as "subfolder" | "prefixed"),
      apply_folder_import: () => ({ created: 2, merged: 1, entriesAdded: 5 }),
    });
    await user.click(await screen.findByRole("button", { name: "Import INI…" }));

    const dialog = within(await screen.findByRole("dialog", { name: "Import lists" }));
    expect(
      dialog.getByText(/an empty root folder, 3 subfolders \(1 empty, ignored\)/),
    ).toBeVisible();
    expect(
      dialog.getByText(/Lines that are not MAME short names were ignored \(2\)/),
    ).toBeVisible();
    expect(dialog.getByLabelText("Import name")).toHaveValue("Category");
    const resultingLists = () => within(dialog.getByRole("list", { name: "Resulting lists" }));
    expect(await resultingLists().findByText("Category / Arcade / Pinball")).toBeInTheDocument();
    expect(dialog.getByText("2 lists to create")).toBeInTheDocument();
    expect(dialog.getByText(/1 existing list to update/)).toBeInTheDocument();
    expect(dialog.getByText("1 empty subfolder ignored")).toBeInTheDocument();
    expect(dialog.getByText("5 machines to add")).toBeInTheDocument();
    // Nothing is applied by the preview.
    expect(calls("apply_folder_import")).toEqual([]);

    await user.click(dialog.getByRole("radio", { name: /By subfolder name/ }));
    expect(await resultingLists().findByText("Arcade / Pinball")).toBeInTheDocument();
    await user.type(dialog.getByRole("searchbox", { name: "Search the resulting lists" }), "maze");
    const resulting = within(dialog.getByRole("list", { name: "Resulting lists" }));
    expect(resulting.getAllByRole("listitem")).toHaveLength(1);
    expect(resulting.getByText("Update")).toBeInTheDocument();

    await user.click(dialog.getByRole("button", { name: "Import" }));
    expect(
      await screen.findByText(
        "Imported category.ini: 2 lists created, 1 list updated, 5 machines added.",
      ),
    ).toBeInTheDocument();
    expect(calls("apply_folder_import")).toEqual([
      { importId: 4, name: "Category", naming: "subfolder" },
    ]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("reports files that cannot be imported and imports nothing when cancelled", async () => {
    let failing = true;
    const lists = listsBackend([]);
    const user = renderPage({
      ...lists.handlers,
      choose_folder_import_file: () => {
        if (failing)
          throw commandError("invalidFile", { message: "the file contains no machines" });
        return FILE;
      },
      preview_folder_import: (args) => preview(args.name as string, "prefixed"),
    });
    await user.click(await screen.findByRole("button", { name: "Import INI…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("the file contains no machines");

    failing = false;
    await user.click(screen.getByRole("button", { name: "Import INI…" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Import lists" }));
    await user.clear(dialog.getByLabelText("Import name"));
    expect(await dialog.findByText("Enter a name for the import.")).toBeInTheDocument();
    expect(dialog.getByRole("button", { name: "Import" })).toBeDisabled();
    await user.click(dialog.getByRole("button", { name: "Cancel" }));
    expect(calls("apply_folder_import")).toEqual([]);
  });
});

function renderManage(
  handlers: Partial<Record<string, Handler>>,
  entry = { kind: "machine" as const, name: "pacman" },
) {
  backend(handlers);
  const user = userEvent.setup();
  const onCard = vi.fn();
  render(
    <CustomListsProvider>
      {/* Next to a card button, as in the renderers. */}
      <button type="button" onClick={onCard}>
        Open card
      </button>
      <ManageListsButton entry={entry} title="Pac-Man" variant="overlay" />
    </CustomListsProvider>,
  );
  return { user, onCard };
}

describe("Manage Lists", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  const LISTS = [listSummary("a", "Mazes"), listSummary("b", "Namco"), listSummary("c", "Retro")];

  it("checks the lists of the machine and saves only the changes, at once", async () => {
    const lists = listsBackend(LISTS, { a: ["pacman"], c: ["pacman"] });
    const { user, onCard } = renderManage(lists.handlers);
    await user.click(screen.getByRole("button", { name: "Manage lists: Pac-Man" }));
    expect(onCard).not.toHaveBeenCalled();

    const dialog = within(await screen.findByRole("dialog", { name: "Manage Lists" }));
    expect(dialog.getByText("Pac-Man")).toBeInTheDocument();
    expect(dialog.getByText("pacman")).toBeInTheDocument();
    expect(await dialog.findByRole("checkbox", { name: /Mazes/ })).toBeChecked();
    expect(dialog.getByRole("checkbox", { name: /Namco/ })).not.toBeChecked();
    expect(dialog.getByText("In 2 of 3 lists")).toBeInTheDocument();

    await user.click(dialog.getByRole("checkbox", { name: /Mazes/ }));
    await user.click(dialog.getByRole("checkbox", { name: /Namco/ }));
    // Toggled twice: no change for this list.
    await user.click(dialog.getByRole("checkbox", { name: /Retro/ }));
    await user.click(dialog.getByRole("checkbox", { name: /Retro/ }));
    expect(calls("set_entry_memberships")).toEqual([]);
    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(calls("set_entry_memberships")).toEqual([
      {
        entry: { kind: "machine", name: "pacman" },
        changes: [
          { listId: "a", member: false },
          { listId: "b", member: true },
        ],
      },
    ]);
  });

  it("discards the changes on Cancel", async () => {
    const lists = listsBackend(LISTS, { a: ["pacman"] });
    const { user } = renderManage(lists.handlers);
    await user.click(screen.getByRole("button", { name: "Manage lists: Pac-Man" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Manage Lists" }));
    await user.click(await dialog.findByRole("checkbox", { name: /Namco/ }));
    await user.click(dialog.getByRole("button", { name: "Cancel" }));
    expect(calls("set_entry_memberships")).toEqual([]);

    // Opened again: the saved state, not the discarded draft.
    await user.click(screen.getByRole("button", { name: "Manage lists: Pac-Man" }));
    const again = within(await screen.findByRole("dialog", { name: "Manage Lists" }));
    expect(await again.findByRole("checkbox", { name: /Namco/ })).not.toBeChecked();
  });

  it("searches hundreds of lists and creates a list checked for the machine", async () => {
    const many = Array.from({ length: 650 }, (_, index) =>
      listSummary(`l${String(index)}`, `Category / Folder ${String(index).padStart(3, "0")}`),
    );
    const lists = listsBackend(many);
    const { user } = renderManage(lists.handlers);
    await user.click(screen.getByRole("button", { name: "Manage lists: Pac-Man" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Manage Lists" }));
    const options = within(await dialog.findByRole("list", { name: "Lists" }));
    // Bounded rendering: the others are found by searching.
    expect(options.getAllByRole("checkbox")).toHaveLength(300);
    expect(dialog.getByText("350 more lists: refine the search to see them.")).toBeInTheDocument();
    await user.type(dialog.getByLabelText("Search lists"), "folder 64");
    expect(options.getAllByRole("checkbox")).toHaveLength(10);

    await user.type(dialog.getByLabelText("New list name"), "category / folder 000");
    await user.click(dialog.getByRole("button", { name: "Create list" }));
    expect(dialog.getByRole("alert")).toHaveTextContent(
      'A list named "Category / Folder 000" already exists.',
    );
    await user.clear(dialog.getByLabelText("New list name"));
    await user.type(dialog.getByLabelText("New list name"), "Favorites to try");
    await user.click(dialog.getByRole("button", { name: "Create list" }));
    await user.clear(dialog.getByLabelText("Search lists"));
    await user.type(dialog.getByLabelText("Search lists"), "to try");
    expect(await dialog.findByRole("checkbox", { name: /Favorites to try/ })).toBeChecked();

    await user.click(dialog.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(calls("set_entry_memberships")).toEqual([
        {
          entry: { kind: "machine", name: "pacman" },
          changes: [{ listId: "id-651", member: true }],
        },
      ]);
    });
  });

  it("keeps the window open with an error when a list was deleted meanwhile", async () => {
    const lists = listsBackend(LISTS);
    const { user } = renderManage({
      ...lists.handlers,
      set_entry_memberships: () => {
        throw commandError("notFound", { message: "the list 'b' does not exist" });
      },
    });
    await user.click(screen.getByRole("button", { name: "Manage lists: Pac-Man" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Manage Lists" }));
    await user.click(await dialog.findByRole("checkbox", { name: /Namco/ }));
    await user.click(dialog.getByRole("button", { name: "Save" }));

    expect(await dialog.findByRole("alert")).toHaveTextContent("the list 'b' does not exist");
    expect(screen.getByRole("dialog", { name: "Manage Lists" })).toBeInTheDocument();
    // The lists were read again to show the current state.
    expect(calls("list_user_lists").length).toBeGreaterThan(1);
  });
});

function Picker({
  lists,
  initial = null,
}: {
  lists: ListSummary[] | null;
  initial?: string | null;
}) {
  const [value, setValue] = useState<string | null>(initial);
  return (
    <>
      <ListCombobox label="List" value={value} lists={lists} onChange={setValue} />
      <output aria-label="Selected">{value ?? "all"}</output>
    </>
  );
}

describe("List combobox", () => {
  const LISTS = Array.from({ length: 450 }, (_, index) =>
    listSummary(`l${String(index)}`, `List ${String(index).padStart(3, "0")}`, index),
  );

  it("filters hundreds of lists by name and chooses with the keyboard", async () => {
    const user = userEvent.setup();
    render(<Picker lists={LISTS} />);
    const input = screen.getByRole("combobox", { name: "List" });
    expect(input).toHaveValue("All items");

    await user.click(input);
    const listbox = screen.getByRole("listbox", { name: "List" });
    // "All items" and the first 200 lists; the others are found by typing.
    expect(within(listbox).getAllByRole("option")).toHaveLength(201);
    expect(within(listbox).getByText("250 more: type to narrow the list.")).toBeInTheDocument();

    await user.type(input, "List 44");
    const options = within(listbox).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "All items",
      ...Array.from({ length: 10 }, (_, index) => `List 44${String(index)}${String(440 + index)}`),
    ]);
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(screen.getByLabelText("Selected")).toHaveTextContent("l441");
    expect(input).toHaveValue("List 441");

    await user.click(input);
    await user.click(screen.getByRole("option", { name: "All items" }));
    expect(screen.getByLabelText("Selected")).toHaveTextContent("all");
  });

  it("closes on Escape without choosing and names a deleted list", async () => {
    const user = userEvent.setup();
    render(<Picker lists={LISTS.slice(0, 3)} initial="gone" />);
    const input = screen.getByRole("combobox", { name: "List" });
    expect(input).toHaveValue("Deleted list");
    await user.click(input);
    await user.type(input, "zzz");
    expect(screen.getByText("No list matches “zzz”.")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Selected")).toHaveTextContent("gone");
  });
});

describe("Lists styles", () => {
  it("take every color from the theme tokens, so light and dark both apply", () => {
    const css = readFileSync(resolve("src/features/lists/lists.css"), "utf8");
    expect(css).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    expect(css).not.toMatch(/\b(rgb|rgba|hsl|hsla)\(/i);
    for (const color of css.matchAll(/(?:color|background|border-color):\s*([^;]+);/g)) {
      expect(color[1]).toMatch(/^(var\(--[a-z-]+\)|transparent|inherit|currentColor)$/);
    }
  });
});
