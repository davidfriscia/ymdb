import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyCatalogStatus } from "../test/catalogFixtures";
import { launchConfigView } from "../test/launcherFixtures";
import { mediaSourcesView } from "../test/mediaFixtures";
import { installationsView, OLD_ID } from "../test/installationFixtures";
import { App } from "./App";
import { LEGACY_PREFERENCES_KEY, PREFERENCES_KEY } from "./preferences";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

let responses: Record<string, unknown> = {};

const EMPTY: Record<string, unknown> = {
  list_installations: installationsView([], null),
  get_catalog_status: emptyCatalogStatus,
  get_launch_config: launchConfigView(),
  get_launch_sessions: [],
  get_media_sources: mediaSourcesView(),
};

function storedPreferences(): unknown {
  return JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null");
}

describe("App", () => {
  beforeEach(() => {
    responses = EMPTY;
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) =>
      command in responses
        ? Promise.resolve(responses[command])
        : Promise.reject(new Error(`unexpected command ${command}`)),
    );
  });

  it("opens on the catalog, with the main navigation", async () => {
    render(<App />);

    const navigation = within(screen.getByRole("navigation", { name: "Main" }));
    expect(navigation.getByText("YMDB")).toBeInTheDocument();
    expect(navigation.getByRole("button", { name: "Catalog" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(navigation.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Catalog",
      "MAME",
      "Settings",
    ]);
    expect(await screen.findByText("No MAME installation")).toBeInTheDocument();
  });

  it("shows the MAME installations on the MAME page", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open MAME settings" }));

    expect(screen.getByRole("heading", { level: 1, name: "MAME" })).toBeVisible();
    expect(screen.getByRole("heading", { level: 2, name: "MAME Installations" })).toBeVisible();
    expect(await screen.findByText(/No MAME installation is registered yet/)).toBeInTheDocument();
  });

  it("keeps interface preferences global when the MAME version changes", async () => {
    responses = {
      ...EMPTY,
      list_installations: installationsView(),
      set_active_installation: installationsView(undefined, OLD_ID),
      get_catalog_status: emptyCatalogStatus,
    };
    localStorage.setItem(
      PREFERENCES_KEY,
      JSON.stringify({ theme: "dark", viewOptionsVisible: true, renderer: "mediaGrid" }),
    );
    const user = userEvent.setup();
    render(<App />);
    const before = storedPreferences();

    await user.selectOptions(await screen.findByRole("combobox", { name: "MAME version" }), OLD_ID);

    expect(await screen.findByRole("combobox", { name: "MAME version" })).toHaveValue(OLD_ID);
    expect(invokeMock).toHaveBeenCalledWith("set_active_installation", { installationId: OLD_ID });
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(storedPreferences()).toEqual(before);
    expect(Object.keys(localStorage)).toEqual([PREFERENCES_KEY]);
  });

  it("shows the version and backend check in the settings", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeVisible();
    expect(screen.getByRole("button", { name: "Check backend connection" })).toBeEnabled();
  });

  it("uses the light theme by default and switches the whole application to dark", async () => {
    const user = userEvent.setup();
    render(<App />);
    expect(document.documentElement).toHaveAttribute("data-theme", "light");

    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("radio", { name: "Light" })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: "Dark" }));

    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(storedPreferences()).toMatchObject({ theme: "dark" });
  });

  it("restores saved preferences at startup, migrating the previous format", () => {
    localStorage.setItem(
      LEGACY_PREFERENCES_KEY,
      JSON.stringify({ theme: "dark", cardSize: 300, filtersVisible: false, renderer: "cards" }),
    );
    render(<App />);

    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(screen.getByRole("radio", { name: "Dark", hidden: true })).toBeChecked();
    expect(storedPreferences()).toMatchObject({
      theme: "dark",
      filtersVisible: false,
      renderer: "classicCards",
      rendererOptions: { classicCards: { cardSize: 300 } },
    });
    expect(localStorage.getItem(LEGACY_PREFERENCES_KEY)).toBeNull();
  });

  it("starts with defaults when the saved preferences are invalid", () => {
    localStorage.setItem(
      PREFERENCES_KEY,
      JSON.stringify({ theme: "neon", renderer: "table", rendererOptions: "x" }),
    );
    render(<App />);

    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(storedPreferences()).toMatchObject({
      theme: "light",
      renderer: "classicCards",
      rendererOptions: { classicCards: { cardSize: 220 } },
    });
  });
});
