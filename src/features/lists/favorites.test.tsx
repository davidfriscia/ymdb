import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { favoritesList } from "../../test/listFixtures";
import { FavoritesProvider } from "./FavoritesProvider";
import { FavoritesSettings } from "./FavoritesSettings";
import type { ListEntry, ListsImportPreview } from "./listsApi";

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

const ENTRIES: ListEntry[] = [
  { kind: "machine", name: "pacman" },
  { kind: "software", listName: "nes", name: "smb" },
];

const PREVIEW: ListsImportPreview = {
  importId: 7,
  lists: [
    {
      listId: "favorites",
      name: "Favorites",
      entries: 3,
      newEntries: 2,
      existingEntries: 1,
      duplicates: 1,
      removedByReplace: 1,
    },
  ],
  ignoredLists: ["Shoot 'em ups"],
};

function renderSettings() {
  const user = userEvent.setup();
  render(
    <FavoritesProvider>
      <FavoritesSettings />
    </FavoritesProvider>,
  );
  return user;
}

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

describe("FavoritesSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("exports the favorites to a file chosen by the user", async () => {
    let path: string | null = null;
    backend({ get_list: () => favoritesList(ENTRIES), export_lists: () => path });
    const user = renderSettings();
    expect(await screen.findByText("2 favorites")).toBeInTheDocument();

    // Cancelled in the save dialog: nothing to report.
    await user.click(screen.getByRole("button", { name: "Export favorites…" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Export favorites…" })).toBeEnabled();
    });
    expect(screen.queryByText(/exported/)).not.toBeInTheDocument();

    path = "C:/Users/me/ymdb-favorites ✓.json";
    await user.click(screen.getByRole("button", { name: "Export favorites…" }));

    expect(
      await screen.findByText("Favorites exported to C:/Users/me/ymdb-favorites ✓.json."),
    ).toBeInTheDocument();
    expect(calls("export_lists")).toEqual([{ listIds: ["favorites"] }, { listIds: ["favorites"] }]);
  });

  it("merges an import after showing what it adds", async () => {
    let entries = ENTRIES.slice(0, 1);
    backend({
      get_list: () => favoritesList(entries),
      preview_lists_import: () => PREVIEW,
      apply_lists_import: () => {
        entries = ENTRIES;
        return [{ listId: "favorites", added: 2, removed: 0, entries: 3 }];
      },
    });
    const user = renderSettings();
    expect(await screen.findByText("1 favorite")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Import favorites…" }));

    const summary = within(await screen.findByRole("group", { name: "Import summary" }));
    expect(summary.getByText("3 in the file")).toBeInTheDocument();
    expect(summary.getByText("2 new")).toBeInTheDocument();
    expect(summary.getByText("1 already in your favorites")).toBeInTheDocument();
    expect(summary.getByText("1 duplicates in the file, imported once")).toBeInTheDocument();
    expect(
      summary.getByText(/Other lists in the file are not imported: Shoot 'em ups/),
    ).toBeVisible();
    // Nothing is applied before a choice.
    expect(calls("apply_lists_import")).toEqual([]);

    await user.click(summary.getByRole("button", { name: "Merge" }));

    expect(
      await screen.findByText("Favorites imported: 2 added, 0 removed, 3 in total."),
    ).toBeInTheDocument();
    expect(calls("apply_lists_import")).toEqual([{ importId: 7, mode: "merge" }]);
    // The favorites are read again for every view.
    expect(await screen.findByText("2 favorites")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Import summary" })).not.toBeInTheDocument();
  });

  it("asks for an explicit confirmation before replacing", async () => {
    backend({
      get_list: () => favoritesList(ENTRIES),
      preview_lists_import: () => PREVIEW,
      apply_lists_import: () => [{ listId: "favorites", added: 2, removed: 1, entries: 3 }],
    });
    const user = renderSettings();
    await screen.findByText("2 favorites");
    await user.click(screen.getByRole("button", { name: "Import favorites…" }));
    const summary = within(await screen.findByRole("group", { name: "Import summary" }));

    await user.click(summary.getByRole("button", { name: "Replace…" }));
    expect(summary.getByRole("alert")).toHaveTextContent(
      "Replace your favorites with the 3 of the file? 1 favorites that are not in the file will be removed.",
    );
    await user.click(summary.getByRole("button", { name: "Back" }));
    expect(summary.queryByRole("button", { name: "Replace favorites" })).not.toBeInTheDocument();
    expect(calls("apply_lists_import")).toEqual([]);

    await user.click(summary.getByRole("button", { name: "Replace…" }));
    await user.click(summary.getByRole("button", { name: "Replace favorites" }));

    expect(
      await screen.findByText("Favorites imported: 2 added, 1 removed, 3 in total."),
    ).toBeInTheDocument();
    expect(calls("apply_lists_import")).toEqual([{ importId: 7, mode: "replace" }]);
  });

  it("cancels an import without changing the favorites", async () => {
    backend({ get_list: () => favoritesList(ENTRIES), preview_lists_import: () => PREVIEW });
    const user = renderSettings();
    await screen.findByText("2 favorites");
    await user.click(screen.getByRole("button", { name: "Import favorites…" }));
    const summary = within(await screen.findByRole("group", { name: "Import summary" }));

    await user.click(summary.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("group", { name: "Import summary" })).not.toBeInTheDocument();
    expect(calls("apply_lists_import")).toEqual([]);
  });

  it("explains a file that cannot be imported", async () => {
    backend({
      get_list: () => favoritesList(ENTRIES),
      preview_lists_import: () => {
        throw Object.assign(new Error("invalidFile"), {
          kind: "invalidFile",
          message: "the file has version 2; this version of YMDB reads version 1",
        });
      },
    });
    const user = renderSettings();
    await screen.findByText("2 favorites");

    await user.click(screen.getByRole("button", { name: "Import favorites…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The file cannot be imported: the file has version 2; this version of YMDB reads version 1",
    );
    expect(screen.queryByRole("group", { name: "Import summary" })).not.toBeInTheDocument();
  });

  it("reports favorites that cannot be read and retries", async () => {
    let attempts = 0;
    backend({
      get_list: () => {
        attempts++;
        if (attempts === 1) {
          throw Object.assign(new Error("settings"), { kind: "settings", message: "invalid" });
        }
        return favoritesList(ENTRIES);
      },
    });
    const user = renderSettings();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The favorites could not be loaded: Settings error: invalid",
    );
    expect(screen.getByRole("button", { name: "Import favorites…" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByText("2 favorites")).toBeInTheDocument();
  });
});
