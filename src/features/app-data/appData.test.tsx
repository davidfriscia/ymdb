import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PREFERENCES_KEY } from "../../app/preferences";
import { ApplicationDataSettings } from "./ApplicationDataSettings";
import type { ApplicationDataView, BackupJobStatus, RestorePreview } from "./appDataApi";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

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

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

const VIEW: ApplicationDataView = {
  configDir: "C:\\Users\\me\\AppData\\Roaming\\io.github.davidfriscia.ymdb",
  dataDir: "C:\\Users\\me\\AppData\\Local\\io.github.davidfriscia.ymdb",
  customDataDir: false,
  entries: [
    {
      id: "lists",
      name: "Favorites and lists",
      description: "Favorites and user lists, by MAME machine and software names.",
      path: "C:\\Users\\me\\AppData\\Roaming\\io.github.davidfriscia.ymdb\\lists.json",
      kind: "file",
      class: "owned",
      exists: true,
      size: 2048,
      files: null,
      inBackup: true,
      exclusionReason: null,
    },
    {
      id: "launchLogs",
      name: "Launch logs",
      description: "Output of MAME sessions started by YMDB.",
      path: "C:\\Users\\me\\AppData\\Local\\io.github.davidfriscia.ymdb\\launch-logs",
      kind: "directory",
      class: "transient",
      exists: false,
      size: null,
      files: null,
      inBackup: false,
      exclusionReason: "Diagnostic logs of past sessions.",
    },
  ],
  externalReferences: [
    { kind: "mameExecutable", owner: "MAME 0.280", path: "D:\\MAME\\mame.exe", exists: true },
    { kind: "mediaDirectory", owner: "snap", path: "\\\\nas\\snap", exists: false },
  ],
  restorePending: false,
};

const PREVIEW: RestorePreview = {
  archivePath: "D:\\Backups\\ymdb-20261024-1305.ymdb-backup.zip",
  formatVersion: 1,
  appVersion: "0.1.0",
  createdAt: Date.UTC(2026, 9, 24, 13, 5),
  platform: { os: "windows", arch: "x86_64" },
  samePlatform: true,
  totalSize: 5 * 1024 * 1024,
  categories: [
    { category: "catalogs", label: "Catalogs", files: 2, size: 5 * 1024 * 1024 - 10 },
    { category: "lists", label: "Favorites and lists", files: 1, size: 10 },
  ],
  replaced: [{ category: "lists", label: "Favorites and lists", files: 1, size: 12 }],
  externalReferences: [
    { kind: "mediaDirectory", owner: "snap", path: "\\\\nas\\snap", exists: false },
  ],
  notes: [],
};

function renderSettings() {
  const user = userEvent.setup();
  render(<ApplicationDataSettings restoreOutcome={null} />);
  return user;
}

describe("ApplicationDataSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    localStorage.clear();
  });

  it("shows the real locations, their state and whether they are backed up", async () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({ state: "idle" }),
    });
    renderSettings();

    expect(await screen.findByText(VIEW.configDir)).toBeVisible();
    expect(screen.getByText(VIEW.dataDir)).toBeVisible();
    const lists = screen.getByText("Favorites and lists").closest("li");
    expect(lists).not.toBeNull();
    const entry = within(lists as HTMLElement);
    expect(entry.getByText(/lists\.json$/)).toBeVisible();
    expect(entry.getByText("File · 2.0 KB")).toBeVisible();
    expect(entry.getByText("In backup")).toBeVisible();
    const logs = within(screen.getByText("Launch logs").closest("li") as HTMLElement);
    // Created on first use: not an error.
    expect(logs.getByText("Directory · Not created yet")).toBeVisible();
    expect(logs.getByText("Not in backup")).toBeVisible();
    expect(logs.getByText("Diagnostic logs of past sessions.")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    // External references.
    expect(screen.getByText("D:\\MAME\\mame.exe")).toBeVisible();
    expect(screen.getByText("Not found")).toBeVisible();
  });

  it("opens and copies locations by entry id", async () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({ state: "idle" }),
      open_application_data_location: () => null,
    });
    const user = renderSettings();
    const lists = within(
      (await screen.findByText("Favorites and lists")).closest("li") as HTMLElement,
    );

    await user.click(lists.getByRole("button", { name: "Open location" }));
    await user.click(lists.getByRole("button", { name: "Copy path" }));

    expect(calls("open_application_data_location")).toEqual([{ entryId: "lists" }]);
    // The clipboard of user-event.
    expect(await navigator.clipboard.readText()).toBe(VIEW.entries[0]?.path);
    expect(await screen.findByText(/^Copied /)).toBeVisible();
  });

  it("exports with the interface preferences and reports the archive", async () => {
    localStorage.setItem(PREFERENCES_KEY, '{"theme":"dark"}');
    let status: BackupJobStatus = { state: "idle" };
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => status,
      start_backup_export: () => {
        status = {
          state: "running",
          jobId: 1,
          kind: "export",
          phase: "writing",
          bytesDone: 1024,
          bytesTotal: 4096,
          cancelRequested: false,
        };
        return true;
      },
    });
    const user = renderSettings();
    await screen.findByText(VIEW.configDir);

    await user.click(screen.getByRole("button", { name: "Export backup…" }));

    expect(calls("start_backup_export")).toEqual([
      { interfacePreferences: { [PREFERENCES_KEY]: '{"theme":"dark"}' } },
    ]);
    expect(await screen.findByText("Writing the archive… 1.0 KB of 4.0 KB.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Export backup…" })).toBeDisabled();

    status = { state: "exported", jobId: 1, path: "D:\\Backups\\b.ymdb-backup.zip", size: 4096 };
    expect(await screen.findByText("D:\\Backups\\b.ymdb-backup.zip")).toBeVisible();
    expect(screen.getByText("Backup exported (4.0 KB) to:")).toBeVisible();
    // The sizes are measured again after the export.
    await waitFor(() => {
      expect(calls("get_application_data").length).toBeGreaterThanOrEqual(2);
    });
  });

  it("reports a failed export without claiming an archive", async () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({
        state: "failed",
        jobId: 4,
        kind: "export",
        message: "1 MAME session(s) running",
      }),
      discard_backup_job: () => null,
    });
    const user = renderSettings();

    expect(await screen.findByText("Export failed: 1 MAME session(s) running")).toBeVisible();
    expect(
      screen.getByText("No archive was written and the application data was not changed."),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(calls("discard_backup_job")).toHaveLength(1);
  });

  it("previews a backup and restores it only after confirmation", async () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({ state: "prepared", jobId: 9, preview: PREVIEW }),
      // YMDB restarts: the call never answers.
      apply_backup_import: () => new Promise(() => undefined),
    });
    const user = renderSettings();

    const confirmation = within(await screen.findByRole("group", { name: "Restore backup" }));
    expect(confirmation.getByText(PREVIEW.archivePath)).toBeVisible();
    expect(confirmation.getByText("Catalogs: 2 files, 5.0 MB")).toBeVisible();
    expect(confirmation.getByText("Favorites and lists: 1 file, 12 B")).toBeVisible();
    expect(confirmation.getByText("\\\\nas\\snap")).toBeVisible();
    expect(screen.getByRole("button", { name: "Import backup…" })).toBeDisabled();
    expect(calls("apply_backup_import")).toHaveLength(0);

    await user.click(confirmation.getByRole("button", { name: "Replace data and restart" }));

    expect(calls("apply_backup_import")).toEqual([{ jobId: 9 }]);
    expect(confirmation.getByRole("button", { name: "Restarting…" })).toBeDisabled();
  });

  it("cancels a previewed backup", async () => {
    let status: BackupJobStatus = { state: "prepared", jobId: 9, preview: PREVIEW };
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => status,
      discard_backup_job: () => {
        status = { state: "idle" };
        return null;
      },
    });
    const user = renderSettings();
    const confirmation = within(await screen.findByRole("group", { name: "Restore backup" }));

    await user.click(confirmation.getByRole("button", { name: "Cancel" }));

    expect(calls("discard_backup_job")).toHaveLength(1);
    await waitFor(() => {
      expect(screen.queryByRole("group", { name: "Restore backup" })).not.toBeInTheDocument();
    });
    expect(calls("apply_backup_import")).toHaveLength(0);
  });

  it("shows a refused restore and keeps the preview", async () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({ state: "prepared", jobId: 9, preview: PREVIEW }),
      apply_backup_import: () => {
        throw Object.assign(new Error("refused"), {
          kind: "refused",
          message: "1 MAME session(s) running",
        });
      },
    });
    const user = renderSettings();
    const confirmation = within(await screen.findByRole("group", { name: "Restore backup" }));

    await user.click(confirmation.getByRole("button", { name: "Replace data and restart" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("1 MAME session(s) running");
    expect(confirmation.getByRole("button", { name: "Replace data and restart" })).toBeEnabled();
  });

  it("reports the outcome of a failed restore", () => {
    backend({
      get_application_data: () => VIEW,
      get_backup_status: () => ({ state: "idle" }),
    });
    render(
      <ApplicationDataSettings
        restoreOutcome={{
          kind: "failed",
          message: "a file is in use. The previous data was put back",
          previousDataKept: true,
        }}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "The restore failed: a file is in use. The previous data was put back",
    );
  });
});
