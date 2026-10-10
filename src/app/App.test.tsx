import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyCatalogStatus } from "../test/catalogFixtures";
import { launchConfigView } from "../test/launcherFixtures";
import { App } from "./App";
import { PREFERENCES_KEY } from "./preferences";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const responses: Record<string, unknown> = {
  get_mame_configuration: { executablePath: null, executableExists: false },
  get_catalog_status: emptyCatalogStatus,
  get_launch_config: launchConfigView(),
  get_launch_sessions: [],
};

function storedPreferences(): unknown {
  return JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null");
}

describe("App", () => {
  beforeEach(() => {
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
    expect(await screen.findByText("No catalog yet")).toBeInTheDocument();
  });

  it("shows the MAME installation and catalog on the MAME page", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open MAME settings" }));

    expect(screen.getByRole("heading", { level: 1, name: "MAME" })).toBeVisible();
    expect(screen.getByRole("heading", { level: 2, name: "MAME Installation" })).toBeVisible();
    expect(screen.getByRole("heading", { level: 2, name: "MAME Catalog" })).toBeVisible();
    expect(await screen.findByText("No MAME executable configured.")).toBeInTheDocument();
    expect(await screen.findByText("No catalog has been imported yet.")).toBeInTheDocument();
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

  it("restores saved preferences at startup", () => {
    localStorage.setItem(
      PREFERENCES_KEY,
      JSON.stringify({ theme: "dark", cardSize: 300, filtersVisible: false, renderer: "cards" }),
    );
    render(<App />);

    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(screen.getByRole("radio", { name: "Dark", hidden: true })).toBeChecked();
    expect(storedPreferences()).toEqual({
      theme: "dark",
      cardSize: 300,
      filtersVisible: false,
      renderer: "cards",
      showCommandPreview: false,
    });
  });

  it("starts with defaults when the saved preferences are invalid", () => {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ theme: "neon", cardSize: "x" }));
    render(<App />);

    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(storedPreferences()).toMatchObject({ theme: "light", cardSize: 220 });
  });
});
