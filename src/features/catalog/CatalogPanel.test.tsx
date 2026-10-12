import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CatalogInfo,
  CatalogStatus,
  ImportKind,
  ImportStatus,
  SoftwareCatalogInfo,
} from "./catalogApi";
import { CatalogPanel } from "./CatalogPanel";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import type { InstallationSummary } from "../installations/installationsApi";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const POLL_MS = 10;

const CATALOG: CatalogInfo = {
  schemaVersion: 2,
  mameRelease: "0.280",
  mameRevision: "mame0280",
  mameBuild: "0.280 (mame0280)",
  executablePath: "C:\\MAME\\mame.exe",
  importedAt: Date.UTC(2026, 9, 9, 12, 0, 0),
  importDurationMs: 5153,
  databaseSize: 110_739_456,
  statistics: {
    machines: 48893,
    devices: 6802,
    bios: 78,
    mechanical: 15683,
    clones: 27320,
    runnableSystems: 42013,
    roms: 364836,
    disks: 1378,
    softwareLists: 733,
  },
  software: null,
};

const SOFTWARE: SoftwareCatalogInfo = {
  mameRelease: "0.280",
  mameRevision: "mame0280",
  importedAt: Date.UTC(2026, 9, 9, 12, 5, 0),
  importDurationMs: 68150,
  matchesMachineCatalog: true,
  statistics: {
    lists: 720,
    software: 139122,
    clones: 43533,
    partiallySupported: 2005,
    unsupported: 39054,
    parts: 236041,
    dataAreas: 236580,
    roms: 235190,
    diskAreas: 11386,
    disks: 11374,
    missingReferencedLists: 13,
  },
};

const WITH_SOFTWARE: CatalogInfo = { ...CATALOG, software: SOFTWARE };

function status(overrides: Partial<CatalogStatus> = {}): CatalogStatus {
  return {
    installationId: NEW_ID,
    catalog: null,
    catalogError: null,
    freshness: null,
    import: { state: "idle" },
    ...overrides,
  };
}

function running(
  itemsProcessed: number,
  cancelRequested = false,
  kind: ImportKind = "machines",
  installationId = NEW_ID,
): ImportStatus {
  return {
    state: "running",
    installationId,
    kind,
    phase: "reading",
    itemsProcessed,
    startedAt: 1,
    cancelRequested,
  };
}

type Handler = () => unknown;

/**
 * Routes mocked IPC calls by command name. Array handlers answer in sequence.
 * `latencyMs` delays every answer, like a real IPC round trip.
 */
function mockCommands(commandHandlers: Record<string, Handler | unknown[]>, latencyMs = 0) {
  const handlers: Record<string, Handler | unknown[]> = {
    list_installations: () => installationsView(),
    ...commandHandlers,
  };
  const queues = new Map<string, unknown[]>();
  const respond = (value: unknown) =>
    new Promise((resolve) => {
      setTimeout(() => {
        resolve(value);
      }, latencyMs);
    });
  invokeMock.mockImplementation((command: string) => {
    const handler = handlers[command];
    if (handler === undefined) {
      return Promise.reject(new Error(`unexpected command ${command}`));
    }
    if (Array.isArray(handler)) {
      const queue = queues.get(command) ?? [...handler];
      queues.set(command, queue);
      return respond(queue.length > 1 ? queue.shift() : queue[0]);
    }
    return respond(handler());
  });
}

/** Waits until the button exists and is enabled (the status has loaded). */
async function findEnabledButton(name: string) {
  const button = await screen.findByRole("button", { name });
  await waitFor(() => {
    expect(button).toBeEnabled();
  });
  return button;
}

function renderPanel(
  onCatalogChanged?: () => void,
  shown: InstallationSummary = installation(NEW_ID, "0.280"),
) {
  const user = userEvent.setup();
  render(
    <InstallationsProvider>
      <CatalogPanel
        installation={shown}
        pollIntervalMs={POLL_MS}
        onCatalogChanged={onCatalogChanged}
      />
    </InstallationsProvider>,
  );
  return user;
}

describe("CatalogPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  describe("machine catalog", () => {
    it("shows that no catalog exists and allows importing", async () => {
      mockCommands({ get_catalog_status: () => status() });
      renderPanel();

      expect(screen.getByText("Loading catalog…")).toBeInTheDocument();
      expect(await screen.findByText("No catalog has been imported yet.")).toBeInTheDocument();
      expect(screen.getByText("Import the machine catalog first.")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Import catalog" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Import software lists" })).toBeDisabled();
    });

    it("shows the catalog summary", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: CATALOG, freshness: "current" }),
      });
      renderPanel();

      const summary = within(await screen.findByLabelText("Catalog summary"));
      expect(summary.getByText("0.280 (mame0280)")).toBeInTheDocument();
      expect(summary.getByText("48,893")).toBeInTheDocument();
      expect(summary.getByText("42,013")).toBeInTheDocument();
      expect(summary.getByText("364,836")).toBeInTheDocument();
      expect(summary.getByText("105.6 MB")).toBeInTheDocument();
      expect(summary.getByText("5.2 s")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Reimport machines" })).toBeEnabled();
    });

    it("warns when the catalog does not match the installation executable", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: CATALOG, freshness: "executableChanged" }),
      });
      renderPanel();

      expect(
        await screen.findByText(/MAME executable has changed since this catalog was imported/),
      ).toBeInTheDocument();
    });

    it("disables importing when the executable no longer exists", async () => {
      mockCommands({ get_catalog_status: () => status() });
      renderPanel(undefined, installation(NEW_ID, "0.280", { executableStatus: "missing" }));

      expect(await screen.findByRole("button", { name: "Import catalog" })).toBeDisabled();
    });

    it("shows indeterminate progress while importing, then the result", async () => {
      // IPC latency longer than the polling interval, as in the real application.
      mockCommands(
        {
          get_catalog_status: [status(), status({ catalog: CATALOG, freshness: "current" })],
          start_catalog_import: () => running(0),
          get_catalog_import_status: [
            running(12000),
            {
              state: "completed",
              installationId: NEW_ID,
              kind: "machines",
              items: 48893,
              durationMs: 5153,
              finishedAt: 2,
            },
          ],
        },
        POLL_MS * 3,
      );
      const onCatalogChanged = vi.fn();
      const user = renderPanel(onCatalogChanged);

      await user.click(await findEnabledButton("Import catalog"));
      expect(onCatalogChanged).not.toHaveBeenCalled();

      const progress = await screen.findByRole("progressbar", { name: "Import progress" });
      expect(progress).not.toHaveAttribute("value");
      expect(await screen.findByText(/12,000 machines processed/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Import catalog" })).toBeDisabled();

      expect(
        await screen.findByText("Machine catalog imported: 48,893 machines in 5.2 s."),
      ).toBeInTheDocument();
      expect(await screen.findByLabelText("Catalog summary")).toBeInTheDocument();
      expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Cancel import" })).not.toBeInTheDocument();
      expect(onCatalogChanged).toHaveBeenCalledTimes(1);
    });

    it("cancels a running import", async () => {
      // The backend reports the cancellation request only after it was made.
      let cancelRequested = false;
      mockCommands({
        get_catalog_status: () => status({ import: running(100) }),
        get_catalog_import_status: () => running(200, cancelRequested),
        cancel_catalog_import: () => {
          cancelRequested = true;
          return running(150, true);
        },
      });
      const user = renderPanel();

      await user.click(await findEnabledButton("Cancel import"));

      expect(invokeMock).toHaveBeenCalledWith("cancel_catalog_import", undefined);
      expect(await screen.findByRole("button", { name: "Cancelling…" })).toBeDisabled();
    });

    it("reports a cancelled import", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({
            import: { state: "cancelled", installationId: NEW_ID, kind: "machines", finishedAt: 1 },
          }),
      });
      renderPanel();

      expect(
        await screen.findByText("Import cancelled. The previous catalog, if any, was kept."),
      ).toBeInTheDocument();
    });

    it("reports import failures", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({
            catalog: CATALOG,
            freshness: "current",
            import: {
              state: "failed",
              installationId: NEW_ID,
              kind: "machines",
              error: { kind: "mameFailed", exitCode: 3, stderr: "fatal error" },
              finishedAt: 1,
            },
          }),
      });
      renderPanel();

      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent("Import failed: MAME exited with code 3.");
      expect(alert).toHaveTextContent("fatal error");
      expect(screen.getByLabelText("Catalog summary")).toBeInTheDocument();
    });

    it("shows command errors when an import cannot start", async () => {
      mockCommands({ get_catalog_status: () => status() });
      const user = renderPanel();
      const button = await findEnabledButton("Import catalog");
      await screen.findByRole("region", { name: "Catalog of MAME 0.280" });
      // Tauri rejects with the serialized `CommandError`, not with an `Error`.
      invokeMock.mockRejectedValueOnce({ kind: "importInProgress" });

      await user.click(button);

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "A catalog import is already running.",
      );
    });

    it("reports an unreadable catalog", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({ catalogError: "catalog schema version 9 is not supported (expected 2)" }),
      });
      renderPanel();

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "The catalog cannot be read: catalog schema version 9 is not supported",
      );
    });
  });

  describe("software lists", () => {
    it("shows that software lists are not imported for the catalog", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: CATALOG, freshness: "current" }),
      });
      renderPanel();

      expect(
        await screen.findByText("Software lists have not been imported for this catalog."),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Import software lists" })).toBeEnabled();
    });

    it("shows the software list summary", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: WITH_SOFTWARE, freshness: "current" }),
      });
      renderPanel();

      const summary = within(await screen.findByLabelText("Software list summary"));
      expect(summary.getByText("720")).toBeInTheDocument();
      expect(summary.getByText("139,122")).toBeInTheDocument();
      expect(summary.getByText("236,041")).toBeInTheDocument();
      expect(summary.getByText("13")).toBeInTheDocument();
      expect(summary.getByText("68.2 s")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Update software lists" })).toBeEnabled();
      expect(screen.getByText(/import the software lists again afterwards/)).toBeInTheDocument();
    });

    it("only allows importing software lists from the catalog's executable", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: CATALOG, freshness: "differentExecutable" }),
      });
      renderPanel();

      // Wait for the status: the buttons are disabled while it loads.
      expect(await findEnabledButton("Reimport machines")).toBeEnabled();
      expect(screen.getByRole("button", { name: "Import software lists" })).toBeDisabled();
    });

    it("warns about software lists from another MAME build", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({
            catalog: { ...CATALOG, software: { ...SOFTWARE, matchesMachineCatalog: false } },
            freshness: "current",
          }),
      });
      renderPanel();

      expect(
        await screen.findByText(/do not come from the MAME build of the machine catalog/),
      ).toBeInTheDocument();
    });

    it("imports software lists with progress and refreshes the summary", async () => {
      mockCommands(
        {
          get_catalog_status: [
            status({ catalog: CATALOG, freshness: "current" }),
            status({ catalog: WITH_SOFTWARE, freshness: "current" }),
          ],
          start_software_list_import: () => running(0, false, "softwareLists"),
          get_catalog_import_status: [
            running(50000, false, "softwareLists"),
            {
              state: "completed",
              installationId: NEW_ID,
              kind: "softwareLists",
              items: 139122,
              durationMs: 68150,
              finishedAt: 2,
            },
          ],
        },
        POLL_MS * 3,
      );
      const user = renderPanel();

      await user.click(await findEnabledButton("Import software lists"));

      expect(
        await screen.findByRole("progressbar", { name: "Import progress" }),
      ).not.toHaveAttribute("value");
      expect(
        await screen.findByText("Reading the software lists… 50,000 software processed."),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Reimport machines" })).toBeDisabled();
      expect(
        await screen.findByText("Software lists imported: 139,122 software in 68.2 s."),
      ).toBeInTheDocument();
      expect(await screen.findByLabelText("Software list summary")).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("start_software_list_import", {
        installationId: NEW_ID,
      });
    });

    it("cancels a running software list import", async () => {
      let cancelRequested = false;
      mockCommands({
        get_catalog_status: () =>
          status({
            catalog: CATALOG,
            freshness: "current",
            import: running(10, false, "softwareLists"),
          }),
        get_catalog_import_status: () => running(20, cancelRequested, "softwareLists"),
        cancel_catalog_import: () => {
          cancelRequested = true;
          return running(15, true, "softwareLists");
        },
      });
      const user = renderPanel();

      await user.click(await findEnabledButton("Cancel import"));

      expect(await screen.findByRole("button", { name: "Cancelling…" })).toBeDisabled();
      expect(screen.getByText(/software processed/)).toBeInTheDocument();
    });

    it("explains a version mismatch error", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({
            catalog: CATALOG,
            freshness: "current",
            import: {
              state: "failed",
              installationId: NEW_ID,
              kind: "softwareLists",
              error: {
                kind: "catalogMismatch",
                catalogRelease: "0.279",
                catalogRevision: "mame0279",
                executableRelease: "0.280",
                executableRevision: "mame0280",
              },
              finishedAt: 1,
            },
          }),
      });
      renderPanel();

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "The machine catalog comes from MAME 0.279 (mame0279) but the executable is MAME 0.280 (mame0280). Reimport the machine catalog first.",
      );
    });
  });

  describe("installations", () => {
    it("reads and imports the catalog of its installation", async () => {
      mockCommands({
        get_catalog_status: () => status({ catalog: CATALOG }),
        start_catalog_import: () => running(0, false, "machines", OLD_ID),
      });
      const user = renderPanel(undefined, installation(OLD_ID, "0.252"));

      expect(
        await screen.findByRole("region", { name: "Catalog of MAME 0.252" }),
      ).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("get_catalog_status", { installationId: OLD_ID });
      await user.click(await findEnabledButton("Reimport machines"));
      expect(invokeMock).toHaveBeenCalledWith("start_catalog_import", { installationId: OLD_ID });
    });

    it("shows that another installation is importing and waits for it", async () => {
      mockCommands({
        get_catalog_status: () =>
          status({ catalog: CATALOG, import: running(1200, false, "machines", OLD_ID) }),
        get_catalog_import_status: () => running(1300, false, "machines", OLD_ID),
      });
      renderPanel();

      expect(
        await screen.findByText(
          /An import of MAME 0.252 is running. Only one import runs at a time./,
        ),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Reimport machines" })).toBeDisabled();
      expect(screen.queryByRole("button", { name: "Cancel import" })).not.toBeInTheDocument();
      expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    });
  });
});
