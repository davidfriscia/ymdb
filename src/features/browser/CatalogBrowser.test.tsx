import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ComponentProps, type ReactNode } from "react";
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
import { favoritesList } from "../../test/listFixtures";
import { CatalogBrowser } from "./CatalogBrowser";
import type { ListEntry } from "../lists/listsApi";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { launchConfigView } from "../../test/launcherFixtures";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import type { InstallationSummary } from "../installations/installationsApi";
import { useInstallations } from "../installations/installationsContext";
import { FavoritesProvider } from "../lists/FavoritesProvider";

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

function commands(
  overrides: Partial<Record<string, Handler>> = {},
  installations: InstallationSummary[] = [
    installation(NEW_ID, "0.280"),
    installation(OLD_ID, "0.252"),
  ],
) {
  let active: string | null = installations[0]?.id ?? null;
  backend({
    list_installations: () => installationsView(installations, active),
    set_active_installation: (args) => {
      active = args.installationId as string;
      return installationsView(installations, active);
    },
    get_catalog_status: (args) => catalogStatus({ installationId: args.installationId as string }),
    search_catalog_machines: machineSearch(),
    get_launch_config: () => launchConfigView(),
    get_launch_sessions: () => [],
    get_software_launch_info: () => null,
    get_media_sources: () => mediaSourcesView({ snap: ["C:/Artwork ✓/snap"] }),
    list_media: () => [],
    get_list: () => favoritesList([]),
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

function ActiveLauncher({ children }: { children: ReactNode }) {
  const { activeId } = useInstallations();
  return (
    <LauncherProvider
      installationId={activeId}
      showCommandPreview={false}
      onShowCommandPreviewChange={() => undefined}
    >
      {children}
    </LauncherProvider>
  );
}

/** Switches the active installation from outside the browser, as the MAME page does. */
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

function withLauncher(children: ReactNode) {
  return (
    <MediaProvider>
      <InstallationsProvider>
        <SwitchToOld />
        <FavoritesProvider>
          <ActiveLauncher>{children}</ActiveLauncher>
        </FavoritesProvider>
      </InstallationsProvider>
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
/** The only scrolling element of the results column. */
const resultsScroll = () => screen.getByTestId("results-scroll");

function required<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error("missing element");
  return value;
}

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

    it("keeps one pagination footer below the scrolling results, for every renderer", async () => {
      commands({ search_catalog_machines: machineSearch(MACHINES, { total: 120 }) });
      const { rerender } = renderBrowser();
      await screen.findByText("1–3 of 120 machines");

      for (const renderer of ["classicCards", "mediaGrid"] as const) {
        rerender({ preferences: { ...PREFERENCES, renderer } });
        const navigations = screen.getAllByRole("navigation", { name: "Pagination" });
        expect(navigations).toHaveLength(1);
        const [pagination] = navigations;
        // The footer is outside the scrolling area, next to it in the results column.
        expect(resultsScroll()).not.toContainElement(required(pagination));
        expect(pagination?.closest(".browser__footer")?.parentElement).toBe(
          resultsScroll().parentElement,
        );
        expect(within(required(pagination)).getByText("Page 1 of 3")).toBeInTheDocument();
        expect(resultsScroll()).toContainElement(screen.getByRole("list", { name: "Results" }));
      }
    });

    it("shows the footer for a single page and hides it without results", async () => {
      let total = 3;
      commands({
        search_catalog_machines: (args) =>
          machineSearch(total === 0 ? [] : MACHINES, { total })(args),
      });
      const { user } = renderBrowser({ debounceMs: 60_000 });
      await screen.findByText("1–3 of 3 machines");
      const pagination = within(screen.getByRole("navigation", { name: "Pagination" }));
      expect(pagination.getByText("Page 1 of 1")).toBeInTheDocument();
      expect(pagination.getByRole("button", { name: "Previous" })).toBeDisabled();
      expect(pagination.getByRole("button", { name: "Next" })).toBeDisabled();
      expect(pagination.getByLabelText("Per page")).toBeEnabled();

      total = 0;
      await user.type(screen.getByLabelText("Search text"), "zzz{Enter}");
      expect(await screen.findByText("No machines found")).toBeInTheDocument();
      expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
    });

    it("returns the results to the top on a new page, leaving filters and view options", async () => {
      commands({ search_catalog_machines: machineSearch(MACHINES, { total: 400 }) });
      const { user } = renderBrowser({
        preferences: { ...PREFERENCES, viewOptionsVisible: true },
      });
      await screen.findByText("1–3 of 400 machines");
      await user.click(await screen.findByRole("checkbox", { name: /Namco/ }));
      await waitFor(() => {
        expect(lastRequest()?.filters?.manufacturers).toEqual(["Namco"]);
      });
      const filters = screen.getByRole("complementary", { name: "Filters" });
      const viewOptions = screen.getByRole("complementary", { name: "View options" });

      const steps: [string, () => Promise<void>, Partial<MachineSearchRequest>][] = [
        [
          "page number",
          () => user.click(screen.getByRole("button", { name: "Page 2" })),
          { page: 1 },
        ],
        ["next", () => user.click(screen.getByRole("button", { name: "Next" })), { page: 2 }],
        [
          "go to",
          async () => {
            await user.type(screen.getByLabelText("Go to"), "7");
            await user.click(screen.getByRole("button", { name: "Go" }));
          },
          { page: 6 },
        ],
        [
          "page size",
          () => user.selectOptions(screen.getByLabelText("Per page"), "200"),
          { page: 0, pageSize: 200 },
        ],
      ];
      for (const [step, act, expected] of steps) {
        resultsScroll().scrollTop = 900;
        filters.scrollTop = 250;
        await act();
        await waitFor(() => {
          expect(lastRequest(), step).toMatchObject(expected);
        });
        expect(resultsScroll().scrollTop, step).toBe(0);
        expect(filters.scrollTop, step).toBe(250);
        expect(screen.getByRole("checkbox", { name: /Namco/ })).toBeChecked();
        expect(screen.getByRole("complementary", { name: "View options" })).toBe(viewOptions);
      }
      expect(lastRequest()?.filters?.manufacturers).toEqual(["Namco"]);
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
      expect(invokeMock).toHaveBeenCalledWith("get_machine_detail", {
        installationId: NEW_ID,
        name: "galaga",
      });
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
      expect(invokeMock).toHaveBeenCalledWith("get_machine_detail", {
        installationId: NEW_ID,
        name: "mspacman",
      });
    });

    it("returns to the same page, view and scroll position after browsing details", async () => {
      commands({
        search_catalog_machines: machineSearch(MACHINES, { total: 400 }),
        get_machine_detail: (args) =>
          machineDetail(String(args.name), {
            description: MACHINES.find((entry) => entry.name === args.name)?.description,
          }),
      });
      const preferences: BrowserPreferences = {
        ...PREFERENCES,
        renderer: "mediaGrid",
        rendererOptions: {
          ...PREFERENCES.rendererOptions,
          mediaGrid: { ...PREFERENCES.rendererOptions.mediaGrid, tileSize: 320 },
        },
      };
      const { user } = renderBrowser({ preferences, debounceMs: 60_000 });
      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      await user.selectOptions(screen.getByLabelText("Sort by"), "Year");
      await user.click(await screen.findByRole("checkbox", { name: /Namco/ }));
      await user.selectOptions(screen.getByLabelText("Per page"), "100");
      await user.click(await screen.findByRole("button", { name: "Page 3" }));
      await waitFor(() => {
        expect(lastRequest()).toMatchObject({ text: "pac", sort: "year", page: 2, pageSize: 100 });
      });
      const searches = browserRequests().length;
      resultsScroll().scrollTop = 640;
      const filters = screen.getByRole("complementary", { name: "Filters" });
      filters.scrollTop = 120;

      await user.click(screen.getByRole("button", { name: "Ms. Pac-Man (mspacman)" }));
      expect(await screen.findByRole("heading", { name: "Ms. Pac-Man" })).toBeInTheDocument();
      // Browsers reset the position of hidden elements: the saved one is restored.
      resultsScroll().scrollTop = 0;
      await user.click(screen.getByRole("button", { name: "Next" }));
      expect(await screen.findByRole("heading", { name: "Galaga" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Back to results" }));

      expect(resultsScroll().scrollTop).toBe(640);
      expect(filters.scrollTop).toBe(120);
      expect(screen.getByLabelText("Search text")).toHaveValue("pac");
      expect(screen.getByLabelText("Sort by")).toHaveValue("year");
      expect(screen.getByRole("checkbox", { name: /Namco/ })).toBeChecked();
      expect(screen.getByLabelText("Per page")).toHaveValue("100");
      expect(screen.getByRole("button", { name: "Page 3" })).toHaveAttribute(
        "aria-current",
        "page",
      );
      expect(screen.getByRole("list", { name: "Results" })).toHaveStyle("--tile-size: 320px");
      // The results were kept: nothing was searched again.
      expect(browserRequests()).toHaveLength(searches);
    });

    it("lays out the header, artwork, launch bar, Advanced and technical details", async () => {
      commands({
        get_media_sources: () =>
          mediaSourcesView({ snap: ["C:/Artwork/snap"], flyers: ["C:/Artwork/flyers"] }),
        list_media: (args) =>
          (args.target as { name: string }).name === "pacman"
            ? [
                { category: "snap", count: 2 },
                { category: "flyers", count: 1 },
              ]
            : [],
        get_machine_detail: (args) =>
          machineDetail(String(args.name), {
            description: MACHINES.find((entry) => entry.name === args.name)?.description,
          }),
      });
      const { user } = renderBrowser();
      await openCard("Pac-Man");
      const title = await screen.findByRole("heading", { name: "Pac-Man" });

      // Header: back, then the position in the results; the title is below the media.
      const start = screen.getByRole("navigation", { name: "Detail navigation" });
      const end = screen.getByRole("navigation", { name: "Result navigation" });
      const back = within(start).getByRole("button", { name: "Back to results" });
      const next = within(end).getByRole("button", { name: "Next" });
      expect(back.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(within(end).getByText("1 / 3")).toBeInTheDocument();
      expect(next.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

      const gallery = await screen.findByRole("region", { name: "Media" });
      await within(gallery).findByRole("list", { name: "Image categories" });
      const launch = await screen.findByRole("region", { name: "Launch" });
      const advanced = screen.getByRole("region", { name: "Advanced launch options" });
      const sections = screen.getByRole("tablist", { name: "Sections" });
      for (const [before, after] of [
        [gallery, launch],
        [launch, advanced],
        [advanced, sections],
      ] as const) {
        expect(
          before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
      // The bar shows the title, the short information and the actions, in order.
      expect(screen.getAllByRole("heading", { level: 2 })).toEqual([title]);
      expect(launch).toContainElement(title);
      expect(within(launch).getByText("pacman")).toBeInTheDocument();
      const controls = [
        within(launch).getByRole("button", { name: "Prepare Download" }),
        within(launch).getByRole("button", { name: "Download & Play" }),
        within(launch).getByRole("button", { name: "Play" }),
        within(launch).getByRole("combobox", { name: "Profile" }),
      ];
      controls.slice(1).forEach((control, index) => {
        expect(
          (controls[index]?.compareDocumentPosition(control) ?? 0) &
            Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      });
      // No launch yet: no status panel.
      expect(screen.queryByRole("region", { name: "Launch status" })).toBeNull();
      expect(within(advanced).getByRole("button", { name: /^Advanced/ })).toHaveAttribute(
        "aria-expanded",
        "false",
      );

      expect(
        within(gallery)
          .getAllByRole("button")
          .map((button) => button.getAttribute("aria-label")),
      ).toEqual(expect.arrayContaining(["Enlarge snapshots 1", "Snapshots (2)", "Flyers (1)"]));
      expect(gallery.querySelector(".media-gallery__image")).toHaveAttribute(
        "src",
        "ymdb-media://localhost/snap%2Fmachine%2Fpacman?v=0",
      );

      // Next title: its own images; without any, a discreet message and no categories.
      await user.click(next);
      await screen.findByRole("heading", { name: "Ms. Pac-Man" });
      expect(document.querySelector('img[src*="pacman%3F"], img[src*="2Fpacman"]')).toBeNull();
      expect(await screen.findByText(/No artwork was found/)).toBeInTheDocument();
      expect(screen.queryByRole("list", { name: "Image categories" })).toBeNull();
      expect(screen.getByText("2 / 3")).toBeInTheDocument();
      expect(
        within(screen.getByRole("region", { name: "Launch" })).getByRole("button", {
          name: "Play",
        }),
      ).toBeInTheDocument();
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
        list_media: () => [{ category: "snap", count: 1 }],
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
      expect(invokeMock).toHaveBeenCalledWith("get_software_detail", {
        installationId: NEW_ID,
        list: "nes",
        name: "smb",
      });
      // The artwork of a software is found by its list and name.
      expect(invokeMock).toHaveBeenCalledWith("list_media", {
        target: { kind: "software", list: "nes", name: "smb" },
      });
      const gallery = within(await screen.findByRole("region", { name: "Media" }));
      expect(
        await gallery.findByRole("img", { name: "Super Mario Bros.: Snapshots" }),
      ).toHaveAttribute("src", "ymdb-media://localhost/snap%2Fsoftware%2Fnes%2Fsmb?v=0");
      await user.click(screen.getByRole("tab", { name: /Parts/ }));
      expect(screen.getByText("smb.prg")).toBeInTheDocument();

      await user.click(screen.getByRole("tab", { name: "Overview" }));
      await user.click(screen.getByRole("button", { name: "nes list (nes)" }));
      expect(await screen.findByRole("heading", { name: "NES carts" })).toBeInTheDocument();
      // A software list has no artwork of its own.
      expect(screen.queryByRole("region", { name: "Media" })).toBeNull();
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

  describe("MAME versions", () => {
    const withViewOptions: BrowserPreferences = { ...PREFERENCES, viewOptionsVisible: true };
    const versionSelect = () =>
      within(screen.getByRole("complementary", { name: "View options" })).getByRole("combobox", {
        name: "MAME version",
      });
    /** The 0.252 catalog only has Pac-Man. */
    const byInstallation: Handler = (args) =>
      args.installationId === OLD_ID
        ? machineSearch(MACHINES.slice(0, 1), { facets: facets([{ value: "Namco", count: 1 }]) })(
            args,
          )
        : machineSearch()(args);
    const searchCalls = () =>
      invokeMock.mock.calls
        .filter(([name]) => name === "search_catalog_machines")
        .map(([, args]) => args as Args);

    it("switches the catalog from the view options, keeping the search and the view", async () => {
      commands({ search_catalog_machines: byInstallation });
      const { user, onPreferencesChange } = renderBrowser({
        preferences: withViewOptions,
        debounceMs: 60_000,
      });
      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      await waitFor(() => {
        expect(lastRequest()?.text).toBe("pac");
      });
      const select = versionSelect();
      expect(
        within(select)
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual(["MAME 0.280", "MAME 0.252"]);
      expect(select).toHaveValue(NEW_ID);

      await user.selectOptions(select, OLD_ID);

      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();
      expect(versionSelect()).toHaveValue(OLD_ID);
      expect(invokeMock).toHaveBeenCalledWith("set_active_installation", {
        installationId: OLD_ID,
      });
      expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: OLD_ID });
      // Results and facets come from the new catalog, with the same search.
      const latest = searchCalls().slice(-2);
      expect(latest.map((args) => args.installationId)).toEqual([OLD_ID, OLD_ID]);
      expect(lastRequest()?.text).toBe("pac");
      expect(screen.getByLabelText("Search text")).toHaveValue("pac");
      const filters = within(screen.getByRole("complementary", { name: "Filters" }));
      expect(filters.getByRole("checkbox", { name: /Namco/ })).toHaveAccessibleName(/^Namco\s*1$/);
      expect(filters.queryByRole("checkbox", { name: /Midway/ })).not.toBeInTheDocument();
      // The renderer and its options are interface preferences: unchanged.
      expect(onPreferencesChange).not.toHaveBeenCalled();
    });

    it("never shows results of the previously selected catalog", async () => {
      const slow = deferred<unknown>();
      let delayed = false;
      commands({
        search_catalog_machines: (args) => {
          if (args.installationId === NEW_ID && isResultRequest(args) && !delayed) {
            delayed = true;
            return slow.promise;
          }
          return byInstallation(args);
        },
      });
      const { user } = renderBrowser({ preferences: withViewOptions });
      await user.selectOptions(
        await screen.findByRole("combobox", { name: "MAME version" }),
        OLD_ID,
      );
      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();

      // The response for 0.280 arrives after the switch: it is ignored.
      slow.resolve(response(MACHINES, { facets: facets() }));
      await new Promise((resolve) => setTimeout(resolve, 30));
      expect(screen.getByText("1–1 of 1 machines")).toBeInTheDocument();
      expect(results().queryByText("Galaga")).not.toBeInTheDocument();
    });

    it("does not keep the previous catalog's results while the new one is searched", async () => {
      const slow = deferred<unknown>();
      commands({
        search_catalog_machines: (args) =>
          args.installationId === OLD_ID && isResultRequest(args)
            ? slow.promise
            : byInstallation(args),
      });
      const { user } = renderBrowser({ preferences: withViewOptions });
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();

      await user.selectOptions(versionSelect(), OLD_ID);
      expect(await screen.findByText("Searching…")).toBeInTheDocument();
      expect(screen.queryByText("Galaga")).not.toBeInTheDocument();

      slow.resolve(
        machineSearch(MACHINES.slice(0, 1))({ request: { page: 0, direction: "ascending" } }),
      );
      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();
    });

    it("closes the details of the previous catalog", async () => {
      commands({
        search_catalog_machines: byInstallation,
        get_machine_detail: (args) => machineDetail(args.name as string),
      });
      const { user } = renderBrowser();
      await openCard("Galaga");
      expect(
        await screen.findByRole("heading", { name: "galaga description" }),
      ).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Activate MAME 0.252" }));

      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "galaga description" })).not.toBeInTheDocument();
      expect(invokeMock).not.toHaveBeenCalledWith("get_machine_detail", {
        installationId: OLD_ID,
        name: "galaga",
      });
    });

    it("keeps the filters and says that missing values match nothing", async () => {
      commands({ search_catalog_machines: byInstallation });
      const { user } = renderBrowser({ preferences: withViewOptions });
      await screen.findByText("1–3 of 3 machines");
      await user.click(screen.getByRole("checkbox", { name: /Midway/ }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ manufacturers: ["Midway"] });
      });

      await user.selectOptions(versionSelect(), OLD_ID);

      expect(
        await screen.findByText(/The filters were kept after changing the MAME version/),
      ).toBeInTheDocument();
      await waitFor(() => {
        expect(searchCalls().at(-1)?.installationId).toBe(OLD_ID);
      });
      expect(lastRequest()).toMatchObject({ page: 0, filters: { manufacturers: ["Midway"] } });
      // A kept value missing from the new catalog stays visible, without a count.
      expect(screen.getByRole("checkbox", { name: /Midway/ })).toBeChecked();

      await user.click(screen.getByRole("button", { name: "Dismiss" }));
      expect(
        screen.queryByText(/The filters were kept after changing the MAME version/),
      ).not.toBeInTheDocument();
      expect(lastRequest()?.filters).toEqual({ manufacturers: ["Midway"] });
    });

    it("shows an installation without catalog and lets the user switch back", async () => {
      commands(
        {
          search_catalog_machines: byInstallation,
          get_catalog_status: (args) =>
            args.installationId === OLD_ID
              ? { ...emptyCatalogStatus, installationId: OLD_ID }
              : catalogStatus(),
        },
        [installation(NEW_ID, "0.280"), installation(OLD_ID, "0.252", { catalogImported: false })],
      );
      const { user } = renderBrowser({ preferences: withViewOptions });
      await screen.findByText("1–3 of 3 machines");
      expect(
        within(versionSelect()).getByRole("option", { name: "MAME 0.252 — no catalog" }),
      ).toBeInTheDocument();

      await user.selectOptions(versionSelect(), OLD_ID);

      expect(await screen.findByText("No catalog yet")).toBeInTheDocument();
      expect(searchCalls().some((args) => args.installationId === OLD_ID)).toBe(false);
      await user.selectOptions(screen.getByRole("combobox", { name: "MAME version" }), NEW_ID);
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
    });

    it("saves quick successive selections in order", async () => {
      const first = deferred<unknown>();
      const saved: string[] = [];
      commands({
        search_catalog_machines: byInstallation,
        set_active_installation: (args) => {
          saved.push(args.installationId as string);
          const view = installationsView(undefined, args.installationId as string);
          return saved.length === 1 ? first.promise.then(() => view) : view;
        },
      });
      const { user } = renderBrowser({ preferences: withViewOptions });
      await screen.findByText("1–3 of 3 machines");

      await user.selectOptions(versionSelect(), OLD_ID);
      await user.selectOptions(versionSelect(), NEW_ID);
      // The second selection waits for the first one to be saved.
      expect(saved).toEqual([OLD_ID]);
      expect(versionSelect()).toHaveValue(NEW_ID);
      first.resolve(undefined);
      await waitFor(() => {
        expect(saved).toEqual([OLD_ID, NEW_ID]);
      });
      expect(versionSelect()).toHaveValue(NEW_ID);
      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
    });

    it("explains that no installation is registered", async () => {
      commands({}, []);
      const { user, onOpenMame } = renderBrowser();

      expect(await screen.findByText("No MAME installation")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Open MAME settings" }));
      expect(onOpenMame).toHaveBeenCalled();
      expect(invokeMock).not.toHaveBeenCalledWith("get_catalog_status", expect.anything());
    });
  });

  describe("favorites", () => {
    const pacman: ListEntry = { kind: "machine", name: "pacman" };
    const bookmark = (title: string) =>
      results().getByRole("button", { name: `Favorite: ${title}` });
    const membershipCalls = () =>
      invokeMock.mock.calls
        .filter(([name]) => name === "set_list_membership")
        .map(([, args]) => args as Args);
    const saveMembership: Handler = (args) => args.member;

    /** The browser with preferences kept in state, as the application does. */
    function renderWithPreferences(initial: Partial<BrowserPreferences> = {}) {
      const user = userEvent.setup();
      function Stateful() {
        const [preferences, setPreferences] = useState<BrowserPreferences>({
          ...PREFERENCES,
          ...initial,
        });
        return (
          <CatalogBrowser
            refreshKey={0}
            preferences={preferences}
            onPreferencesChange={(change) => {
              setPreferences((current) => ({ ...current, ...change }));
            }}
            onOpenMame={() => undefined}
            debounceMs={20}
          />
        );
      }
      render(withLauncher(<Stateful />));
      return user;
    }

    it("bookmarks a classic card without opening it and shows the change at once", async () => {
      const saving = deferred<boolean>();
      commands({
        get_list: () => favoritesList([pacman]),
        set_list_membership: () => saving.promise,
      });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");
      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "true");
      });

      expect(bookmark("Pac-Man")).toHaveAttribute("title", "Remove from favorites");
      const galaga = bookmark("Galaga");
      expect(galaga).toHaveAttribute("aria-pressed", "false");
      expect(galaga).toHaveAttribute("title", "Add to favorites");
      // Next to the card button, never inside it.
      expect(galaga.closest(".card")).toBeNull();

      await user.click(galaga);

      expect(galaga).toHaveAttribute("aria-pressed", "true");
      expect(galaga).toHaveAttribute("aria-busy", "true");
      expect(membershipCalls()).toEqual([
        { listId: "favorites", entry: { kind: "machine", name: "galaga" }, member: true },
      ]);
      // A second click while saving is ignored.
      await user.click(galaga);
      expect(membershipCalls()).toHaveLength(1);
      saving.resolve(true);
      await waitFor(() => {
        expect(galaga).not.toHaveAttribute("aria-busy");
      });
      expect(galaga).toHaveAttribute("title", "Remove from favorites");
      expect(screen.queryByRole("heading", { name: "Galaga" })).not.toBeInTheDocument();
      expect(invokeMock).not.toHaveBeenCalledWith("get_machine_detail", expect.anything());
    });

    it("toggles a bookmark with the keyboard", async () => {
      commands({ get_list: () => favoritesList([pacman]), set_list_membership: saveMembership });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");
      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "true");
      });

      bookmark("Pac-Man").focus();
      await user.keyboard(" ");

      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "false");
      });
      expect(bookmark("Pac-Man")).toHaveFocus();
      expect(membershipCalls()).toEqual([{ listId: "favorites", entry: pacman, member: false }]);
      expect(screen.queryByRole("heading", { name: "Pac-Man" })).not.toBeInTheDocument();
    });

    it("restores the bookmark and explains a change that cannot be saved", async () => {
      commands({
        set_list_membership: () => {
          throw Object.assign(new Error("settings"), { kind: "settings", message: "disk full" });
        },
      });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");
      await waitFor(() => {
        expect(bookmark("Galaga")).toBeEnabled();
      });

      await user.click(bookmark("Galaga"));

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Could not add to favorites: Settings error: disk full",
      );
      expect(bookmark("Galaga")).toHaveAttribute("aria-pressed", "false");
      await user.click(screen.getByRole("button", { name: "Dismiss" }));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("disables bookmarks when the favorites cannot be read", async () => {
      commands({
        get_list: () => {
          throw Object.assign(new Error("settings"), { kind: "settings", message: "invalid" });
        },
      });
      renderBrowser();
      await screen.findByText("1–3 of 3 machines");

      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("title", "Favorites are unavailable");
      });
      expect(bookmark("Pac-Man")).toBeDisabled();
    });

    it("uses the same bookmark on media tiles, for machines and software", async () => {
      commands({
        get_list: () => favoritesList([{ kind: "software", listName: "nes", name: "smb" }]),
        set_list_membership: saveMembership,
        search_catalog_software: () =>
          response([
            software("nes", "smb", { description: "Super Mario Bros." }),
            software("snes", "smw", { description: "Super Mario World" }),
          ]),
      });
      const { user } = renderBrowser({
        preferences: { ...PREFERENCES, renderer: "mediaGrid" },
      });
      await screen.findByText("1–3 of 3 machines");
      expect(results().getByRole("button", { name: "Pac-Man (pacman)" })).toBeInTheDocument();
      expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "false");
      expect(bookmark("Pac-Man").closest(".media-tile")).toBeNull();

      await user.selectOptions(screen.getByLabelText("Catalog type"), "Software");
      await screen.findByRole("button", { name: "Super Mario Bros. (nes/smb)" });
      await waitFor(() => {
        expect(bookmark("Super Mario Bros.")).toHaveAttribute("aria-pressed", "true");
      });
      await user.click(bookmark("Super Mario World"));

      expect(membershipCalls()).toEqual([
        {
          listId: "favorites",
          entry: { kind: "software", listName: "snes", name: "smw" },
          member: true,
        },
      ]);
      expect(invokeMock).not.toHaveBeenCalledWith("get_software_detail", expect.anything());
    });

    it("restricts the search to favorites with the text, filters and sort, from the first page", async () => {
      commands({ search_catalog_machines: machineSearch(MACHINES, { total: 400 }) });
      const user = renderWithPreferences({ viewOptionsVisible: true });
      await user.type(await screen.findByLabelText("Search text"), "pac{Enter}");
      await user.click(await screen.findByRole("checkbox", { name: /Namco/ }));
      await user.selectOptions(screen.getByLabelText("Sort by"), "year");
      await user.click(await screen.findByRole("button", { name: "Page 2" }));
      await waitFor(() => {
        expect(lastRequest()?.page).toBe(1);
      });
      resultsScroll().scrollTop = 900;
      const panel = within(screen.getByRole("complementary", { name: "View options" }));

      await user.click(panel.getByRole("checkbox", { name: "Favorites only" }));

      await waitFor(() => {
        expect(lastRequest()).toMatchObject({
          text: "pac",
          sort: "year",
          page: 0,
          filters: { manufacturers: ["Namco"], list: "favorites" },
        });
      });
      expect(lastFacetRequest()?.filters).toMatchObject({ list: "favorites" });
      expect(resultsScroll().scrollTop).toBe(0);
      const active = within(screen.getByRole("group", { name: "Active filters" }));
      expect(active.getByText("Favorites only")).toBeInTheDocument();

      // Clearing the filters keeps the view option.
      await user.click(active.getByRole("button", { name: "Clear all filters" }));
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({ list: "favorites" });
      });

      await user.click(
        screen.getByRole("button", { name: "Show all results, not only favorites" }),
      );
      await waitFor(() => {
        expect(lastRequest()?.filters).toEqual({});
      });
      expect(panel.getByRole("checkbox", { name: "Favorites only" })).not.toBeChecked();
    });

    it("removes an unbookmarked favorite from the results and keeps a valid page", async () => {
      const names = Array.from({ length: 51 }, (_, index) => `m${String(index).padStart(2, "0")}`);
      const favorites = new Set(names);
      commands({
        get_list: () => favoritesList(names.map((name) => ({ kind: "machine", name }))),
        set_list_membership: (args) => {
          const entry = args.entry as { name: string };
          if (args.member) favorites.add(entry.name);
          else favorites.delete(entry.name);
          return args.member;
        },
        // The backend applies the list before paging.
        search_catalog_machines: (args) => {
          const request = machineRequest(args);
          const matching = names
            .filter((name) => request.filters?.list !== "favorites" || favorites.has(name))
            .map((name) => machine(name, { description: `Machine ${name}` }));
          const pageSize = request.pageSize ?? 50;
          const page = request.page ?? 0;
          return response(matching.slice(page * pageSize, (page + 1) * pageSize), {
            page,
            pageSize,
            total: matching.length,
            facets: facets(),
          });
        },
      });
      const user = renderWithPreferences({ favoritesOnly: true });
      expect(await screen.findByText("1–50 of 51 machines")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Page 2" }));
      expect(await screen.findByText("51–51 of 51 machines")).toBeInTheDocument();
      await waitFor(() => {
        expect(bookmark("Machine m50")).toHaveAttribute("aria-pressed", "true");
      });

      await user.click(bookmark("Machine m50"));

      // Reloaded without the removed favorite: the empty second page gives way to the first.
      expect(await screen.findByText("1–50 of 50 machines")).toBeInTheDocument();
      expect(lastRequest()).toMatchObject({ page: 0, filters: { list: "favorites" } });
      expect(results().queryByText("Machine m50")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Page 2" })).not.toBeInTheDocument();
    });

    it("explains when no favorite matches and shows all results again", async () => {
      commands({
        search_catalog_machines: (args) =>
          machineRequest(args).filters?.list === "favorites"
            ? machineSearch([], { total: 0 })(args)
            : machineSearch()(args),
      });
      const user = renderWithPreferences({ favoritesOnly: true });

      expect(
        await screen.findByText("No favorites match the search and filters in this catalog."),
      ).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Show all results" }));

      expect(await screen.findByText("1–3 of 3 machines")).toBeInTheDocument();
      expect(lastRequest()?.filters).toEqual({});
    });

    it("keeps the same favorites with another MAME installation", async () => {
      commands({
        get_list: () => favoritesList([pacman]),
        search_catalog_machines: (args) =>
          args.installationId === OLD_ID
            ? machineSearch(MACHINES.slice(0, 1))(args)
            : machineSearch()(args),
      });
      const { user } = renderBrowser({ preferences: { ...PREFERENCES, viewOptionsVisible: true } });
      await screen.findByText("1–3 of 3 machines");
      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "true");
      });

      const panel = within(screen.getByRole("complementary", { name: "View options" }));
      await user.selectOptions(panel.getByRole("combobox", { name: "MAME version" }), OLD_ID);

      expect(await screen.findByText("1–1 of 1 machines")).toBeInTheDocument();
      expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "true");
      // Favorites are global: they are not read again for another installation.
      expect(invokeMock.mock.calls.filter(([name]) => name === "get_list")).toHaveLength(1);
    });

    it("shares the bookmark of the detail header with the results", async () => {
      commands({
        get_list: () => favoritesList([pacman]),
        set_list_membership: saveMembership,
        get_machine_detail: () => machineDetail("pacman", { description: "Pac-Man" }),
      });
      const { user } = renderBrowser();
      await screen.findByText("1–3 of 3 machines");
      await waitFor(() => {
        expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "true");
      });
      await openCard("Pac-Man");
      const header = await screen.findByRole("heading", { name: "Pac-Man" });
      const detail = within(required(header.closest("article")));
      const detailBookmark = detail.getByRole("button", { name: "Favorite: Pac-Man" });
      expect(detailBookmark).toHaveAttribute("aria-pressed", "true");

      await user.click(detailBookmark);
      await waitFor(() => {
        expect(detailBookmark).toHaveAttribute("aria-pressed", "false");
      });
      await user.click(screen.getByRole("button", { name: "Back to results" }));

      expect(bookmark("Pac-Man")).toHaveAttribute("aria-pressed", "false");
      expect(membershipCalls()).toEqual([{ listId: "favorites", entry: pacman, member: false }]);
    });
  });
});
