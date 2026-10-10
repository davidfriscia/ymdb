import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyCatalogStatus } from "../../test/catalogFixtures";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { session } from "../../test/launcherFixtures";
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
    get_launch_config: () => {
      throw new Error("not needed");
    },
    get_launch_sessions: () => options.sessions ?? [],
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

const registered = () => within(screen.getByRole("list", { name: "Registered installations" }));

describe("MAME page", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("lists every installation and shows the active one with its catalog", async () => {
    registry({
      installations: [
        installation(NEW_ID, "0.280"),
        installation(OLD_ID, "0.252", { catalogImported: false, executableStatus: "changed" }),
      ],
    });
    renderPage();

    const list = await screen.findByRole("list", { name: "Registered installations" });
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("MAME 0.280Active");
    expect(items[0]).toHaveTextContent("Catalog imported");
    expect(items[1]).toHaveTextContent("No catalog");
    expect(items[1]).toHaveTextContent("The executable changed since it was verified");
    // The active installation is shown below, with its catalog.
    expect(await screen.findByRole("heading", { level: 2, name: "MAME 0.280" })).toBeVisible();
    expect(
      await screen.findByRole("heading", { level: 2, name: "Catalog of MAME 0.280" }),
    ).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: NEW_ID });
  });

  it("names installations with the same version by their folder", async () => {
    registry({
      installations: [
        installation(NEW_ID, "0.280", { executablePath: "C:\\MAME\\stable\\mame.exe" }),
        installation(OLD_ID, "0.280", { executablePath: "C:\\MAME\\debug\\mame.exe" }),
      ],
    });
    renderPage();
    await screen.findByRole("list", { name: "Registered installations" });
    expect(registered().getByRole("button", { name: "Manage MAME 0.280 · stable" })).toBeVisible();
    expect(registered().getByRole("button", { name: "Manage MAME 0.280 · debug" })).toBeVisible();
  });

  it("selects an installation to manage and makes it active", async () => {
    registry();
    const user = renderPage();

    await user.click(await screen.findByRole("button", { name: "Manage MAME 0.252" }));
    expect(await screen.findByRole("heading", { level: 2, name: "MAME 0.252" })).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: OLD_ID });
    expect(invokeMock).not.toHaveBeenCalledWith("set_active_installation", expect.anything());

    await user.click(screen.getByRole("button", { name: "Set as active" }));
    expect(invokeMock).toHaveBeenCalledWith("set_active_installation", { installationId: OLD_ID });
    expect(await screen.findByRole("button", { name: "Active" })).toBeDisabled();
    // MAME is never started by a selection.
    expect(invokeMock).not.toHaveBeenCalledWith("start_launch", expect.anything());
  });

  it("adds an installation without importing its catalog or changing the others", async () => {
    registry();
    const user = renderPage();
    await screen.findByRole("heading", { level: 2, name: "MAME 0.280" });

    await user.click(screen.getByRole("button", { name: "Add installation…" }));

    expect(invokeMock).toHaveBeenCalledWith("add_installation", { path: NEW_PATH });
    expect(await screen.findByText(/MAME 0.281 was added/)).toBeInTheDocument();
    expect(registered().getAllByRole("listitem")).toHaveLength(3);
    // The new installation is selected here; the active one does not change.
    expect(await screen.findByRole("heading", { level: 2, name: "MAME 0.281" })).toBeVisible();
    const [first] = registered().getAllByRole("listitem");
    expect(first).toHaveTextContent("MAME 0.280Active");
    expect(invokeMock).not.toHaveBeenCalledWith("set_active_installation", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("start_catalog_import", expect.anything());
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
    await user.click(await screen.findByRole("button", { name: "Add installation…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The selected file was not added: The program did not report a MAME version.",
    );
    expect(registered().getAllByRole("listitem")).toHaveLength(2);

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
    expect(await screen.findByRole("heading", { level: 2, name: "MAME 0.252" })).toBeVisible();
  });

  it("detects installations and adds the unregistered ones", async () => {
    registry();
    const user = renderPage();
    await user.click(await screen.findByRole("button", { name: "Detect installations" }));

    const detected = within(await screen.findByRole("list", { name: "Detected installations" }));
    const items = detected.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Registered");
    await user.click(detected.getByRole("button", { name: `Add ${NEW_PATH}` }));

    expect(invokeMock).toHaveBeenCalledWith("add_installation", { path: NEW_PATH });
    await waitFor(() => {
      expect(detected.getAllByText("Registered")).toHaveLength(2);
    });
  });

  it("verifies and renames the selected installation", async () => {
    registry();
    const user = renderPage();
    await screen.findByRole("heading", { level: 2, name: "MAME 0.280" });

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
    expect(
      await screen.findByRole("heading", { level: 2, name: "Arcade ✓ (MAME 0.280)" }),
    ).toBeVisible();
  });

  it("removes an installation after confirmation, with the chosen files", async () => {
    registry();
    const user = renderPage();
    await screen.findByRole("heading", { level: 2, name: "MAME 0.280" });

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
    expect(registered().getAllByRole("listitem")).toHaveLength(1);
    expect(await screen.findByRole("heading", { level: 2, name: "MAME 0.252" })).toBeVisible();
  });

  it("does not remove an installation while MAME runs from it", async () => {
    registry({ sessions: [session(1, "pacman", { state: "running" }, null, NEW_ID)] });
    const user = renderPage();
    await screen.findByRole("heading", { level: 2, name: "MAME 0.280" });

    await user.click(screen.getByRole("button", { name: "Remove…" }));
    const confirm = within(screen.getByRole("group", { name: "Remove installation" }));
    expect(await confirm.findByText(/MAME is running from this installation/)).toBeInTheDocument();
    expect(confirm.getByRole("button", { name: "Remove installation" })).toBeDisabled();
  });

  it("shows how to start without installations", async () => {
    registry({ installations: [] });
    renderPage();
    expect(await screen.findByText(/No MAME installation is registered yet/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: /Catalog of/ })).not.toBeInTheDocument();
  });
});
