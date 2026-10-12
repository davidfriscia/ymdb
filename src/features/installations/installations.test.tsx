import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyCatalogStatus } from "../../test/catalogFixtures";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { launchConfig, launchConfigView, session } from "../../test/launcherFixtures";
import { remoteSourcesView } from "../../test/remoteFixtures";
import { LauncherProvider } from "../launcher/LauncherProvider";
import type { SessionInfo } from "../launcher/launcherApi";
import type { ValidationReport } from "../mame-installation/mameApi";
import type { InstallationSummary } from "./installationsApi";
import { InstallationsProvider } from "./InstallationsProvider";
import { MamePage } from "./MamePage";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

const ADDED_ID = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NEW_PATH = "D:\\Émulateurs MAME ✓\\0.281\\mame.exe";

function report(path: string, valid = true): ValidationReport {
  return {
    executablePath: path,
    status: valid ? { kind: "valid" } : { kind: "unrecognizedOutput" },
    version: valid ? { release: "0.281", revision: "mame0281" } : null,
    output: { exitCode: 0, stdout: valid ? "0.281 (mame0281)" : "hello", stderr: "" },
    checkedAt: Date.UTC(2026, 9, 10),
  };
}

/** A stateful fake registry backend. */
function registry(
  options: {
    installations?: InstallationSummary[];
    sessions?: SessionInfo[];
    overrides?: Partial<Record<string, Handler>>;
  } = {},
) {
  let list = options.installations ?? [
    installation(NEW_ID, "0.280"),
    installation(OLD_ID, "0.252"),
  ];
  let active: string | null = list[0]?.id ?? null;
  const view = () => installationsView(list, active);
  const handlers: Partial<Record<string, Handler>> = {
    list_installations: view,
    set_active_installation: (args) => {
      active = args.installationId as string;
      return view();
    },
    pick_mame_executable: () => NEW_PATH,
    add_installation: (args) => {
      const added = installation(ADDED_ID, "0.281", {
        executablePath: args.path as string,
        catalogImported: false,
      });
      list = [...list, added];
      return { report: report(args.path as string), installation: added };
    },
    verify_installation: (args) => ({
      report: report(list.find((entry) => entry.id === args.installationId)?.executablePath ?? ""),
      installation: list.find((entry) => entry.id === args.installationId),
    }),
    rename_installation: (args) => {
      const name = args.displayName as string | null;
      list = list.map((entry) =>
        entry.id === args.installationId
          ? { ...entry, displayName: name, label: name ?? entry.label }
          : entry,
      );
      return list.find((entry) => entry.id === args.installationId);
    },
    remove_installation: (args) => {
      list = list.filter((entry) => entry.id !== args.installationId);
      if (active === args.installationId) active = list[0]?.id ?? null;
      return {
        removedInstallationId: args.installationId,
        activeInstallationId: active,
        keptPaths: args.deleteLaunchConfig
          ? []
          : [
              `C:\\Users\\me\\AppData\\Roaming\\ymdb\\installations\\${String(args.installationId)}`,
            ],
      };
    },
    detect_mame_installations: () => [
      {
        source: "path",
        report: report(list[0]?.executablePath ?? ""),
        registeredInstallationId: list[0]?.id ?? null,
      },
      { source: "conventional", report: report(NEW_PATH), registeredInstallationId: null },
    ],
    get_catalog_status: (args) => ({
      ...emptyCatalogStatus,
      installationId: args.installationId as string,
    }),
    get_launch_config: (args) =>
      launchConfigView(
        launchConfig(
          args.installationId === OLD_ID ? { machineOverrides: { galaga: { window: true } } } : {},
        ),
        args.installationId as string,
      ),
    get_launch_sessions: () => options.sessions ?? [],
    get_remote_sources: (args) => remoteSourcesView(args.installationId as string, undefined, []),
    ...options.overrides,
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

function renderPage() {
  const user = userEvent.setup();
  render(
    <InstallationsProvider>
      <LauncherProvider
        installationId={null}
        showCommandPreview={false}
        onShowCommandPreviewChange={() => undefined}
      >
        <MamePage refreshKey={0} onCatalogChanged={() => undefined} />
      </LauncherProvider>
    </InstallationsProvider>,
  );
  return user;
}

const sidebar = () => within(screen.getByRole("navigation", { name: "MAME installations" }));
const registered = () => within(sidebar().getByRole("list", { name: "Registered installations" }));

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** The header of the group of an installation, whose name starts with its label. */
function groupHeader(label: string) {
  return sidebar().getByRole("button", { name: new RegExp(`^${escaped(label)} `) });
}

function groupHeaders() {
  return registered()
    .getAllByRole("button")
    .filter((button) => button.hasAttribute("aria-expanded"));
}

/** Opens a section of an installation from the navigation, expanding its group. */
async function open(user: ReturnType<typeof userEvent.setup>, label: string, section: string) {
  const header = await waitFor(() => groupHeader(label));
  if (header.getAttribute("aria-expanded") === "false") await user.click(header);
  await user.click(sidebar().getByRole("button", { name: `${section} of ${label}` }));
}

/** The content panel, named by the selected section. */
function content(section: string) {
  return within(screen.getByRole("region", { name: section }));
}

/** Checks the heading of the content panel and the installation it shows. */
async function expectShown(section: string, label: string) {
  expect(await screen.findByRole("heading", { level: 2, name: section })).toBeVisible();
  await waitFor(() => {
    expect(screen.getByRole("region", { name: section })).toHaveTextContent(
      new RegExp(`^${escaped(label)}`),
    );
  });
}

describe("MAME page", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("groups the sections by installation and opens the status of the active one", async () => {
    registry({
      installations: [
        installation(NEW_ID, "0.280"),
        installation(OLD_ID, "0.252", { catalogImported: false, executableStatus: "changed" }),
      ],
    });
    renderPage();

    expect(await screen.findByRole("heading", { level: 1, name: "MAME" })).toBeVisible();
    await expectShown("Status", "MAME 0.280");
    expect(content("Status").getByText("Active", { selector: ".badge" })).toBeVisible();
    expect(content("Status").getByText("C:\\Emulators\\MAME 0.280\\mame.exe")).toBeVisible();
    // The navigation starts with Add Installation, then one group per installation.
    expect(sidebar().getAllByRole("button")[0]).toHaveAccessibleName("Add Installation");
    expect(groupHeaders()).toHaveLength(2);
    expect(groupHeader("MAME 0.280")).toHaveAttribute("aria-expanded", "true");
    expect(groupHeader("MAME 0.280")).toHaveTextContent("Active");
    expect(groupHeader("MAME 0.280")).toHaveTextContent("MAME 0.280 (mame0280) · Catalog imported");
    expect(groupHeader("MAME 0.252")).toHaveAttribute("aria-expanded", "false");
    expect(groupHeader("MAME 0.252")).not.toHaveTextContent("Active");
    expect(groupHeader("MAME 0.252")).toHaveTextContent("Changed");
    expect(groupHeader("MAME 0.252")).toHaveTextContent("No catalog");
    expect(
      within(registered().getByRole("list", { name: "MAME 0.280" }))
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual([
      "Status of MAME 0.280",
      "Catalog of MAME 0.280",
      "ROM Manager of MAME 0.280",
      "Launch Options of MAME 0.280",
    ]);
    expect(sidebar().getByRole("button", { name: "Status of MAME 0.280" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    // Only the shown section is loaded.
    expect(invokeMock).not.toHaveBeenCalledWith("get_catalog_status", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("get_remote_sources", expect.anything());
  });

  it("shows each section of the chosen installation without changing the active one", async () => {
    registry();
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    await open(user, "MAME 0.252", "Catalog");
    await expectShown("Catalog", "MAME 0.252");
    expect(await screen.findByRole("region", { name: "Catalog of MAME 0.252" })).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: OLD_ID });
    expect(await content("Catalog").findByRole("button", { name: "Import catalog" })).toBeVisible();

    await open(user, "MAME 0.252", "ROM Manager");
    await expectShown("ROM Manager", "MAME 0.252");
    expect(await screen.findByRole("group", { name: "Machine ROMs" })).toBeVisible();
    expect(screen.getByRole("group", { name: "Samples" })).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("get_remote_sources", { installationId: OLD_ID });
    // The catalog stays mounted but hidden.
    expect(screen.queryByRole("region", { name: "Catalog of MAME 0.252" })).not.toBeInTheDocument();

    await open(user, "MAME 0.252", "Launch Options");
    await expectShown("Launch Options", "MAME 0.252");
    expect(
      await screen.findByRole("list", { name: "Machine and software options" }),
    ).toHaveTextContent("galaga · window: On");
    expect(screen.getByRole("group", { name: "Global options" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Create profile" })).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("get_launch_config", { installationId: OLD_ID });

    await open(user, "MAME 0.280", "Launch Options");
    await expectShown("Launch Options", "MAME 0.280");
    expect(await screen.findByText("No machine or software has its own options.")).toBeVisible();

    expect(invokeMock).not.toHaveBeenCalledWith("set_active_installation", expect.anything());
    // MAME is never started by a selection.
    expect(invokeMock).not.toHaveBeenCalledWith("start_launch", expect.anything());
  });

  it("makes the shown installation active from its status", async () => {
    registry();
    const user = renderPage();
    await open(user, "MAME 0.252", "Status");
    await expectShown("Status", "MAME 0.252");

    await user.click(content("Status").getByRole("button", { name: "Set as active" }));
    expect(invokeMock).toHaveBeenCalledWith("set_active_installation", { installationId: OLD_ID });
    expect(await screen.findByRole("button", { name: "Active" })).toBeDisabled();
    await waitFor(() => {
      expect(groupHeader("MAME 0.252")).toHaveTextContent("Active");
    });
    expect(groupHeader("MAME 0.280")).not.toHaveTextContent("Active");
    // The shown installation does not change.
    await expectShown("Status", "MAME 0.252");
  });

  it("tells installations with the same version apart by their id", async () => {
    registry({
      installations: [
        installation(NEW_ID, "0.280", { executablePath: "C:\\MAME\\stable\\mame.exe" }),
        installation(OLD_ID, "0.280", { executablePath: "C:\\MAME\\debug\\mame.exe" }),
      ],
    });
    const user = renderPage();
    await expectShown("Status", "MAME 0.280 · stable");
    expect(groupHeader("MAME 0.280 · debug")).toBeVisible();

    await open(user, "MAME 0.280 · debug", "Status");
    await expectShown("Status", "MAME 0.280 · debug");
    expect(content("Status").getByText("C:\\MAME\\debug\\mame.exe")).toBeVisible();
    await open(user, "MAME 0.280 · debug", "ROM Manager");
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_remote_sources", { installationId: OLD_ID });
    });
    expect(invokeMock).not.toHaveBeenCalledWith("get_remote_sources", { installationId: NEW_ID });
    expect(
      sidebar().getByRole("button", { name: "ROM Manager of MAME 0.280 · debug" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      sidebar().getByRole("button", { name: "Status of MAME 0.280 · stable" }),
    ).not.toHaveAttribute("aria-current");
  });

  it("expands and collapses the groups", async () => {
    registry();
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    await user.click(groupHeader("MAME 0.280"));
    expect(groupHeader("MAME 0.280")).toHaveAttribute("aria-expanded", "false");
    expect(
      sidebar().queryByRole("button", { name: "Status of MAME 0.280" }),
    ).not.toBeInTheDocument();
    // Collapsing a group does not change the shown section.
    await expectShown("Status", "MAME 0.280");

    await user.click(groupHeader("MAME 0.252"));
    expect(sidebar().getByRole("button", { name: "Catalog of MAME 0.252" })).toBeVisible();
    await user.click(groupHeader("MAME 0.280"));
    expect(sidebar().getByRole("button", { name: "Status of MAME 0.280" })).toBeVisible();
  });

  it("moves through the navigation with the keyboard", async () => {
    registry();
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    sidebar().getByRole("button", { name: "Add Installation" }).focus();
    await user.keyboard("{ArrowDown}");
    expect(groupHeader("MAME 0.280")).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(sidebar().getByRole("button", { name: "Catalog of MAME 0.280" })).toHaveFocus();
    await user.keyboard("{Enter}");
    await expectShown("Catalog", "MAME 0.280");
    // Sections of collapsed groups are skipped: End reaches the header of MAME 0.252.
    await user.keyboard("{End}");
    expect(groupHeader("MAME 0.252")).toHaveFocus();
    await user.keyboard(" ");
    expect(groupHeader("MAME 0.252")).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{End}");
    expect(sidebar().getByRole("button", { name: "Launch Options of MAME 0.252" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(sidebar().getByRole("button", { name: "Add Installation" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(sidebar().getByRole("button", { name: "Add Installation" })).toHaveFocus();
    // Tab moves through the same buttons.
    await user.tab();
    expect(groupHeader("MAME 0.280")).toHaveFocus();
  });

  it("keeps unsaved changes of a section while other sections are shown", async () => {
    registry();
    const user = renderPage();
    await open(user, "MAME 0.280", "ROM Manager");
    const roms = within(await screen.findByRole("group", { name: "Machine ROMs" }));
    await user.type(roms.getByLabelText("Remote base path"), "/volume1/mame/roms");

    await open(user, "MAME 0.280", "Status");
    await expectShown("Status", "MAME 0.280");
    await open(user, "MAME 0.252", "ROM Manager");
    await expectShown("ROM Manager", "MAME 0.252");
    // The other installation has its own, untouched settings.
    const other = within(
      await screen.findByRole("region", { name: "ROM Manager settings of MAME 0.252" }),
    );
    expect(
      within(await other.findByRole("group", { name: "Machine ROMs" })).getByLabelText(
        "Remote base path",
      ),
    ).toHaveValue("");

    await open(user, "MAME 0.280", "ROM Manager");
    const own = within(screen.getByRole("region", { name: "ROM Manager settings of MAME 0.280" }));
    expect(
      within(own.getByRole("group", { name: "Machine ROMs" })).getByLabelText("Remote base path"),
    ).toHaveValue("/volume1/mame/roms");
    expect(own.getByRole("button", { name: "Save ROM Manager settings" })).toBeEnabled();
    expect(invokeMock).not.toHaveBeenCalledWith("save_remote_sources", expect.anything());
  });

  it("keeps unsaved launch options while other sections are shown", async () => {
    registry();
    const user = renderPage();
    await open(user, "MAME 0.280", "Launch Options");
    const global = within(await screen.findByRole("group", { name: "Global options" }));
    await user.selectOptions(global.getByRole("combobox"), "window");
    await user.type(await screen.findByLabelText("New profile name"), "Cabinet");

    await open(user, "MAME 0.280", "Catalog");
    await expectShown("Catalog", "MAME 0.280");
    await open(user, "MAME 0.280", "Launch Options");

    expect(screen.getByLabelText("New profile name")).toHaveValue("Cabinet");
    expect(
      within(screen.getByRole("group", { name: "Global options" })).getByText("-window"),
    ).toBeVisible();
    expect(invokeMock).not.toHaveBeenCalledWith("save_launch_config", expect.anything());
  });

  it("adds an installation and shows it without importing its catalog or changing the others", async () => {
    registry();
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    await user.click(sidebar().getByRole("button", { name: "Add Installation" }));
    expect(screen.getByRole("heading", { level: 2, name: "Add Installation" })).toBeVisible();
    expect(sidebar().getByRole("button", { name: "Add Installation" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await user.click(screen.getByRole("button", { name: "Add installation…" }));

    expect(invokeMock).toHaveBeenCalledWith("add_installation", { path: NEW_PATH });
    // The new installation is shown at once, with its group expanded.
    await expectShown("Status", "MAME 0.281");
    expect(screen.getByRole("status")).toHaveTextContent(
      "MAME 0.281 was added. Its catalog has not been imported yet.",
    );
    expect(groupHeaders()).toHaveLength(3);
    expect(groupHeader("MAME 0.281")).toHaveAttribute("aria-expanded", "true");
    expect(sidebar().getByRole("button", { name: "Status of MAME 0.281" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    // The active installation does not change.
    expect(groupHeader("MAME 0.280")).toHaveTextContent("Active");
    expect(groupHeader("MAME 0.281")).not.toHaveTextContent("Active");
    expect(invokeMock).not.toHaveBeenCalledWith("set_active_installation", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("start_catalog_import", expect.anything());

    // Its configuration is reachable immediately; the notice goes with the navigation.
    await open(user, "MAME 0.281", "Catalog");
    await expectShown("Catalog", "MAME 0.281");
    expect(screen.queryByText(/MAME 0.281 was added/)).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: ADDED_ID });
  });

  it("explains a file that is not MAME and an executable already registered", async () => {
    registry({
      overrides: {
        add_installation: (args) => ({
          report: report(args.path as string, false),
          installation: null,
        }),
      },
    });
    const user = renderPage();
    await user.click(await screen.findByRole("button", { name: "Add Installation" }));
    await user.click(screen.getByRole("button", { name: "Add installation…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The selected file was not added: The program did not report a MAME version.",
    );
    expect(groupHeaders()).toHaveLength(2);
    expect(screen.getByRole("heading", { level: 2, name: "Add Installation" })).toBeVisible();

    registry({
      overrides: {
        add_installation: () => {
          throw Object.assign(new Error("duplicate"), {
            kind: "duplicateInstallation",
            existingInstallationId: OLD_ID,
          });
        },
      },
    });
    await user.click(screen.getByRole("button", { name: "Add installation…" }));
    expect(
      await screen.findByText("This executable is already registered as MAME 0.252."),
    ).toBeInTheDocument();
    await expectShown("Status", "MAME 0.252");
  });

  it("detects installations and keeps the list after adding one", async () => {
    registry();
    const user = renderPage();
    await user.click(await screen.findByRole("button", { name: "Add Installation" }));
    await user.click(screen.getByRole("button", { name: "Detect installations" }));

    const detected = within(await screen.findByRole("list", { name: "Detected installations" }));
    const items = detected.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Registered");
    await user.click(detected.getByRole("button", { name: `Add ${NEW_PATH}` }));

    expect(invokeMock).toHaveBeenCalledWith("add_installation", { path: NEW_PATH });
    await expectShown("Status", "MAME 0.281");

    await user.click(sidebar().getByRole("button", { name: "Add Installation" }));
    expect(screen.getByRole("list", { name: "Detected installations" })).toBeVisible();
    expect(detected.getAllByText("Registered")).toHaveLength(2);
  });

  it("verifies and renames the shown installation", async () => {
    registry();
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(invokeMock).toHaveBeenCalledWith("verify_installation", { installationId: NEW_ID });
    expect(await screen.findByText("Valid MAME installation.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Rename…" }));
    await user.type(screen.getByLabelText("Installation name"), "Arcade ✓");
    await user.click(screen.getByRole("button", { name: "Save name" }));
    expect(invokeMock).toHaveBeenCalledWith("rename_installation", {
      installationId: NEW_ID,
      displayName: "Arcade ✓",
    });
    await expectShown("Status", "Arcade ✓ (MAME 0.280)");
    expect(groupHeader("Arcade ✓ (MAME 0.280)")).toBeVisible();
  });

  it("removes an installation after confirmation and shows a remaining one", async () => {
    registry();
    const user = renderPage();
    // Sections stay mounted once visited.
    await open(user, "MAME 0.280", "ROM Manager");
    await screen.findByRole("region", { name: "ROM Manager settings of MAME 0.280" });
    await open(user, "MAME 0.280", "Status");

    await user.click(screen.getByRole("button", { name: "Remove…" }));
    const confirm = within(screen.getByRole("group", { name: "Remove installation" }));
    expect(confirm.getByText(/never deleted/)).toBeInTheDocument();
    expect(confirm.getByRole("checkbox", { name: /delete its imported catalog/ })).toBeChecked();
    expect(confirm.getByRole("checkbox", { name: /delete its launch options/ })).not.toBeChecked();
    expect(
      confirm.getByText(/the first remaining installation becomes active/),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("remove_installation", expect.anything());

    await user.click(confirm.getByRole("button", { name: "Remove installation" }));

    expect(invokeMock).toHaveBeenCalledWith("remove_installation", {
      installationId: NEW_ID,
      deleteCatalog: true,
      deleteLaunchConfig: false,
    });
    expect(await screen.findByText("MAME 0.280 was removed from YMDB.")).toBeInTheDocument();
    expect(screen.getByText(/installations\\0123456789abcdef/)).toBeInTheDocument();
    // The selection moves to the remaining (now active) installation.
    await expectShown("Status", "MAME 0.252");
    expect(groupHeaders()).toHaveLength(1);
    expect(sidebar().queryByRole("button", { name: /of MAME 0\.280$/ })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "ROM Manager settings of MAME 0.280", hidden: true }),
    ).not.toBeInTheDocument();
  });

  it("does not remove an installation while MAME runs from it", async () => {
    registry({ sessions: [session(1, "pacman", { state: "running" }, null, NEW_ID)] });
    const user = renderPage();
    await expectShown("Status", "MAME 0.280");

    await user.click(screen.getByRole("button", { name: "Remove…" }));
    const confirm = within(screen.getByRole("group", { name: "Remove installation" }));
    expect(await confirm.findByText(/MAME is running from this installation/)).toBeInTheDocument();
    expect(confirm.getByRole("button", { name: "Remove installation" })).toBeDisabled();
  });

  it("shows how to start without installations", async () => {
    registry({ installations: [] });
    renderPage();
    expect(await screen.findByText(/No MAME installation is registered yet/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Add Installation" })).toBeVisible();
    expect(sidebar().getByText("No installation registered.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Add installation…" })).toBeEnabled();
    expect(screen.queryByRole("region", { name: /Catalog of/ })).not.toBeInTheDocument();
  });

  it("shows an error when the installations cannot be read", async () => {
    invokeMock.mockImplementation((command: string) =>
      command === "list_installations"
        ? Promise.reject(new Error("disk unavailable"))
        : Promise.resolve([]),
    );
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /The MAME installations could not be read: .*disk unavailable/,
    );
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
  });
});
