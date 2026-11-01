import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { catalogStatus, machine, machineDetail, response } from "../../test/catalogFixtures";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { launchConfigView } from "../../test/launcherFixtures";
import { favoritesList } from "../../test/listFixtures";
import { mediaSourcesView } from "../../test/mediaFixtures";
import { NO_CLASSIFICATIONS, classificationStatus } from "../../test/supportFileFixtures";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import { useInstallations } from "../installations/installationsContext";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { CustomListsProvider } from "../lists/CustomListsProvider";
import { FavoritesProvider } from "../lists/FavoritesProvider";
import { MediaProvider } from "../media/MediaProvider";
import type { SavedFilter } from "../saved-filters/savedFiltersApi";
import type { FacetResult, MachineFacet, MachineSearchRequest } from "../search/searchApi";
import type { ClassificationStatus, DimensionValues } from "../support-files/supportFilesApi";
import { DEFAULT_BROWSER_PREFERENCES, type BrowserPreferences } from "./browserPreferences";
import { CatalogBrowser } from "./CatalogBrowser";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  convertFileSrc: (path: string, protocol: string) =>
    `${protocol}://localhost/${encodeURIComponent(path)}`,
}));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

const MACHINES = [
  machine("1942", { description: "1942", manufacturer: "Capcom" }),
  machine("1943", { description: "1943: The Battle of Midway", manufacturer: "Capcom" }),
  machine("pacman", { description: "Pac-Man" }),
];

const VALUES: Partial<Record<MachineFacet, { value: string; count: number }[]>> = {
  manufacturer: [{ value: "Capcom", count: 2 }],
  genre: [
    { value: "Shooter", count: 3 },
    { value: "Maze", count: 2 },
  ],
  category: [
    { value: "Maze", count: 2 },
    { value: "Maze / Collect", count: 2 },
    { value: "Shooter", count: 3 },
    { value: "Shooter / Flying Vertical", count: 2 },
    { value: "Shooter / Gallery", count: 1 },
  ],
  series: [
    { value: "194X", count: 3 },
    { value: "Pac-Man", count: 2 },
  ],
  // In MAME version order, as the backend returns them.
  versionAdded: [
    { value: "0.1", count: 1 },
    { value: "0.10", count: 1 },
    { value: "0.34b4", count: 2 },
    { value: "0.288", count: 1 },
  ],
};

function machineSearch(args: Args) {
  const request = (args as { request: MachineSearchRequest }).request;
  const facets: FacetResult<MachineFacet>[] = (request.facets ?? []).map(({ facet, limit }) => ({
    facet,
    values: VALUES[facet] ?? [],
    // Like the 720 series of MAME 0.288: more than the panel shows.
    hasMore: facet === "series" && (limit ?? 50) < 1000,
  }));
  return response(MACHINES, {
    page: request.page ?? 0,
    pageSize: request.pageSize ?? 50,
    facets,
  });
}

const ALL: ClassificationStatus = classificationStatus([
  "genre",
  "category",
  "series",
  "versionAdded",
]);

function commands(overrides: Partial<Record<string, Handler>> = {}) {
  let active = NEW_ID;
  const installations = [installation(NEW_ID, "0.280"), installation(OLD_ID, "0.252")];
  const handlers: Partial<Record<string, Handler>> = {
    list_installations: () => installationsView(installations, active),
    set_active_installation: (args) => {
      active = args.installationId as string;
      return installationsView(installations, active);
    },
    get_catalog_status: (args) => catalogStatus({ installationId: args.installationId as string }),
    search_catalog_machines: machineSearch,
    get_classification_status: () => ALL,
    find_missing_classification_values: () => [],
    get_saved_filters: () => [],
    get_launch_config: () => launchConfigView(),
    get_launch_sessions: () => [],
    get_software_launch_info: () => null,
    get_media_sources: () => mediaSourcesView(),
    list_media: () => [],
    get_list: () => favoritesList([]),
    list_user_lists: () => [],
    ...overrides,
  };
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
const machineRequests = () =>
  calls("search_catalog_machines").map(
    (args) => (args as { request: MachineSearchRequest }).request,
  );
const resultRequests = () => machineRequests().filter((request) => request.direction);
const lastRequest = () => resultRequests().at(-1);

function SwitchToOld() {
  const { select } = useInstallations();
  return (
    <button
      type="button"
      onClick={() => {
        select(OLD_ID);
      }}
    >
      Activate MAME 0.252
    </button>
  );
}

function Providers({ children }: { children: ReactNode }) {
  return (
    <MediaProvider>
      <InstallationsProvider>
        <SwitchToOld />
        <FavoritesProvider>
          <CustomListsProvider>
            <LauncherProvider
              installationId={NEW_ID}
              showCommandPreview={false}
              onShowCommandPreviewChange={() => undefined}
            >
              {children}
            </LauncherProvider>
          </CustomListsProvider>
        </FavoritesProvider>
      </InstallationsProvider>
    </MediaProvider>
  );
}

function renderBrowser(preferences: Partial<BrowserPreferences> = {}) {
  const user = userEvent.setup();
  const onPreferencesChange = vi.fn();
  const onOpenSupportFiles = vi.fn();
  const props: ComponentProps<typeof CatalogBrowser> = {
    refreshKey: 0,
    preferences: { ...DEFAULT_BROWSER_PREFERENCES, ...preferences },
    onPreferencesChange,
    onOpenMame: () => undefined,
    onOpenSupportFiles,
    debounceMs: 10,
  };
  render(
    <Providers>
      <CatalogBrowser {...props} />
    </Providers>,
  );
  return { user, onPreferencesChange, onOpenSupportFiles };
}

const filterPanel = async () =>
  within(await screen.findByRole("complementary", { name: "Filters" }));

describe("classification filters", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("are shown only when the catalog has them, with a way to add them", async () => {
    commands({ get_classification_status: () => NO_CLASSIFICATIONS });
    const { user, onOpenSupportFiles } = renderBrowser();
    const filters = await filterPanel();
    expect(await filters.findByText(/Import MAME support files/)).toBeInTheDocument();
    expect(filters.queryByRole("group", { name: "Genre" })).not.toBeInTheDocument();
    await user.click(filters.getByRole("button", { name: "Open Support Files settings" }));
    expect(onOpenSupportFiles).toHaveBeenCalled();
    // No classification facet is requested without classifications.
    expect(
      machineRequests()
        .flatMap((request) => request.facets ?? [])
        .map((f) => f.facet),
    ).not.toContain("genre");
  });

  it("report a projection that could not be updated, with Retry", async () => {
    commands({ get_classification_status: () => classificationStatus(["genre"], "disk full") });
    renderBrowser();
    const filters = await filterPanel();
    expect(await filters.findByRole("alert")).toHaveTextContent(
      "The classifications of this catalog could not be updated: disk full",
    );
    // The previous classifications stay usable.
    expect(await filters.findByRole("group", { name: "Genre" })).toBeInTheDocument();
  });

  it("combine values with OR within a dimension and AND across dimensions", async () => {
    commands();
    const { user } = renderBrowser();
    const filters = await filterPanel();
    const genre = within(await filters.findByRole("group", { name: "Genre" }));
    await user.click(await genre.findByRole("checkbox", { name: /Shooter/ }));
    const series = within(filters.getByRole("group", { name: "Series" }));
    await user.click(series.getByRole("checkbox", { name: /194X/ }));
    await user.click(series.getByRole("checkbox", { name: /Pac-Man/ }));

    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ genres: ["Shooter"], series: ["194X", "Pac-Man"] });
    });
    expect(lastRequest()?.page).toBe(0);
    // The selected count is shown in the title.
    expect(filters.getByRole("group", { name: /^Series \(2\)/ })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Active filters" })).toHaveTextContent("Series: 194X");
  });

  it("select a category with its descendants, and search the tree", async () => {
    commands();
    const { user } = renderBrowser();
    const filters = await filterPanel();
    const category = within(await filters.findByRole("group", { name: "Category" }));
    await user.click(await category.findByRole("button", { name: "Expand Shooter" }));
    await user.click(category.getByRole("checkbox", { name: "Shooter" }));

    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ categories: ["Shooter"] });
    });
    const child = category.getByRole("checkbox", { name: "Shooter / Flying Vertical" });
    expect(child).toBeChecked();
    expect(child).toBeDisabled();

    // Searching filters the values, not the selection.
    await user.type(category.getByLabelText("Search Category values"), "gallery");
    expect(category.getByRole("checkbox", { name: "Shooter / Gallery" })).toBeInTheDocument();
    expect(category.queryByRole("checkbox", { name: "Maze" })).not.toBeInTheDocument();
    expect(lastRequest()?.filters).toEqual({ categories: ["Shooter"] });
  });

  it("filter version added by a range in MAME version order", async () => {
    commands();
    const { user } = renderBrowser();
    const filters = await filterPanel();
    const from = await filters.findByRole("combobox", { name: "Version Added from" });
    await waitFor(() => {
      expect(
        within(from)
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual(["Any", "0.1", "0.10", "0.34b4", "0.288"]);
    });
    await user.selectOptions(from, "0.10");
    await user.selectOptions(filters.getByRole("combobox", { name: "Version Added to" }), "0.34b4");
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ versionAddedRange: { from: "0.10", to: "0.34b4" } });
    });
    expect(screen.getByRole("group", { name: "Active filters" })).toHaveTextContent(
      "Version Added: 0.10 – 0.34b4",
    );
    // Exact versions are listed newest first.
    const versions = within(filters.getByRole("list", { name: "Version Added values" }));
    expect(
      versions.getAllByRole("checkbox").map((box) => box.closest("label")?.textContent),
    ).toEqual(["0.2881", "0.34b42", "0.101", "0.11"]);
  });
});

describe("Browse all", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("applies or cancels a draft selection", async () => {
    commands();
    const { user } = renderBrowser();
    const filters = await filterPanel();
    const series = within(await filters.findByRole("group", { name: "Series" }));
    await user.click(await series.findByRole("checkbox", { name: /Pac-Man/ }));
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ series: ["Pac-Man"] });
    });
    const searches = resultRequests().length;

    await user.click(series.getByRole("button", { name: "Browse all Series values" }));
    let dialog = within(await screen.findByRole("dialog", { name: "Series" }));
    expect(await dialog.findByText(/^1 selected/)).toBeInTheDocument();
    await user.click(dialog.getByRole("checkbox", { name: /194X/ }));
    expect(dialog.getByText(/^2 selected/)).toBeInTheDocument();
    // Cancel leaves the filters untouched.
    await user.click(dialog.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(resultRequests()).toHaveLength(searches);

    await user.click(series.getByRole("button", { name: "Browse all Series values" }));
    dialog = within(await screen.findByRole("dialog", { name: "Series" }));
    // The full list loads with the current text and other filters.
    const valuesRequest = machineRequests().at(-1);
    expect(valuesRequest?.facets).toEqual([{ facet: "series", limit: 1000 }]);
    // Searching keeps the selection.
    await user.type(dialog.getByLabelText("Search Series values"), "194");
    expect(await dialog.findByRole("checkbox", { name: /Pac-Man/ })).toBeChecked();
    await user.click(await dialog.findByRole("checkbox", { name: /194X/ }));
    await user.click(dialog.getByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ series: ["Pac-Man", "194X"] });
    });

    // Clear selection, then Escape: nothing changes.
    await user.click(series.getByRole("button", { name: "Browse all Series values" }));
    dialog = within(await screen.findByRole("dialog", { name: "Series" }));
    await user.click(await dialog.findByRole("button", { name: "Clear selection" }));
    expect(dialog.getByText("0 selected")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastRequest()?.filters).toEqual({ series: ["Pac-Man", "194X"] });

    // Clear selection, then Apply: the facet is cleared.
    await user.click(series.getByRole("button", { name: "Browse all Series values" }));
    dialog = within(await screen.findByRole("dialog", { name: "Series" }));
    await user.click(await dialog.findByRole("button", { name: "Clear selection" }));
    await user.click(dialog.getByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({});
    });
  });

  it("shows the category tree with keyboard-reachable controls", async () => {
    commands();
    const { user } = renderBrowser();
    const filters = await filterPanel();
    const category = within(await filters.findByRole("group", { name: "Category" }));
    await user.click(await category.findByRole("button", { name: "Browse all Category values" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Category" }));
    // The search field has the focus; Tab reaches the tree.
    expect(dialog.getByLabelText("Search Category values")).toHaveFocus();
    await user.click(await dialog.findByRole("button", { name: "Expand Maze" }));
    await user.click(dialog.getByRole("checkbox", { name: "Maze / Collect" }));
    await user.click(dialog.getByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ categories: ["Maze / Collect"] });
    });
  });
});

describe("machine detail classifications", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("show the catalog filtered by a value", async () => {
    commands({
      get_machine_detail: () =>
        machineDetail("1942", {
          description: "1942",
          classifications: {
            genres: ["Shooter"],
            categories: ["Shooter / Flying Vertical"],
            series: ["194X"],
            versionAdded: "0.34b4",
          },
        }),
    });
    const { user } = renderBrowser();
    await user.type(await screen.findByLabelText("Search text"), "1942");
    await waitFor(() => {
      expect(lastRequest()?.text).toBe("1942");
    });
    const results = within(await screen.findByRole("list", { name: "Results" }));
    const [card] = await results.findAllByText("1942");
    if (!card) throw new Error("missing card");
    await user.click(card);

    const scope = screen;
    expect(
      await scope.findByRole("button", { name: "Show the machines of genre Shooter" }),
    ).toBeInTheDocument();
    // Every level of a category can be followed.
    expect(
      scope.getByRole("button", { name: "Show the machines of category Shooter" }),
    ).toBeInTheDocument();
    expect(
      scope.getByRole("button", {
        name: "Show the machines of category Shooter / Flying Vertical",
      }),
    ).toBeInTheDocument();
    expect(
      scope.getByRole("button", { name: "Show the machines of version 0.34b4" }),
    ).toBeInTheDocument();

    await user.click(scope.getByRole("button", { name: "Show the machines of series 194X" }));
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ series: ["194X"] });
    });
    expect(lastRequest()?.text).toBeUndefined();
    expect(await screen.findByRole("list", { name: "Results" })).toBeVisible();
  });
});

function preset(id: string, name: string, criteria: unknown, criteriaVersion = 1): SavedFilter {
  return { id, name, criteriaVersion, criteria, createdAt: 1, updatedAt: 1 };
}

const SHOOTERS = preset("shooters", "Shooters", {
  scope: "machines",
  text: "194",
  values: { genre: ["Shooter"], series: ["194X"] },
  flags: { clones: "parents" },
  ranges: { versionAdded: { from: "0.10" } },
  favoritesOnly: true,
});

describe("Saved Filters", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("saves the current filters, marks changes and updates on request", async () => {
    let presets: SavedFilter[] = [];
    commands({
      get_saved_filters: () => presets,
      create_saved_filter: (args) => {
        presets = [preset("new", args.name as string, args.criteria)];
        return { id: "new", presets };
      },
      update_saved_filter: (args) => {
        presets = [preset("new", "Mazes", args.criteria)];
        return presets;
      },
    });
    const { user } = renderBrowser({ viewOptionsVisible: true });
    const filters = await filterPanel();
    await user.click(
      await within(await filters.findByRole("group", { name: "Genre" })).findByRole("checkbox", {
        name: /Maze/,
      }),
    );
    const view = within(screen.getByRole("complementary", { name: "View options" }));
    await user.type(await view.findByLabelText("Saved filter name"), "Mazes");
    await user.click(view.getByRole("button", { name: "Save current filters" }));

    await waitFor(() => {
      expect(calls("create_saved_filter")).toEqual([
        {
          name: "Mazes",
          criteriaVersion: 2,
          criteria: {
            scope: "machines",
            text: "",
            values: { genre: ["Maze"] },
            flags: {},
            ranges: {},
            favoritesOnly: false,
            listId: null,
          },
        },
      ]);
    });
    expect(await view.findByText("Mazes", { selector: "strong" })).toBeInTheDocument();
    expect(view.queryByText("Modified")).not.toBeInTheDocument();

    // A change marks the preset as modified; it is not saved automatically.
    await user.click(
      within(filters.getByRole("group", { name: /^Genre \(1\)/ })).getByRole("checkbox", {
        name: /Shooter/,
      }),
    );
    expect(await view.findByText("Modified")).toBeInTheDocument();
    expect(calls("update_saved_filter")).toEqual([]);
    await user.click(view.getByRole("button", { name: "Update “Mazes”" }));
    await waitFor(() => {
      expect(calls("update_saved_filter")).toHaveLength(1);
    });
    expect(calls("update_saved_filter")[0]).toMatchObject({
      id: "new",
      criteria: { values: { genre: ["Maze", "Shooter"] } },
    });
    await waitFor(() => {
      expect(view.queryByText("Modified")).not.toBeInTheDocument();
    });
  });

  it("loads a preset at once and resets the page, on any installation", async () => {
    commands({ get_saved_filters: () => [SHOOTERS] });
    const { user, onPreferencesChange } = renderBrowser({ viewOptionsVisible: true });
    const view = within(await screen.findByRole("complementary", { name: "View options" }));
    await screen.findByText("1–3 of 3 machines");
    const before = resultRequests().length;

    await user.click(await view.findByRole("button", { name: "Load Shooters" }));
    await waitFor(() => {
      expect(lastRequest()).toMatchObject({
        text: "194",
        page: 0,
        filters: {
          genres: ["Shooter"],
          series: ["194X"],
          clones: "parents",
          versionAddedRange: { from: "0.10" },
        },
      });
    });
    // One search for the whole preset, not one per criterion.
    expect(resultRequests().length).toBe(before + 1);
    expect(onPreferencesChange).toHaveBeenCalledWith({ favoritesOnly: true });
    expect(screen.getByLabelText("Search text")).toHaveValue("194");
    const filters = await filterPanel();
    expect(filters.getByRole("combobox", { name: "Parent / clone" })).toHaveValue("parents");
    expect(view.getByRole("button", { name: "Load Shooters" })).toHaveAttribute(
      "aria-current",
      "true",
    );

    // The same criteria apply to the catalog of another installation.
    await user.click(screen.getByRole("button", { name: "Activate MAME 0.252" }));
    await waitFor(() => {
      expect(calls("search_catalog_machines").at(-1)?.installationId).toBe(OLD_ID);
    });
    expect(lastRequest()?.filters).toMatchObject({ genres: ["Shooter"], series: ["194X"] });
  });

  it("warns about classification criteria that match nothing in this catalog", async () => {
    commands({
      get_saved_filters: () => [SHOOTERS],
      get_classification_status: () => classificationStatus(["genre", "versionAdded"]),
      find_missing_classification_values: (args) =>
        (args.values as DimensionValues[]).filter((entry) => entry.dimension === "series"),
    });
    const { user } = renderBrowser({ viewOptionsVisible: true });
    const view = within(await screen.findByRole("complementary", { name: "View options" }));
    await user.click(await view.findByRole("button", { name: "Load Shooters" }));
    expect(
      await screen.findByText(
        /These classification criteria match nothing in this catalog: Series: 194X \(no active support file\)/,
      ),
    ).toBeInTheDocument();
    // The criterion is kept (it narrows to nothing) rather than dropped.
    expect(lastRequest()?.filters).toMatchObject({ series: ["194X"] });
    expect(screen.getByRole("group", { name: "Active filters" })).toHaveTextContent("Series: 194X");
  });

  it("renames and deletes presets after confirmation", async () => {
    let presets = [SHOOTERS];
    commands({
      get_saved_filters: () => presets,
      rename_saved_filter: (args) => {
        presets = [{ ...SHOOTERS, name: args.name as string }];
        return presets;
      },
      delete_saved_filter: () => {
        presets = [];
        return presets;
      },
    });
    const { user } = renderBrowser({ viewOptionsVisible: true });
    const view = within(await screen.findByRole("complementary", { name: "View options" }));
    await user.click(await view.findByRole("button", { name: "Rename Shooters" }));
    const input = view.getByLabelText("New name of Shooters");
    await user.clear(input);
    await user.type(input, "Vertical shooters");
    await user.click(view.getByRole("button", { name: "Rename" }));
    expect(await view.findByRole("button", { name: "Load Vertical shooters" })).toBeInTheDocument();
    expect(calls("rename_saved_filter")).toEqual([{ id: "shooters", name: "Vertical shooters" }]);

    await user.click(view.getByRole("button", { name: "Delete Vertical shooters" }));
    expect(view.getByText("Delete “Vertical shooters”?")).toBeInTheDocument();
    expect(calls("delete_saved_filter")).toEqual([]);
    await user.click(view.getByRole("button", { name: "Delete" }));
    expect(await view.findByText(/No saved filters yet/)).toBeInTheDocument();
  });

  it("keeps presets of a newer format without loading them", async () => {
    commands({ get_saved_filters: () => [preset("future", "Future", { scope: "machines" }, 9)] });
    renderBrowser({ viewOptionsVisible: true });
    const view = within(await screen.findByRole("complementary", { name: "View options" }));
    expect(await view.findByRole("button", { name: "Load Future" })).toBeDisabled();
    expect(
      view.getByText(/Saved by a newer YMDB \(format 9\); kept unchanged/),
    ).toBeInTheDocument();
  });

  it("reports criteria this version cannot apply", async () => {
    const legacy = preset("legacy", "Legacy", {
      scope: "machines",
      text: "",
      values: { genre: ["Shooter"], rating: ["5"] },
      flags: {},
      ranges: {},
      favoritesOnly: false,
    });
    commands({ get_saved_filters: () => [legacy] });
    const { user } = renderBrowser({ viewOptionsVisible: true });
    const view = within(await screen.findByRole("complementary", { name: "View options" }));
    await user.click(await view.findByRole("button", { name: "Load Legacy" }));
    expect(await screen.findByText(/were not applied: rating/)).toBeInTheDocument();
    await waitFor(() => {
      expect(lastRequest()?.filters).toEqual({ genres: ["Shooter"] });
    });
  });
});
