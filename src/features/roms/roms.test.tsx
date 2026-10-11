import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { useMemo } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { NEW_ID, OLD_ID, installationsView } from "../../test/installationFixtures";
import { launchConfig, launchConfigView, session } from "../../test/launcherFixtures";
import { FINGERPRINT, NAS_ID } from "../../test/remoteFixtures";
import { artifact, downloadPlan, jobStatus, transfer } from "../../test/romFixtures";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import {
  InstallationScope,
  useInstallationScope,
  useInstallations,
} from "../installations/installationsContext";
import type { LaunchRequest, SessionInfo } from "../launcher/launcherApi";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { useLauncher } from "../launcher/launcherContext";
import { MachineLaunchPanel } from "../launcher/MachineLaunchPanel";
import { RomDownloadSection } from "./RomDownloadSection";
import type { DownloadJobStatus, DownloadPlan } from "./romsApi";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

/** A stateful fake backend: installations, launcher and ROM manager. */
function backend(
  options: {
    plan?: (request: LaunchRequest, args: Args) => DownloadPlan;
    /** Statuses returned by successive status queries; the last one stays. */
    statuses?: DownloadJobStatus[];
    initial?: DownloadJobStatus | null;
    overrides?: Partial<Record<string, Handler>>;
  } = {},
) {
  let active = NEW_ID;
  let sessions: SessionInfo[] = [];
  let status: DownloadJobStatus | null = options.initial ?? null;
  const script = [...(options.statuses ?? [])];
  const handlers: Partial<Record<string, Handler>> = {
    list_installations: () => installationsView(undefined, active),
    set_active_installation: (args) => {
      active = args.installationId as string;
      return installationsView(undefined, active);
    },
    get_launch_config: (args) => launchConfigView(launchConfig(), args.installationId as string),
    get_launch_sessions: () => sessions,
    start_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      const started = session(sessions.length + 1, request.machine, { state: "running" }, null);
      sessions = [started, ...sessions];
      return started;
    },
    prepare_download_plan: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      return (options.plan ?? (() => downloadPlan({}, request.installationId)))(request, args);
    },
    start_download: () => {
      status = script.shift() ?? jobStatus();
      return status;
    },
    get_download_status: () => {
      if (status?.state === "running" && script.length > 0) status = script.shift() ?? status;
      return status;
    },
    cancel_download: () => {
      if (status) status = { ...status, cancelRequested: true };
      return status;
    },
    approve_remote_host_key: () => ({ connections: [], loadError: null }),
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
  return {
    addSession: (started: SessionInfo) => {
      sessions = [started, ...sessions];
    },
    queue: (...next: DownloadJobStatus[]) => {
      script.push(...next);
    },
  };
}

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

/** The ROM download section of `pacman` for the active installation. */
function Harness({ machine = "pacman" }: { machine?: string }) {
  const installationId = useInstallationScope();
  const { sessions } = useLauncher();
  const { select } = useInstallations();
  const request = useMemo(() => ({ installationId, machine }), [installationId, machine]);
  const target = useMemo(() => ({ machine, software: null }), [machine]);
  return (
    <>
      <RomDownloadSection request={request} target={target} pollIntervalMs={20} />
      <p>{sessions.length} session(s)</p>
      <button
        type="button"
        onClick={() => {
          select(installationId === NEW_ID ? OLD_ID : NEW_ID);
        }}
      >
        Switch installation
      </button>
    </>
  );
}

function ActiveScope({ children }: { children: React.ReactNode }) {
  const { activeId } = useInstallations();
  return (
    <LauncherProvider
      installationId={activeId}
      pollIntervalMs={20}
      showCommandPreview={false}
      onShowCommandPreviewChange={() => undefined}
    >
      {activeId !== null && <InstallationScope value={activeId}>{children}</InstallationScope>}
    </LauncherProvider>
  );
}

function renderSection(children: React.ReactNode = <Harness />) {
  const user = userEvent.setup();
  render(
    <InstallationsProvider>
      <ActiveScope>{children}</ActiveScope>
    </InstallationsProvider>,
  );
  return user;
}

async function preparePlan(user: ReturnType<typeof userEvent.setup>, button = "Prepare Download") {
  await user.click(await screen.findByRole("button", { name: button }));
  return screen.findByRole("region", { name: "Download plan" });
}

describe("ROM downloads", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("prepares a plan without downloading anything", async () => {
    backend();
    const user = renderSection();
    const plan = await preparePlan(user);

    expect(calls("prepare_download_plan")).toEqual([
      { request: { installationId: NEW_ID, machine: "pacman" }, passphrases: {} },
    ]);
    expect(calls("start_download")).toHaveLength(0);
    const summary = within(plan).getByRole("status");
    expect(summary).toHaveTextContent("1 present");
    expect(summary).toHaveTextContent("1 to download (2.0 KiB)");
    expect(summary).toHaveTextContent("1 not found");
    expect(summary).toHaveTextContent("1 warning");
    const rows = within(plan).getAllByRole("row");
    expect(rows[1]).toHaveTextContent(/pacman\.zip.*Machine ROMs.*Selected machine.*To download/);
    expect(rows[1]).toHaveTextContent("E:\\Downloads\\ROMs\\pacman.zip");
    expect(rows[2]).toHaveTextContent(/puckman\.zip.*Parent.*Present locally/);
    expect(rows[2]).toHaveTextContent("C:\\MAME\\roms\\puckman.zip");
    expect(rows[3]).toHaveTextContent(/pacman\/pacman\.chd.*Not found remotely/);
    expect(rows[3]).toHaveTextContent("was not found in the remote repository");
    expect(within(plan).getByText(/MAME decides whether the set can run/)).toBeInTheDocument();
    // Warnings do not prevent downloading the available files.
    expect(within(plan).getByRole("button", { name: "Download 1 file" })).toBeEnabled();
  });

  it("shows when there is nothing to download", async () => {
    backend({
      plan: () =>
        downloadPlan({
          artifacts: [artifact("pacman.zip", { availability: "present", download: false })],
          summary: {
            ...downloadPlan().summary,
            present: 1,
            toDownload: 0,
            notFound: 0,
            warnings: 0,
          },
        }),
    });
    const user = renderSection();
    const plan = await preparePlan(user);
    expect(within(plan).getByText("Nothing to download.")).toBeInTheDocument();
    expect(within(plan).getByRole("button", { name: "Download 0 files" })).toBeDisabled();
  });

  it("downloads with progress and reports a completion with warnings", async () => {
    const backendState = backend({
      statuses: [
        jobStatus(),
        jobStatus({
          transfers: [
            transfer("machineRoms:pacman.zip", { status: "downloading", bytesTransferred: 1024 }),
          ],
          bytesTransferred: 1024,
        }),
      ],
    });
    const user = renderSection();
    const plan = await preparePlan(user);
    await user.click(within(plan).getByRole("button", { name: "Download 1 file" }));

    expect(calls("start_download")).toEqual([{ planId: "plan-1", passphrases: {}, play: false }]);
    const progress = await screen.findByRole("region", { name: "Download progress" });
    await waitFor(() => {
      expect(progress).toHaveTextContent("file 1 of 1: pacman.zip (1.0 KiB of 2.0 KiB)");
    });
    expect(within(progress).getByText("1.0 KiB of 2.0 KiB (50%)")).toBeInTheDocument();

    backendState.queue(
      jobStatus({
        state: "completedWithWarnings",
        transfers: [
          transfer("machineRoms:pacman.zip", { status: "downloaded", bytesTransferred: 2048 }),
        ],
        bytesTransferred: 2048,
        finishedAt: 2,
      }),
    );
    const result = await screen.findByRole("region", { name: "Download result" });
    expect(result).toHaveTextContent("Download completed with warnings");
    expect(within(result).queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    // The plan shows each transfer.
    const rows = within(screen.getByRole("region", { name: "Download plan" })).getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Downloaded");
  });

  it("cancels a running download", async () => {
    const backendState = backend({ statuses: [jobStatus()] });
    const user = renderSection();
    const plan = await preparePlan(user);
    await user.click(within(plan).getByRole("button", { name: "Download 1 file" }));
    await user.click(await screen.findByRole("button", { name: "Cancel download" }));

    expect(calls("cancel_download")).toHaveLength(1);
    expect(await screen.findByText("Cancelling…")).toBeInTheDocument();
    backendState.queue(
      jobStatus({
        state: "cancelled",
        cancelRequested: true,
        transfers: [transfer("machineRoms:pacman.zip", { status: "cancelled" })],
        finishedAt: 2,
      }),
    );
    const result = await screen.findByRole("region", { name: "Download result" });
    expect(result).toHaveTextContent("Download cancelled. Completed files were kept");
    expect(within(result).getByRole("button", { name: "Retry" })).toBeEnabled();
  });

  it("reports failed transfers separately and retries them", async () => {
    backend({
      statuses: [
        jobStatus(),
        jobStatus({
          state: "failed",
          transfers: [
            transfer("machineRoms:pacman.zip", {
              status: "failed",
              error: { kind: "remote", error: { kind: "connectionLost" } },
            }),
          ],
          finishedAt: 2,
        }),
        jobStatus(),
      ],
    });
    const user = renderSection();
    const plan = await preparePlan(user);
    await user.click(within(plan).getByRole("button", { name: "Download 1 file" }));
    const result = await screen.findByRole("region", { name: "Download result" });
    expect(result).toHaveTextContent("Download failed");
    expect(result).toHaveTextContent("pacman.zip: The connection was interrupted.");

    await user.click(within(result).getByRole("button", { name: "Retry" }));
    expect(calls("start_download").at(-1)).toEqual({
      planId: "plan-1",
      passphrases: {},
      play: false,
    });
    expect(await screen.findByRole("region", { name: "Download progress" })).toBeInTheDocument();
  });

  it("Download & Play starts MAME through the backend once the files are downloaded", async () => {
    const backendState = backend({ statuses: [jobStatus({ play: true })] });
    const user = renderSection();
    const plan = await preparePlan(user, "Download & Play");
    await user.click(within(plan).getByRole("button", { name: "Download 1 & Play" }));
    expect(calls("start_download")).toEqual([{ planId: "plan-1", passphrases: {}, play: true }]);
    expect(screen.getByText("0 session(s)")).toBeInTheDocument();

    backendState.addSession(session(9, "pacman"));
    backendState.queue(
      jobStatus({
        play: true,
        state: "completed",
        transfers: [transfer("machineRoms:pacman.zip", { status: "downloaded" })],
        finishedAt: 2,
        launch: { status: "started", sessionId: 9 },
      }),
    );
    expect(
      await screen.findByText("MAME was started with the downloaded files."),
    ).toBeInTheDocument();
    // The sessions are read again to show the started one.
    expect(await screen.findByText("1 session(s)")).toBeInTheDocument();
    expect(calls("start_launch")).toHaveLength(0);
  });

  it("Download & Play does not start MAME after a failed transfer", async () => {
    backend({
      statuses: [
        jobStatus({
          play: true,
          state: "failed",
          transfers: [
            transfer("machineRoms:pacman.zip", {
              status: "failed",
              error: { kind: "sizeMismatch", expected: 2048, received: 10 },
            }),
          ],
          finishedAt: 2,
          launch: { status: "notStarted" },
        }),
      ],
    });
    const user = renderSection();
    const plan = await preparePlan(user, "Download & Play");
    await user.click(within(plan).getByRole("button", { name: "Download 1 & Play" }));
    const result = await screen.findByRole("region", { name: "Download result" });
    expect(result).toHaveTextContent("Received 10 bytes instead of 2,048");
    expect(result).toHaveTextContent("MAME was not started because the downloads did not complete");
    expect(within(result).getByRole("button", { name: "Retry" })).toBeEnabled();
    expect(calls("start_launch")).toHaveLength(0);
  });

  it("Download & Play plays directly when every file is present", async () => {
    backend({
      plan: (request) =>
        downloadPlan(
          {
            artifacts: [artifact("pacman.zip", { availability: "present", download: false })],
            summary: { ...downloadPlan().summary, present: 1, toDownload: 0, notFound: 0 },
          },
          request.installationId,
        ),
    });
    const user = renderSection();
    const plan = await preparePlan(user, "Download & Play");
    await user.click(within(plan).getByRole("button", { name: "Play" }));
    expect(calls("start_download")).toHaveLength(0);
    expect(calls("start_launch")).toEqual([
      { request: { installationId: NEW_ID, machine: "pacman" } },
    ]);
  });

  it("asks for a passphrase and checks the repository again", async () => {
    backend({
      plan: (request, args) =>
        (args.passphrases as Record<string, string>)[NAS_ID] === "secret"
          ? downloadPlan({}, request.installationId)
          : downloadPlan(
              {
                connectionIssues: [
                  {
                    connectionId: NAS_ID,
                    connectionName: "Home NAS",
                    error: { kind: "passphraseRequired" },
                  },
                ],
              },
              request.installationId,
            ),
    });
    const user = renderSection();
    const plan = await preparePlan(user);
    expect(within(plan).getByRole("alert")).toHaveTextContent(
      "Home NAS: The private key is protected by a passphrase.",
    );
    await user.type(within(plan).getByLabelText("Passphrase for Home NAS"), "secret");
    await user.click(within(plan).getByRole("button", { name: "Unlock and check again" }));
    await waitFor(() => {
      expect(calls("prepare_download_plan")).toHaveLength(2);
    });
    expect(calls("prepare_download_plan")[1]?.passphrases).toEqual({ [NAS_ID]: "secret" });
    await waitFor(() => {
      expect(screen.queryByText(/protected by a passphrase/)).not.toBeInTheDocument();
    });
    // The passphrase goes with the download too, and nowhere else.
    await user.click(screen.getByRole("button", { name: "Download 1 file" }));
    expect(calls("start_download")[0]?.passphrases).toEqual({ [NAS_ID]: "secret" });
  });

  it("approves an unknown server identity only on request", async () => {
    let approved = false;
    backend({
      plan: (request) =>
        approved
          ? downloadPlan({}, request.installationId)
          : downloadPlan(
              {
                connectionIssues: [
                  {
                    connectionId: NAS_ID,
                    connectionName: "Home NAS",
                    error: {
                      kind: "hostKeyUnknown",
                      host: "nas.local",
                      port: 22,
                      algorithm: "ssh-ed25519",
                      fingerprint: FINGERPRINT,
                      publicKey: "ssh-ed25519 AAAA",
                    },
                  },
                ],
              },
              request.installationId,
            ),
      overrides: {
        approve_remote_host_key: () => {
          approved = true;
          return { connections: [], loadError: null };
        },
      },
    });
    const user = renderSection();
    const plan = await preparePlan(user);
    expect(plan).toHaveTextContent(FINGERPRINT);
    expect(calls("approve_remote_host_key")).toHaveLength(0);
    await user.click(within(plan).getByRole("button", { name: "Approve and check again" }));
    expect(calls("approve_remote_host_key")).toEqual([
      { connectionId: NAS_ID, fingerprint: FINGERPRINT },
    ]);
    await waitFor(() => {
      expect(screen.queryByText(FINGERPRINT)).not.toBeInTheDocument();
    });
  });

  it("never shows a plan of another installation and keeps the download running", async () => {
    backend({ statuses: [jobStatus()] });
    const user = renderSection();
    const plan = await preparePlan(user);
    await user.click(within(plan).getByRole("button", { name: "Download 1 file" }));
    expect(await screen.findByRole("region", { name: "Download progress" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Switch installation" }));
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Download progress" })).not.toBeInTheDocument();
    });
    expect(screen.queryByRole("region", { name: "Download plan" })).not.toBeInTheDocument();
    expect(
      screen.getByText("Another download is running: Pac-Man (Midway) (MAME 0.280)."),
    ).toBeInTheDocument();
    // Nothing was cancelled, and no new download can start meanwhile.
    expect(calls("cancel_download")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Prepare Download" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Switch installation" }));
    expect(await screen.findByRole("region", { name: "Download progress" })).toBeInTheDocument();
  });

  it("shows a download that started before the page was opened", async () => {
    backend({
      initial: jobStatus({
        transfers: [
          transfer("machineRoms:pacman.zip", { status: "downloading", bytesTransferred: 512 }),
        ],
        bytesTransferred: 512,
      }),
    });
    renderSection();
    const progress = await screen.findByRole("region", { name: "Download progress" });
    expect(progress).toHaveTextContent("pacman.zip (512 B of 2.0 KiB)");
    expect(calls("prepare_download_plan")).toHaveLength(0);
  });

  it("keeps the normal Play button independent of the ROM manager", async () => {
    backend({ overrides: { get_download_status: () => Promise.reject(new Error("unavailable")) } });
    const user = userEvent.setup();
    render(
      <InstallationsProvider>
        <ActiveScope>
          <MachineLaunchPanel name="pacman" isRunnable />
        </ActiveScope>
      </InstallationsProvider>,
    );
    const play = await screen.findByRole("button", { name: "Play" });
    await waitFor(() => {
      expect(play).toBeEnabled();
    });
    expect(screen.getByRole("button", { name: "Prepare Download" })).toBeEnabled();
    await user.click(play);
    expect(calls("start_launch")).toEqual([
      { request: { installationId: NEW_ID, machine: "pacman" } },
    ]);
    expect(calls("prepare_download_plan")).toHaveLength(0);
  });
});

// Stylesheets are read from disk (relative to the project root, where
// Vitest runs): Vitest does not process CSS imports.
const romsCss = readFileSync("src/features/roms/roms.css", "utf8");
const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

describe("ROM download styles", () => {
  it("use only theme tokens defined by the light and dark themes", () => {
    expect(romsCss).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
    const tokens = new Set(romsCss.match(/--color-[a-z-]+/g));
    expect(tokens.size).toBeGreaterThan(0);
    for (const theme of ["light", "dark"]) {
      const block = new RegExp(`:root\\[data-theme="${theme}"\\][^{]*\\{([^}]*)\\}`).exec(
        themesCss,
      )?.[1];
      expect(block, theme).toBeDefined();
      for (const token of tokens) {
        expect(block, `${theme} ${token}`).toContain(`${token}:`);
      }
    }
  });
});
