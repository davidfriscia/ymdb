import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyCatalogStatus } from "../test/catalogFixtures";
import { launchConfigView } from "../test/launcherFixtures";
import { mediaSourcesView } from "../test/mediaFixtures";
import { favoritesList } from "../test/listFixtures";
import { installationsView, OLD_ID } from "../test/installationFixtures";
import { customTheme, THEME_ID } from "../test/themeFixtures";
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
  get_remote_connections: { connections: [], loadError: null },
  get_list: favoritesList(),
  list_user_lists: [],
  get_support_files: { sources: [], dimensions: [] },
  get_classification_status: { dimensions: [], syncError: null },
  get_saved_filters: [],
  get_custom_themes: [],
  take_restore_outcome: null,
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
      "Lists",
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
    // Without installations, the page opens on Add Installation.
    expect(screen.getByRole("heading", { level: 2, name: "Add Installation" })).toBeVisible();
    expect(await screen.findByText(/No MAME installation is registered yet/)).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "MAME installations" })).toBeVisible();
    // The main navigation is unchanged.
    expect(
      within(screen.getByRole("navigation", { name: "Main" })).getByRole("button", {
        name: "MAME",
      }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("opens Settings on Appearance and shows one section at a time", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeVisible();
    const sections = within(screen.getByRole("navigation", { name: "Settings sections" }));
    expect(sections.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Appearance",
      "Media Sources",
      "Remote Connections",
      "Controllers",
      "Favorites",
      "Support Files",
      "Application Data",
      "About",
      "Diagnostics",
    ]);
    expect(sections.getByRole("button", { name: "Appearance" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("heading", { level: 2, name: "Appearance" })).toBeVisible();
    expect(screen.getByRole("radio", { name: "Light" })).toBeVisible();

    await user.click(sections.getByRole("button", { name: "Media Sources" }));
    expect(screen.getByRole("heading", { level: 2, name: "Media Sources" })).toBeVisible();
    expect(sections.getByRole("button", { name: "Media Sources" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(sections.getByRole("button", { name: "Appearance" })).not.toHaveAttribute(
      "aria-current",
    );
    // The content is replaced, not appended.
    expect(screen.queryByRole("radio", { name: "Light" })).not.toBeInTheDocument();
    expect(await screen.findAllByRole("button", { name: "Add directory…" })).toHaveLength(5);

    await user.click(sections.getByRole("button", { name: "Remote Connections" }));
    expect(screen.getByRole("heading", { level: 2, name: "Remote Connections" })).toBeVisible();
    expect(await screen.findByRole("button", { name: "Add connection…" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Add directory…" })).not.toBeInTheDocument();
  });

  it("keeps an unsaved connection form while other sections are shown", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Remote Connections" }));
    await user.click(await screen.findByRole("button", { name: "Add connection…" }));
    await user.type(screen.getByLabelText("Connection name"), "Home NAS ✓");

    await user.click(screen.getByRole("button", { name: "Appearance" }));
    expect(screen.queryByLabelText("Connection name")).not.toBeVisible();
    await user.click(screen.getByRole("button", { name: "Remote Connections" }));

    expect(screen.getByRole("form", { name: "New remote connection" })).toBeVisible();
    expect(screen.getByLabelText("Connection name")).toHaveValue("Home NAS ✓");
    expect(invokeMock).not.toHaveBeenCalledWith("save_remote_connection", expect.anything());
  });

  it("keeps the Settings section while other pages are visited", async () => {
    const user = userEvent.setup();
    render(<App />);
    const main = within(screen.getByRole("navigation", { name: "Main" }));

    await user.click(main.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Diagnostics" }));
    await user.click(main.getByRole("button", { name: "Catalog" }));
    await user.click(main.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("heading", { level: 2, name: "Diagnostics" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Check backend connection" })).toBeVisible();
  });

  it("moves through the Settings sections with the keyboard", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const sections = within(screen.getByRole("navigation", { name: "Settings sections" }));

    sections.getByRole("button", { name: "Appearance" }).focus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(sections.getByRole("button", { name: "Remote Connections" })).toHaveFocus();
    await user.keyboard("{End}");
    expect(sections.getByRole("button", { name: "Diagnostics" })).toHaveFocus();
    await user.keyboard("{ArrowUp}{Enter}");
    expect(screen.getByRole("heading", { level: 2, name: "About" })).toBeVisible();
    await user.keyboard("{Home}");
    expect(sections.getByRole("button", { name: "Appearance" })).toHaveFocus();
  });

  it("collapses the section navigation behind a toggle on narrow windows", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));

    // The toggle is only displayed on narrow windows (CSS); it names the current section.
    const toggle = screen.getByRole("button", { name: "Sections, current: Appearance" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAttribute(
      "aria-controls",
      screen.getByRole("navigation", { name: "Settings sections" }).id,
    );
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    // Choosing a section closes it.
    await user.click(screen.getByRole("button", { name: "About" }));
    expect(screen.getByRole("heading", { level: 2, name: "About" })).toBeVisible();
    expect(screen.getByRole("button", { name: /^Sections, current: About/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
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

    await user.click(screen.getByRole("button", { name: "About" }));
    expect(screen.getByText("Your MAME DB (YMDB)")).toBeVisible();
    expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeVisible();
    // The obsolete note about downloads is gone.
    expect(screen.queryByText(/not downloaded/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(screen.getByRole("heading", { level: 3, name: "Backend connection" })).toBeVisible();
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

  it("switches to Neon 80s and back immediately, keeping the choice for the next start", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    const neon = screen.getByRole("radio", { name: "Neon 80s" });
    expect(neon).toHaveAccessibleDescription(/Synthwave arcade/);
    await user.click(neon);
    expect(document.documentElement).toHaveAttribute("data-theme", "neon80s");
    expect(storedPreferences()).toMatchObject({ theme: "neon80s" });

    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    await user.click(screen.getByRole("radio", { name: "Neon 80s" }));
    unmount();

    delete document.documentElement.dataset.theme;
    render(<App />);
    expect(document.documentElement).toHaveAttribute("data-theme", "neon80s");
    expect(screen.getByRole("radio", { name: "Neon 80s", hidden: true })).toBeChecked();
  });

  it("shows a preview of every theme drawn from its own tokens", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));

    const previews = document.querySelectorAll("[data-theme-preview]");
    expect([...previews].map((preview) => preview.getAttribute("data-theme-preview"))).toEqual([
      "light",
      "dark",
      "neon80s",
    ]);
    for (const preview of previews) expect(preview).toHaveAttribute("aria-hidden", "true");
  });

  it("applies a custom theme to the whole application and switches back to a built-in one", async () => {
    responses = { ...EMPTY, get_custom_themes: [customTheme()] };
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));

    await user.click(await screen.findByRole("radio", { name: "Sunset" }));
    const root = document.documentElement;
    expect(root).toHaveAttribute("data-theme", "custom");
    expect(root.style.getPropertyValue("--color-accent")).toBe("#ff8800");
    expect(root.style.getPropertyValue("--glow-accent")).toBe("0 0 10px rgb(255 136 0 / 45%)");
    expect(storedPreferences()).toMatchObject({ theme: `custom:${THEME_ID}` });

    // A built-in theme removes every custom value.
    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(root).toHaveAttribute("data-theme", "dark");
    expect(root.style.getPropertyValue("--color-accent")).toBe("");
    expect(root.getAttribute("style") ?? "").toBe("");
  });

  it("shows Light when the selected custom theme no longer exists, keeping the preference", async () => {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ theme: `custom:${THEME_ID}` }));
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByText(/selected custom theme is not available/)).toBeVisible();
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(storedPreferences()).toMatchObject({ theme: `custom:${THEME_ID}` });
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

describe("App after a restore", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    invokeMock.mockReset();
  });

  it("applies the restored interface preferences and reports the restore", async () => {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ theme: "light" }));
    responses = {
      ...EMPTY,
      take_restore_outcome: {
        kind: "restored",
        createdAt: Date.UTC(2026, 9, 24, 12, 0),
        appVersion: "0.1.0",
        interfacePreferences: { [PREFERENCES_KEY]: JSON.stringify({ theme: "dark" }) },
      },
      get_application_data: {
        configDir: "C:/ymdb/config",
        dataDir: "C:/ymdb/data",
        customDataDir: false,
        entries: [],
        externalReferences: [],
        restorePending: false,
      },
      get_backup_status: { state: "idle" },
    };
    invokeMock.mockImplementation((command: string) =>
      command in responses
        ? Promise.resolve(responses[command])
        : Promise.reject(new Error(`unexpected command ${command}`)),
    );

    render(<App />);

    expect(
      await screen.findByText(/Application data restored from the backup created on/),
    ).toBeVisible();
    expect(screen.getByRole("heading", { level: 2, name: "Application Data" })).toBeVisible();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(storedPreferences()).toMatchObject({ theme: "dark" });
    expect(invokeMock.mock.calls.filter(([name]) => name === "take_restore_outcome")).toHaveLength(
      1,
    );
  });
});
