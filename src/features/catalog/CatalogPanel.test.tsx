import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogInfo, CatalogStatus, ImportStatus } from "./catalogApi";
import { CatalogPanel } from "./CatalogPanel";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const POLL_MS = 10;

const CATALOG: CatalogInfo = {
  schemaVersion: 1,
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
};

function status(overrides: Partial<CatalogStatus> = {}): CatalogStatus {
  return {
    catalog: null,
    catalogError: null,
    freshness: null,
    mameConfigured: true,
    import: { state: "idle" },
    ...overrides,
  };
}

function running(machinesProcessed: number, cancelRequested = false): ImportStatus {
  return {
    state: "running",
    phase: "reading",
    machinesProcessed,
    startedAt: 1,
    cancelRequested,
  };
}

type Handler = () => unknown;

/**
 * Routes mocked IPC calls by command name. Array handlers answer in sequence.
 * `latencyMs` delays every answer, like a real IPC round trip.
 */
function mockCommands(handlers: Record<string, Handler | unknown[]>, latencyMs = 0) {
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

function renderPanel() {
  const user = userEvent.setup();
  render(<CatalogPanel pollIntervalMs={POLL_MS} />);
  return user;
}

describe("CatalogPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("shows that no catalog exists and allows importing", async () => {
    mockCommands({ get_catalog_status: () => status() });
    renderPanel();

    expect(screen.getByText("Loading catalog…")).toBeInTheDocument();
    expect(await screen.findByText("No catalog has been imported yet.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import catalog" })).toBeEnabled();
  });

  it("shows the catalog summary", async () => {
    mockCommands({ get_catalog_status: () => status({ catalog: CATALOG, freshness: "current" }) });
    renderPanel();

    const summary = within(await screen.findByLabelText("Catalog summary"));
    expect(summary.getByText("0.280 (mame0280)")).toBeInTheDocument();
    expect(summary.getByText("48,893")).toBeInTheDocument();
    expect(summary.getByText("42,013")).toBeInTheDocument();
    expect(summary.getByText("364,836")).toBeInTheDocument();
    expect(summary.getByText("105.6 MB")).toBeInTheDocument();
    expect(summary.getByText("5.2 s")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reimport catalog" })).toBeEnabled();
  });

  it("warns when the catalog does not match the configured executable", async () => {
    mockCommands({
      get_catalog_status: () => status({ catalog: CATALOG, freshness: "executableChanged" }),
    });
    renderPanel();

    expect(
      await screen.findByText(/MAME executable has changed since this catalog was imported/),
    ).toBeInTheDocument();
  });

  it("disables importing when MAME is not configured", async () => {
    mockCommands({ get_catalog_status: () => status({ mameConfigured: false }) });
    renderPanel();

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
          { state: "completed", machines: 48893, durationMs: 5153, finishedAt: 2 },
        ],
      },
      POLL_MS * 3,
    );
    const user = renderPanel();

    await user.click(await findEnabledButton("Import catalog"));

    const progress = await screen.findByRole("progressbar", { name: "Import progress" });
    expect(progress).not.toHaveAttribute("value");
    expect(await screen.findByText(/12,000 machines processed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import catalog" })).toBeDisabled();

    expect(
      await screen.findByText("Catalog imported: 48,893 machines in 5.2 s."),
    ).toBeInTheDocument();
    expect(await screen.findByLabelText("Catalog summary")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel import" })).not.toBeInTheDocument();
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
      get_catalog_status: () => status({ import: { state: "cancelled", finishedAt: 1 } }),
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
            error: { kind: "mameFailed", exitCode: 3, stderr: "fatal error" },
            finishedAt: 1,
          },
        }),
    });
    renderPanel();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "Import failed: MAME exited with code 3 while listing machines.",
    );
    expect(alert).toHaveTextContent("fatal error");
    expect(screen.getByLabelText("Catalog summary")).toBeInTheDocument();
  });

  it("shows command errors when an import cannot start", async () => {
    mockCommands({ get_catalog_status: () => status() });
    invokeMock.mockResolvedValueOnce(status());
    // Tauri rejects with the serialized `CommandError`, not with an `Error`.
    invokeMock.mockRejectedValueOnce({ kind: "importInProgress" });
    const user = renderPanel();

    await user.click(await findEnabledButton("Import catalog"));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "A catalog import is already running.",
    );
  });

  it("reports an unreadable catalog", async () => {
    mockCommands({
      get_catalog_status: () =>
        status({ catalogError: "catalog schema version 9 is not supported (expected 1)" }),
    });
    renderPanel();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The catalog cannot be read: catalog schema version 9 is not supported",
    );
  });
});
