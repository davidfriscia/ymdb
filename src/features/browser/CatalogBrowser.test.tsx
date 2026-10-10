import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  catalogStatus,
  emptyCatalogStatus,
  machine,
  machineDetail,
  response,
  software,
  softwareDetail,
  softwareListDetail,
} from "../../test/catalogFixtures";
import type { CatalogStatus } from "../catalog/catalogApi";
import type {
  FacetResult,
  MachineFacet,
  MachineSearchRequest,
  MachineSummary,
  SoftwareSearchRequest,
} from "../search/searchApi";
import { DEFAULT_BROWSER_PREFERENCES, type BrowserPreferences } from "./browserPreferences";
import { MediaProvider } from "../media/MediaProvider";
import { mediaSourcesView } from "../../test/mediaFixtures";
import { CatalogBrowser } from "./CatalogBrowser";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { launchConfigView } from "../../test/launcherFixtures";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  convertFileSrc: (path: string, protocol: string) =>
    `${protocol}://localhost/${encodeURIComponent(path)}`,
}));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

/** Routes mocked IPC calls by command name; handlers may throw or return promises. */
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

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const MACHINES = [
  machine("pacman", { description: "Pac-Man" }),
  machine("mspacman", { description: "Ms. Pac-Man", cloneOf: "pacman", manufacturer: "Midway" }),
  machine("galaga", { description: "Galaga", year: "1981", driverStatus: "imperfect" }),
];

function facets(
  manufacturers: { value: string | null; count: number }[] = [
    { value: "Namco", count: 2 },
    { value: "Midway", count: 1 },
  ],
  hasMore = false,
): FacetResult<MachineFacet>[] {
  return [
    { facet: "manufacturer", values: manufacturers, hasMore },
    {
      facet: "year",
      values: [
        { value: "1980", count: 2 },
        { value: "1981", count: 1 },
      ],
      hasMore: false,
    },
    { facet: "driverStatus", values: [{ value: "good", count: 3 }], hasMore: false },
    { facet: "emulationStatus", values: [{ value: "good", count: 3 }], hasMore: false },
    { facet: "softwareList", values: [], hasMore: false },
  ];
}

const machineRequest = (args: Args) => (args as { request: MachineSearchRequest }).request;
const softwareRequest = (args: Args) => (args as { request: SoftwareSearchRequest }).request;

/** Answers machine searches with the requested page of `items`. */
function machineSearch(
  items: MachineSummary[] = MACHINES,
  options: { total?: number; facets?: FacetResult<MachineFacet>[] } = {},
): Handler {
  return (args) => {
    const request = machineRequest(args);
    return response(items, {
      page: request.page ?? 0,
      pageSize: request.pageSize ?? 50,
      total: options.total ?? items.length,
      facets: options.facets ?? facets(),
    });
  };
}

function commands(overrides: Partial<Record<string, Handler>> = {}) {
  backend({
    get_catalog_status: () => catalogStatus(),
    search_catalog_machines: machineSearch(),
    get_launch_config: () => launchConfigView(),
    get_launch_sessions: () => [],
    get_software_launch_info: () => null,
    get_media_sources: () => mediaSourcesView({ snap: ["C:/Artwork ✓/snap"] }),
    ...overrides,
  });
}

function machineRequests(): MachineSearchRequest[] {
  return invokeMock.mock.calls
    .filter(([name]) => name === "search_catalog_machines")
    .map(([, args]) => machineRequest(args as Args));
}

/** Result page requests (related lists send no direction, facet requests no page). */
const resultRequests = () => machineRequests().filter((request) => request.direction);
const facetRequests = () => machineRequests().filter((request) => request.facets);
/** Every search made by the browser itself. */
const browserRequests = () => [...resultRequests(), ...facetRequests()];
const lastRequest = () => resultRequests().at(-1);
const lastFacetRequest = () => facetRequests().at(-1);
const isResultRequest = (args: Args) => machineRequest(args).direction !== undefined;

function withLauncher(children: ReactNode) {
  return (
    <MediaProvider>
      <LauncherProvider showCommandPreview={false} onShowCommandPreviewChange={() => undefined}>
        {children}
      </LauncherProvider>
    </MediaProvider>
  );
}

const PREFERENCES: BrowserPreferences = DEFAULT_BROWSER_PREFERENCES;

function renderBrowser(props: Partial<ComponentProps<typeof CatalogBrowser>> = {}) {
  const user = userEvent.setup();
  const onPreferencesChange = vi.fn();
  const onOpenMame = vi.fn();
  const base: ComponentProps<typeof CatalogBrowser> = {
    refreshKey: 0,
    preferences: PREFERENCES,
    onPreferencesChange,
    onOpenMame,
    debounceMs: 20,
    ...props,
  };
  const view = render(withLauncher(<CatalogBrowser {...base} />));
  const rerender = (next: Partial<ComponentProps<typeof CatalogBrowser>>) => {
    view.rerender(withLauncher(<CatalogBrowser {...base} {...next} />));
  };
  return { user, onPreferencesChange, onOpenMame, rerender };
}

const results = () => within(screen.getByRole("list", { name: "Results" }));

async function openCard(text: string) {
  const user = userEvent.setup();
  const list = await screen.findByRole("list", { name: "Results" });
  await user.click(await within(list).findByText(text));
}

describe("CatalogBrowser", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  describe("results", () => {
    it("searches the catalog and shows machine cards", async () => {
      commands();
      renderBrowser();

      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
      expect(results().getByText("Pac-Man")).toBeInTheDocument();
      expect(results().getByText("Clone of pacman")).toBeInTheDocument();
      expect(results().getByText("Driver: imperfect")).toBeInTheDocument();
      expect(resultRequests()).toHaveLength(1);
      expect(facetRequests()).toHaveLength(1);
      expect(lastRequest()).toMatchObject({ page: 0, pageSize: 50, filters: {} });
    });

    it("debounces the search text", async () => {
      commands();
      const { user } = renderBrowser({ debounceMs: 80 });
      await screen.findByText("1–3 of 3 machines");

      await user.type(screen.getByLabelText("Search text"), "pac");

      await waitFor(() => {
        expect(lastRequest()?.text).toBe("pac");
      });
      expect(resultRequests().map((request) => request.text)).toEqual([undefined, "pac"]);
      expect(facetRequests().map((request) => request.text)).toEqual([undefined, "pac"]);
    });

    it("searches immediately with the Search button", async () => {
      commands();
      const { user } = renderBrowser({ debounceMs: 60_000 });
      await screen.findByText("1–3 of 3 machines");

      await user.type(screen.getByLabelText("Search text"), "galaga");
      expect(resultRequests()).toHaveLength(1);
      await user.click(screen.getByRole("button", { name: "Search" }));

      await waitFor(() => {
        expect(lastRequest()?.text).toBe("galaga");
      });
    });

    it("ignores responses of superseded searches", async () => {
      const slow = deferred<unknown>();
      commands({
        search_catalog_machines: (args) =>
          machineRequest(args).text === "pac"
            ? response([MACHINES[0]], { facets: facets() })
            : slow.promise,
      });
      const { user } = renderBrowser({ debounceMs: 60_000 });

      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();

      slow.resolve(response([MACHINES[2]], { facets: facets() }));
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(results().getByText("Pac-Man")).toBeInTheDocument();
      expect(results().queryByText("Galaga")).not.toBeInTheDocument();
    });

    it("shows when nothing matches", async () => {
      commands({ search_catalog_machines: machineSearch([], { total: 0 }) });
      const { user } = renderBrowser();

      expect(await screen.findByText("No machines found")).toBeInTheDocument();
      expect(screen.getByText("No results match the search and filters.")).toBeInTheDocument();

      await user.click(screen.getByRole("checkbox", { name: /Namco/ }));
      const active = within(await screen.findByRole("group", { name: "Active filters" }));
      await user.click(active.getByRole("button", { name: "Clear all filters" }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({});
      });
    });

    it("shows search errors and retries", async () => {
      let attempts = 0;
      commands({
        search_catalog_machines: (args) => {
          if (isResultRequest(args)) attempts++;
          if (isResultRequest(args) && attempts === 1)
            throw Object.assign(new Error("internal"), {
              kind: "internal",
              message: "disk I/O error",
            });
          return machineSearch()(args);
        },
      });
      const { user } = renderBrowser();

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "The search failed: Unexpected error: disk I/O error",
      );
      await user.click(screen.getByRole("button", { name: "Retry" }));
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
    });
  });

  describe("filters", () => {
    it("combines facet values and flags, and removes them", async () => {
      commands();
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");

      const filters = within(screen.getByRole("complementary", { name: "Filters" }));
      expect(filters.getByRole("checkbox", { name: /Namco/ })).toHaveAccessibleName(/^Namcos*2$/);
      await user.click(filters.getByRole("checkbox", { name: /Namco/ }));
      await user.click(filters.getByRole("checkbox", { name: /Midway/ }));
      await user.selectOptions(filters.getByLabelText("Parent / clone"), "Parents only");

      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({
          manufacturers: ["Namco", "Midway"],
          clones: "parents",
        });
      });
      const active = within(screen.getByRole("group", { name: "Active filters" }));
      expect(active.getByText("Manufacturer: Namco")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^Filters/ })).toHaveTextContent("Filters3");

      await user.click(active.getByRole("button", { name: "Remove filter Manufacturer: Namco" }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ manufacturers: ["Midway"], clones: "parents" });
      });

      await user.click(filters.getByRole("button", { name: "Clear Manufacturer filter" }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ clones: "parents" });
      });

      await user.click(screen.getByRole("button", { name: "Clear all filters" }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({});
      });
      expect(screen.queryByRole("group", { name: "Active filters" })).not.toBeInTheDocument();
    });

    it("keeps selected values that are not in the returned facet values", async () => {
      commands();
      const { user } = renderBrowser();
      const filters = within(await screen.findByRole("complementary", { name: "Filters" }));
      await user.click(await filters.findByRole("checkbox", { name: /Namco/ }));

      commands({ search_catalog_machines: machineSearch(MACHINES, { facets: facets([]) }) });
      await user.click(filters.getByRole("checkbox", { name: /1980/ }));

      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ manufacturers: ["Namco"], years: ["1980"] });
      });
      expect(await filters.findByRole("checkbox", { name: /^Namcos*—$/ })).toBeChecked();
    });

    it("searches facet values and shows more of them", async () => {
      const many = Array.from({ length: 12 }, (_, index) => ({
        value: `Maker ${String(index)}`,
        count: 20 - index,
      }));
      commands({
        search_catalog_machines: machineSearch(MACHINES, { facets: facets(many, true) }),
      });
      const { user } = renderBrowser();
      const filters = within(await screen.findByRole("complementary", { name: "Filters" }));

      await user.type(await filters.findByLabelText("Search Manufacturer values"), "cap");
      await waitFor(() => {
        expect(lastFacetRequest()?.facets?.[0]).toEqual({
          facet: "manufacturer",
          search: "cap",
          limit: 50,
        });
      });

      await user.click(filters.getByRole("button", { name: "Show more" }));
      await waitFor(() => {
        expect(lastFacetRequest()?.facets?.[0]).toMatchObject({ limit: 100 });
      });
    });

    it("hides and shows the filter panel without changing the search", async () => {
      commands();
      const { user, onPreferencesChange, rerender } = renderBrowser();
      const filters = within(await screen.findByRole("complementary", { name: "Filters" }));
      await user.click(await filters.findByRole("checkbox", { name: /Namco/ }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ manufacturers: ["Namco"] });
      });
      const searches = browserRequests().length;

      const toggle = screen.getByRole("button", { name: /^Filters/ });
      expect(toggle).toHaveAttribute("aria-pressed", "true");
      await user.click(toggle);
      expect(onPreferencesChange).toHaveBeenCalledWith({ filtersVisible: false });

      rerender({ preferences: { ...PREFERENCES, filtersVisible: false } });
      expect(screen.queryByRole("complementary", { name: "Filters" })).not.toBeInTheDocument();
      expect(screen.getByText("Manufacturer: Namco")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^Filters/ })).toHaveAttribute(
        "aria-pressed",
        "false",
      );
      expect(document.querySelector(".browser__body--filters")).toBeNull();
      expect(results().getByText("Pac-Man")).toBeInTheDocument();

      rerender({ preferences: PREFERENCES });
      expect(await screen.findByRole("checkbox", { name: /Namco/ })).toBeChecked();
      expect(browserRequests()).toHaveLength(searches);
    });
  });

  describe("sorting and pagination", () => {
    it("sorts results", async () => {
      commands();
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");

      const sort = screen.getByLabelText("Sort by");
      expect(within(sort).getByRole("option", { name: "Best match" })).toBeDisabled();
      await user.selectOptions(sort, "Year");
      await waitFor(() => {
        expect(lastRequest()?.sort).toBe("year");
      });
      await user.click(screen.getByRole("button", { name: "Sort direction: ascending" }));
      await waitFor(() => {
        expect(lastRequest()).toMatchObject({ sort: "year", direction: "descending" });
      });
    });

    it("moves between pages and changes the page size", async () => {
      commands({ search_catalog_machines: machineSearch(MACHINES, { total: 120 }) });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 120 machines");
      const facetSearches = facetRequests().length;

      await user.click(screen.getByRole("button", { name: "Page 2" }));
      await waitFor(() => {
        expect(lastRequest()?.page).toBe(1);
      });
      expect(await screen.findByRole("button", { name: "Page 2" })).toHaveAttribute(
        "aria-current",
        "page",
      );

      await user.type(screen.getByLabelText("Go to"), "3");
      await user.click(screen.getByRole("button", { name: "Go" }));
      await waitFor(() => {
        expect(lastRequest()?.page).toBe(2);
      });

      await user.selectOptions(screen.getByLabelText("Per page"), "100");
      await waitFor(() => {
        expect(lastRequest()).toMatchObject({ page: 0, pageSize: 100 });
      });
      // Facets do not depend on the page: they are not recounted.
      expect(facetRequests()).toHaveLength(facetSearches);
    });
  });

  describe("display", () => {
    const withView = (change: Partial<BrowserPreferences> = {}): BrowserPreferences => ({
      ...PREFERENCES,
      viewOptionsVisible: true,
      ...change,
    });
    const mediaGrid = (options: Record<string, number | boolean | string> = {}) =>
      withView({
        renderer: "mediaGrid",
        rendererOptions: {
          ...PREFERENCES.rendererOptions,
          mediaGrid: { ...PREFERENCES.rendererOptions.mediaGrid, ...options },
        },
      });

    it("resizes classic cards from the view options without searching again", async () => {
      commands();
      const { onPreferencesChange, rerender } = renderBrowser({ preferences: withView() });
      await screen.findByText("1–3 of 3 machines");
      const searches = browserRequests().length;
      const panel = within(screen.getByRole("complementary", { name: "View options" }));
      expect(panel.getByRole("radio", { name: "Classic Cards" })).toHaveAttribute(
        "aria-checked",
        "true",
      );
      expect(panel.queryByLabelText(/Tile size/)).not.toBeInTheDocument();

      fireEvent.change(panel.getByLabelText(/Card size/), { target: { value: "300" } });
      expect(onPreferencesChange).toHaveBeenCalledWith({
        rendererOptions: { ...PREFERENCES.rendererOptions, classicCards: { cardSize: 300 } },
      });

      rerender({
        preferences: withView({
          rendererOptions: { ...PREFERENCES.rendererOptions, classicCards: { cardSize: 300 } },
        }),
      });
      expect(screen.getByRole("list", { name: "Results" })).toHaveStyle("--card-size: 300px");
      expect(browserRequests()).toHaveLength(searches);
    });

    it("switches renderer without changing the search", async () => {
      commands();
      const { user, onPreferencesChange, rerender } = renderBrowser({
        preferences: withView(),
        debounceMs: 60_000,
      });
      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      await user.click(await screen.findByRole("checkbox", { name: /Namco/ }));
      await waitFor(() => {
        expect(lastRequest()).toMatchObject({ text: "pac", filters: { manufacturers: ["Namco"] } });
      });
      await screen.findByText("1–3 of 3 machines");
      const searches = browserRequests().length;

      await user.click(screen.getByRole("radio", { name: "Media Grid" }));
      expect(onPreferencesChange).toHaveBeenCalledWith({ renderer: "mediaGrid" });
      rerender({ preferences: mediaGrid() });

      const tiles = within(screen.getByRole("list", { name: "Results" }));
      expect(tiles.getByRole("button", { name: "Pac-Man (pacman)" })).toBeInTheDocument();
      expect(screen.getByLabelText("Search text")).toHaveValue("pac");
      expect(screen.getByText("Manufacturer: Namco")).toBeInTheDocument();
      const panel = within(screen.getByRole("complementary", { name: "View options" }));
      expect(panel.getByRole("radio", { name: "Media Grid" })).toHaveAttribute(
        "aria-checked",
        "true",
      );
      for (const label of [
        /Tile size/,
        "Artwork",
        "Image fit",
        "Show title",
        "Show technical name",
        "Show status badges",
      ]) {
        expect(panel.getByLabelText(label)).toBeInTheDocument();
      }
      expect(panel.queryByLabelText(/Card size/)).not.toBeInTheDocument();
      expect(browserRequests()).toHaveLength(searches);
    });

    it("shows and hides both panels independently", async () => {
      commands();
      const { user, onPreferencesChange, rerender } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");
      const body = () => document.querySelector(".browser__body");
      const filters = () => screen.queryByRole("complementary", { name: "Filters" });
      const viewOptions = () => screen.queryByRole("complementary", { name: "View options" });

      // Default: filters only.
      expect(filters()).toBeInTheDocument();
      expect(viewOptions()).not.toBeInTheDocument();
      expect(body()).toHaveClass("browser__body--filters");
      expect(body()).not.toHaveClass("browser__body--view");

      await user.click(screen.getByRole("button", { name: "View" }));
      expect(onPreferencesChange).toHaveBeenCalledWith({ viewOptionsVisible: true });

      for (const [filtersVisible, viewOptionsVisible] of [
        [true, true],
        [false, true],
        [false, false],
        [true, false],
      ] as const) {
        rerender({ preferences: { ...PREFERENCES, filtersVisible, viewOptionsVisible } });
        expect(Boolean(filters())).toBe(filtersVisible);
        expect(Boolean(viewOptions())).toBe(viewOptionsVisible);
        expect(body()?.classList.contains("browser__body--filters")).toBe(filtersVisible);
        expect(body()?.classList.contains("browser__body--view")).toBe(viewOptionsVisible);
        // Both toggles stay available, and the results are unchanged.
        expect(screen.getByRole("button", { name: /^Filters/ })).toHaveAttribute(
          "aria-pressed",
          String(filtersVisible),
        );
        expect(screen.getByRole("button", { name: "View" })).toHaveAttribute(
          "aria-pressed",
          String(viewOptionsVisible),
        );
        expect(results().getByText("Pac-Man")).toBeInTheDocument();
      }
    });

    it("shows artwork with lazy loading, by category, with placeholders", async () => {
      commands();
      const { rerender } = renderBrowser({ preferences: mediaGrid() });
      await screen.findByText("1–3 of 3 machines");
      const button = (name: string) =>
        screen.getByRole("button", { name: new RegExp(`\\(${name}\\)$`) });
      const tile = (name: string) => within(button(name));
      // Images are decorative (alt=""): the tile button carries the name.
      const imageOf = (name: string) => button(name).querySelector("img");

      const image = imageOf("pacman");
      if (!image) throw new Error("no image");
      expect(image).toHaveAttribute("src", "ymdb-media://localhost/snap%2Fmachine%2Fpacman?v=0");
      expect(image).toHaveAttribute("loading", "lazy");
      fireEvent.load(image);
      expect(image).toHaveAttribute("data-state", "loaded");

      // A missing image shows a placeholder; the title stays visible.
      const galaga = imageOf("galaga");
      if (!galaga) throw new Error("no image");
      fireEvent.error(galaga);
      expect(tile("galaga").getByTestId("media-placeholder")).toHaveTextContent("No snapshots");
      expect(tile("galaga").getByText("Galaga")).toBeInTheDocument();
      expect(tile("pacman").queryByTestId("media-placeholder")).not.toBeInTheDocument();

      // Titles have no configured directory: no request, a hint and placeholders.
      rerender({ preferences: mediaGrid({ category: "titles" }) });
      expect(await screen.findByRole("note")).toHaveTextContent(
        "No directory is configured for title screens.",
      );
      expect(document.querySelectorAll(".media-grid img")).toHaveLength(0);
      expect(screen.getAllByTestId("media-placeholder")).toHaveLength(3);
    });

    it("applies the media grid options", async () => {
      commands();
      const { rerender } = renderBrowser({ preferences: mediaGrid({ tileSize: 320 }) });
      await screen.findByText("1–3 of 3 machines");
      const grid = screen.getByRole("list", { name: "Results" });
      expect(grid).toHaveStyle("--tile-size: 320px");
      expect(grid).toHaveAttribute("data-fit", "contain");
      expect(within(grid).getByText("Pac-Man")).toBeInTheDocument();
      expect(within(grid).queryByText("pacman")).not.toBeInTheDocument();
      expect(within(grid).getAllByText(/^Driver:/)).toHaveLength(3);

      rerender({
        preferences: mediaGrid({
          tileSize: 320,
          fit: "cover",
          showTitle: false,
          showName: true,
          showBadges: false,
        }),
      });
      expect(grid).toHaveAttribute("data-fit", "cover");
      expect(within(grid).queryByText("Pac-Man")).not.toBeInTheDocument();
      expect(within(grid).getByText("pacman")).toBeInTheDocument();
      expect(within(grid).queryByText(/^Driver:/)).not.toBeInTheDocument();
      // The full title stays available as accessible name and tooltip.
      const tile = within(grid).getByRole("button", { name: "Pac-Man (pacman)" });
      expect(tile).toHaveAttribute("title", expect.stringContaining("Pac-Man"));
    });

    it("opens the detail from a media tile with the keyboard", async () => {
      commands({ get_machine_detail: () => machineDetail("galaga", { description: "Galaga" }) });
      const { user } = renderBrowser({ preferences: mediaGrid() });
      const tile = await screen.findByRole("button", { name: "Galaga (galaga)" });
      tile.focus();
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("heading", { name: "Galaga" })).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("get_machine_detail", { name: "galaga" });
    });

    it("requests software artwork by list and name", async () => {
      commands({
        search_catalog_software: () =>
          response([software("nes", "smb", { description: "Super Mario Bros." })]),
      });
      const { user } = renderBrowser({ preferences: mediaGrid() });
      await screen.findByText("1–3 of 3 machines");
      await user.selectOptions(screen.getByLabelText("Catalog type"), "Software");
      const tile = await screen.findByRole("button", { name: "Super Mario Bros. (nes/smb)" });
      expect(tile.querySelector("img")).toHaveAttribute(
        "src",
        "ymdb-media://localhost/snap%2Fsoftware%2Fnes%2Fsmb?v=0",
      );
      expect(within(tile).getByText("Supported")).toBeInTheDocument();
    });
  });

  describe("details", () => {
    it("navigates parent and clone relations and restores the results", async () => {
      commands({
        search_catalog_machines: (args) => {
          const request = machineRequest(args);
          if (request.filters?.cloneOf === "pacman") {
            return response([MACHINES[1]], { pageSize: request.pageSize ?? 20 });
          }
          return machineSearch(MACHINES, { total: 120 })(args);
        },
        get_machine_detail: (args) =>
          args.name === "mspacman"
            ? machineDetail("mspacman", {
                description: "Ms. Pac-Man",
                parent: { name: "pacman", description: "Pac-Man" },
              })
            : machineDetail("pacman", { description: "Pac-Man", cloneCount: 1 }),
      });
      const { user } = renderBrowser({ debounceMs: 60_000 });
      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      await user.click(await screen.findByRole("checkbox", { name: /Namco/ }));
      await user.click(await screen.findByRole("button", { name: "Page 2" }));
      await waitFor(() => {
        expect(lastRequest()).toMatchObject({ text: "pac", page: 1 });
      });
      const searches = browserRequests().length;

      await openCard("Ms. Pac-Man");
      expect(await screen.findByRole("heading", { name: "Ms. Pac-Man" })).toBeInTheDocument();
      expect(screen.getByLabelText("Search text")).not.toBeVisible();

      await user.click(screen.getByRole("button", { name: "Pac-Man (pacman)" }));
      expect(await screen.findByRole("heading", { name: "Pac-Man" })).toBeInTheDocument();
      const clones = within(await screen.findByRole("list", { name: "Clones" }));
      expect(clones.getByRole("button", { name: "Ms. Pac-Man" })).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Back" }));
      expect(await screen.findByRole("heading", { name: "Ms. Pac-Man" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Back to results" }));

      expect(await screen.findByLabelText("Search text")).toHaveValue("pac");
      expect(screen.getByText("Manufacturer: Namco")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Page 2" })).toHaveAttribute(
        "aria-current",
        "page",
      );
      expect(browserRequests()).toHaveLength(searches);
    });

    it("steps through the current page with previous and next", async () => {
      commands({
        get_machine_detail: (args) =>
          machineDetail(String(args.name), {
            description: MACHINES.find((entry) => entry.name === args.name)?.description,
          }),
      });
      const { user } = renderBrowser();
      await openCard("Pac-Man");

      expect(await screen.findByRole("heading", { name: "Pac-Man" })).toBeInTheDocument();
      expect(screen.getByText("1 / 3")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Previous" })).toBeDisabled();

      await user.click(screen.getByRole("button", { name: "Next" }));
      expect(await screen.findByRole("heading", { name: "Ms. Pac-Man" })).toBeInTheDocument();
      expect(screen.getByText("2 / 3")).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("get_machine_detail", { name: "mspacman" });
    });

    it("shows the detail sections", async () => {
      commands({
        get_machine_detail: () =>
          machineDetail("pacman", {
            description: "Pac-Man",
            roms: [
              {
                name: "pacman.6e",
                bios: null,
                size: 4096,
                crc: "c1e6ab10",
                sha1: "e87e059c5be45753f7e9f33dff851f16d6751181",
                mergeName: null,
                region: "maincpu",
                offset: "0",
                status: "good",
                isOptional: false,
              },
            ],
            chips: [{ chipType: "cpu", name: "Zilog Z80", tag: "maincpu", clock: 3072000 }],
            devices: [{ device: { name: "z80", description: null }, instanceCount: 1 }],
            softwareLists: [
              {
                name: "pacman_cart",
                tag: null,
                status: "original",
                filter: null,
                description: null,
                available: false,
              },
            ],
          }),
        get_software_list_detail: () =>
          softwareListDetail("pacman_cart", { available: false, description: null }),
      });
      const { user } = renderBrowser();
      await openCard("Pac-Man");
      await screen.findByRole("heading", { name: "Pac-Man" });

      await user.click(screen.getByRole("tab", { name: /ROMs/ }));
      const roms = within(screen.getByRole("table", { name: "ROMs" }));
      expect(roms.getByText("pacman.6e")).toBeInTheDocument();
      expect(roms.getByText("4 KiB (4,096 B)")).toBeInTheDocument();

      await user.click(screen.getByRole("tab", { name: /Chipset/ }));
      expect(screen.getByText("3.072 MHz")).toBeInTheDocument();

      await user.click(screen.getByRole("tab", { name: /Devices/ }));
      expect(screen.getByText("(not in the catalog)")).toBeInTheDocument();

      await user.click(screen.getByRole("tab", { name: /Software lists/ }));
      expect(screen.getByText(/not imported/)).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "pacman_cart" }));
      expect(
        await screen.findByText(
          "This list is referenced by machines but is not provided by this MAME version.",
        ),
      ).toBeInTheDocument();
    });

    it("explains a record that is no longer in the catalog", async () => {
      commands({ get_machine_detail: () => null });
      renderBrowser();
      await openCard("Galaga");

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "This machine is not in the catalog.",
      );
      expect(screen.getByRole("button", { name: "Back to results" })).toBeInTheDocument();
    });

    it("reloads results and details when the catalog is replaced", async () => {
      let replaced = false;
      commands({
        get_machine_detail: () =>
          replaced ? null : machineDetail("galaga", { description: "Galaga" }),
      });
      const { rerender } = renderBrowser();
      await openCard("Galaga");
      expect(await screen.findByRole("heading", { name: "Galaga" })).toBeInTheDocument();
      const searches = browserRequests().length;

      replaced = true;
      rerender({ refreshKey: 1 });

      expect(await screen.findByText(/This machine is not in the catalog/)).toBeInTheDocument();
      await waitFor(() => {
        expect(browserRequests().length).toBeGreaterThan(searches);
      });
    });
  });

  describe("software", () => {
    it("browses software and opens software and list details", async () => {
      commands({
        search_catalog_software: (args) =>
          response([software("nes", "smb", { description: "Super Mario Bros." })], {
            pageSize: softwareRequest(args).pageSize ?? 50,
          }),
        get_software_detail: () =>
          softwareDetail("nes", "smb", {
            description: "Super Mario Bros.",
            parts: [
              {
                name: "cart",
                interface: "nes_cart",
                features: [{ name: "pcb", value: "NES-NROM-256" }],
                dataAreas: [
                  {
                    name: "prg",
                    size: 32768,
                    databits: 8,
                    endian: "little",
                    roms: [
                      {
                        name: "smb.prg",
                        size: 32768,
                        length: null,
                        crc: "967a605f",
                        sha1: null,
                        offset: "0",
                        value: null,
                        status: "good",
                        loadFlag: null,
                      },
                    ],
                  },
                ],
                diskAreas: [],
              },
            ],
          }),
        get_software_list_detail: () => softwareListDetail("nes", { description: "NES carts" }),
      });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");

      await user.selectOptions(screen.getByLabelText("Catalog type"), "Software");
      expect(await screen.findByText("1–1 of 1 software")).toBeInTheDocument();
      expect(results().getByText("nes/smb")).toBeInTheDocument();
      expect(results().getByText("Supported")).toBeInTheDocument();
      const filters = within(screen.getByRole("complementary", { name: "Filters" }));
      expect(filters.getByLabelText("Declares disks")).toBeInTheDocument();

      await openCard("Super Mario Bros.");
      expect(await screen.findByRole("heading", { name: "Super Mario Bros." })).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("get_software_detail", { list: "nes", name: "smb" });
      await user.click(screen.getByRole("tab", { name: /Parts/ }));
      expect(screen.getByText("smb.prg")).toBeInTheDocument();

      await user.click(screen.getByRole("tab", { name: "Overview" }));
      await user.click(screen.getByRole("button", { name: "nes list (nes)" }));
      expect(await screen.findByRole("heading", { name: "NES carts" })).toBeInTheDocument();
    });

    it("explains that software lists are not imported", async () => {
      commands({
        get_catalog_status: () => {
          const status = catalogStatus();
          return { ...status, catalog: status.catalog && { ...status.catalog, software: null } };
        },
      });
      const { user, onOpenMame } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");

      await user.selectOptions(screen.getByLabelText("Catalog type"), "Software");
      expect(
        await screen.findByText(/The software lists have not been imported yet/),
      ).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Open MAME settings" }));
      expect(onOpenMame).toHaveBeenCalled();
      expect(invokeMock).not.toHaveBeenCalledWith("search_catalog_software", expect.anything());
    });
  });

  describe("catalog states", () => {
    it("explains that no catalog is imported", async () => {
      commands({ get_catalog_status: () => emptyCatalogStatus });
      const { user, onOpenMame } = renderBrowser();

      expect(await screen.findByText("No catalog yet")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Open MAME settings" }));
      expect(onOpenMame).toHaveBeenCalled();
      expect(browserRequests()).toHaveLength(0);
    });

    it("warns about an outdated catalog", async () => {
      commands({
        get_catalog_status: (): CatalogStatus =>
          catalogStatus({ freshness: "differentExecutable" }),
      });
      renderBrowser();

      expect(
        await screen.findByText(/imported from a different MAME executable/),
      ).toBeInTheDocument();
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
    });

    it("retries when the catalog status cannot be read", async () => {
      let attempts = 0;
      commands({
        get_catalog_status: () => {
          attempts++;
          if (attempts === 1) throw new Error("boom");
          return catalogStatus();
        },
      });
      const { user } = renderBrowser();

      expect(await screen.findByRole("alert")).toHaveTextContent("could not be read");
      await user.click(screen.getByRole("button", { name: "Retry" }));
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
    });
  });
});
