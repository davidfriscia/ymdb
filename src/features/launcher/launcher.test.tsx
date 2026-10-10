import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { launchConfig, launchConfigView, session } from "../../test/launcherFixtures";
import { installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { InstallationsProvider } from "../installations/InstallationsProvider";
import { InstallationScope, useInstallations } from "../installations/installationsContext";
import type {
  LaunchConfig,
  LaunchRequest,
  MachineCandidate,
  SessionInfo,
  SoftwareLaunchInfo,
} from "./launcherApi";
import { LauncherProvider } from "./LauncherProvider";
import { LaunchSettings } from "./LaunchSettings";
import { MachineLaunchPanel } from "./MachineLaunchPanel";
import { SoftwareLaunchPanel } from "./SoftwareLaunchPanel";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

function backend(commandHandlers: Partial<Record<string, Handler>>) {
  let active = NEW_ID;
  const handlers: Partial<Record<string, Handler>> = {
    list_installations: () => installationsView(undefined, active),
    set_active_installation: (args) => {
      active = args.installationId as string;
      return installationsView(undefined, active);
    },
    ...commandHandlers,
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

/** Rejection with a Tauri `CommandError` payload. */
function commandError(kind: string, fields: Record<string, unknown> = {}): Error {
  return Object.assign(new Error(kind), { kind, ...fields });
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);
const lastRequest = (command: string) =>
  (calls(command).at(-1) as { request: LaunchRequest } | undefined)?.request;

/** A stateful fake launcher backend. */
function launcherBackend(
  options: {
    config?: LaunchConfig;
    info?: SoftwareLaunchInfo | null;
    overrides?: Partial<Record<string, Handler>>;
  } = {},
) {
  // Every installation has its own configuration.
  const configs = new Map<string, LaunchConfig>([[NEW_ID, options.config ?? launchConfig()]]);
  const configOf = (id: string) => configs.get(id) ?? launchConfig();
  let sessions: SessionInfo[] = [];
  backend({
    get_launch_config: (args) => {
      const id = args.installationId as string;
      return launchConfigView(configOf(id), id);
    },
    save_launch_config: (args) => {
      const id = args.installationId as string;
      configs.set(id, args.config as LaunchConfig);
      return launchConfigView(configOf(id), id);
    },
    get_launch_sessions: () => sessions,
    get_software_launch_info: () => options.info ?? null,
    preview_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      return {
        plan: {
          executable: "C:\\Émulateurs MAME ✓\\mame.exe",
          workingDirectory: "C:\\Émulateurs MAME ✓",
          arguments: [
            { role: "machine", tokens: [request.machine] },
            ...(request.software?.media ?? []).map((media) => ({
              role: "media",
              device: media.device,
              part: media.part,
              tokens: [
                `-${media.device}`,
                `${request.software?.list ?? ""}:${request.software?.name ?? ""}:${media.part}`,
              ],
            })),
            { role: "option", name: "window", source: "global", tokens: ["-window"] },
          ],
        },
        commandLine: [request.machine, "-window"],
        warnings: request.software ? ["The software may not work fully."] : [],
      };
    },
    start_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      const started = session(
        sessions.length + 1,
        request.machine,
        { state: "running" },
        request.software ? { list: request.software.list, name: request.software.name } : null,
        request.installationId,
      );
      sessions = [started, ...sessions];
      return started;
    },
    stop_launch_session: (args) => {
      sessions = sessions.map((entry) =>
        entry.id === args.id ? { ...entry, state: { state: "stopped", finishedAt: 1 } } : entry,
      );
      return sessions.find((entry) => entry.id === args.id);
    },
    ...options.overrides,
  });
  return {
    config: () => configOf(NEW_ID),
    configOf,
    setSessions: (next: SessionInfo[]) => {
      sessions = next;
    },
  };
}

interface ProvidersProps {
  children: ReactNode;
  showCommandPreview: boolean;
  onShowCommandPreviewChange: (show: boolean) => void;
}

/** The launcher of the active installation; launch panels browse that installation. */
function ActiveLauncher({ children, ...props }: ProvidersProps) {
  const { activeId } = useInstallations();
  return (
    <LauncherProvider installationId={activeId} pollIntervalMs={20} {...props}>
      {activeId !== null && <InstallationScope value={activeId}>{children}</InstallationScope>}
    </LauncherProvider>
  );
}

function tree(
  children: ReactNode,
  showCommandPreview: boolean,
  onShowCommandPreviewChange: (show: boolean) => void,
) {
  return (
    <InstallationsProvider>
      <ActiveLauncher
        showCommandPreview={showCommandPreview}
        onShowCommandPreviewChange={onShowCommandPreviewChange}
      >
        {children}
      </ActiveLauncher>
    </InstallationsProvider>
  );
}

function renderWithLauncher(children: ReactNode, showCommandPreview = false) {
  const user = userEvent.setup();
  const onShowCommandPreviewChange = vi.fn();
  const view = render(tree(children, showCommandPreview, onShowCommandPreviewChange));
  return { user, onShowCommandPreviewChange, view };
}

async function findPlay() {
  const play = await screen.findByRole("button", { name: "Play" });
  await waitFor(() => {
    expect(play).toBeEnabled();
  });
  return play;
}

function candidate(name: string, overrides: Partial<MachineCandidate> = {}): MachineCandidate {
  return {
    name,
    description: `${name} description`,
    year: "1983",
    manufacturer: "Apple Computer",
    cloneOf: null,
    relation: "original",
    filters: [],
    filterMatch: true,
    devices: [
      {
        briefName: "flop1",
        instanceName: "floppydisk1",
        deviceType: "floppydisk",
        interfaces: ["floppy_5_25"],
      },
      {
        briefName: "flop2",
        instanceName: "floppydisk2",
        deviceType: "floppydisk",
        interfaces: ["floppy_5_25"],
      },
    ],
    defaultMedia: [
      { device: "flop1", part: "flop1" },
      { device: "flop2", part: "flop2" },
    ],
    launchable: true,
    ...overrides,
  };
}

function softwareInfo(overrides: Partial<SoftwareLaunchInfo> = {}): SoftwareLaunchInfo {
  return {
    listName: "apple2_flop_orig",
    name: "ultima4",
    description: "Ultima IV",
    parts: [
      {
        name: "flop1",
        interface: "floppy_5_25",
        label: "Side A - Program Disk",
        compatibility: "A2E",
      },
      {
        name: "flop2",
        interface: "floppy_5_25",
        label: "Side B - Towne Disk",
        compatibility: "A2E",
      },
    ],
    candidates: [
      candidate("apple2e"),
      candidate("apple2", { relation: "compatible", filters: ["A2"], filterMatch: false }),
      candidate("apple2tape", { devices: [], defaultMedia: [], launchable: false }),
    ],
    recommended: null,
    ...overrides,
  };
}

describe("launcher", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  describe("machine launch", () => {
    it("starts a machine and shows the running session", async () => {
      launcherBackend();
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);

      await user.click(await findPlay());

      expect(lastRequest("start_launch")).toEqual({ installationId: NEW_ID, machine: "pacman" });
      const status = await screen.findByRole("status", { name: "Session" });
      expect(status).toHaveTextContent("Running");
      expect(screen.getByRole("button", { name: "Running" })).toBeDisabled();
    });

    it("shows the starting state and launch errors", async () => {
      const pending = deferred<unknown>();
      launcherBackend({
        overrides: {
          start_launch: () =>
            pending.promise.then(() => {
              throw commandError("catalogMismatch", { freshness: "differentExecutable" });
            }),
        },
      });
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);

      await user.click(await findPlay());
      expect(screen.getByRole("button", { name: "Starting…" })).toBeDisabled();

      pending.resolve(null);
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "The catalog does not match the MAME executable of this installation",
      );
      expect(screen.getByRole("button", { name: "Play" })).toBeEnabled();
    });

    it("reports a session that is already running", async () => {
      launcherBackend({
        overrides: {
          start_launch: () => {
            throw commandError("alreadyRunning", { sessionId: 4 });
          },
        },
      });
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);
      await user.click(await findPlay());
      expect(await screen.findByRole("alert")).toHaveTextContent("already running");
    });

    it("stops a running session after confirmation", async () => {
      launcherBackend();
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);
      await user.click(await findPlay());
      const status = await screen.findByRole("status", { name: "Session" });

      await user.click(within(status).getByRole("button", { name: "Stop" }));
      expect(within(status).getByText(/unsaved progress and NVRAM/)).toBeInTheDocument();
      await user.click(within(status).getByRole("button", { name: "Keep running" }));
      expect(calls("stop_launch_session")).toHaveLength(0);

      await user.click(within(status).getByRole("button", { name: "Stop" }));
      await user.click(within(status).getByRole("button", { name: "Stop now" }));

      expect(calls("stop_launch_session")).toEqual([{ id: 1 }]);
      expect(await screen.findByText("The session was stopped.")).toBeInTheDocument();
      expect(await findPlay()).toBeEnabled();
    });

    it("polls a running session until MAME exits and shows why it failed", async () => {
      const fake = launcherBackend();
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);
      await user.click(await findPlay());
      await screen.findByText(/MAME 0.280 is running since/);

      fake.setSessions([
        session(1, "pacman", {
          state: "exited",
          exitCode: 2,
          success: false,
          message: "Required files are missing (ROMs, CHDs or software media).",
          logTail: "pacman.6e NOT FOUND",
          finishedAt: 2,
        }),
      ]);

      expect(await screen.findByText(/Required files are missing/)).toHaveTextContent(
        "(exit code 2)",
      );
      expect(screen.getByText("Failed")).toBeInTheDocument();
      expect(screen.getByText("pacman.6e NOT FOUND")).toBeInTheDocument();
    });

    it("explains that devices cannot be launched", async () => {
      launcherBackend();
      renderWithLauncher(<MachineLaunchPanel name="z80" isRunnable={false} />);
      expect(await screen.findByText(/MAME cannot run it on its own/)).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Play" })).not.toBeInTheDocument();
    });

    it("launches with the selected profile", async () => {
      launcherBackend({
        config: launchConfig({
          profiles: [
            { id: "default", name: "Default", options: {} },
            { id: "quiet", name: "Quiet", options: { volume: -30 } },
          ],
        }),
      });
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);
      await findPlay();

      await user.selectOptions(screen.getByLabelText("Profile"), "Quiet");
      await user.click(screen.getByRole("button", { name: "Play" }));

      expect(lastRequest("start_launch")).toEqual({
        installationId: NEW_ID,
        machine: "pacman",
        profileId: "quiet",
      });
    });
  });

  describe("command line preview", () => {
    it("toggles the preview preference and shows the arguments with their origin", async () => {
      launcherBackend();
      const { user, onShowCommandPreviewChange, view } = renderWithLauncher(
        <MachineLaunchPanel name="pacman" isRunnable />,
      );
      await findPlay();

      await user.click(screen.getByRole("button", { name: "Command line" }));
      expect(onShowCommandPreviewChange).toHaveBeenCalledWith(true);

      view.rerender(
        tree(<MachineLaunchPanel name="pacman" isRunnable />, true, onShowCommandPreviewChange),
      );
      expect(screen.getByRole("button", { name: "Command line" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      const preview = await screen.findByLabelText("Command line");
      expect(within(preview).getByText("C:\\Émulateurs MAME ✓\\mame.exe")).toBeInTheDocument();
      const args = within(within(preview).getByRole("list", { name: "Arguments" }));
      expect(args.getByText("pacman")).toBeInTheDocument();
      expect(args.getByText("Global")).toBeInTheDocument();
      expect(within(preview).getByLabelText("Full command line")).toHaveTextContent(
        '"C:\\Émulateurs MAME ✓\\mame.exe" pacman -window',
      );
      expect(lastRequest("preview_launch")).toEqual({ installationId: NEW_ID, machine: "pacman" });
    });

    it("shows preview errors", async () => {
      launcherBackend({
        overrides: {
          preview_launch: () => {
            throw commandError("installationNotFound", { installationId: NEW_ID });
          },
        },
      });
      renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />, true);
      expect(
        await screen.findByText("This MAME installation is no longer registered."),
      ).toBeInTheDocument();
    });
  });

  describe("options", () => {
    it("saves options for a machine and refreshes the preview", async () => {
      const fake = launcherBackend();
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />, true);
      await findPlay();
      await screen.findByLabelText("Full command line");
      const previews = calls("preview_launch").length;

      await user.click(screen.getByRole("button", { name: "Options for this machine" }));
      const editor = screen.getByRole("group", { name: "Options for this machine" });
      expect(within(editor).getByText(/MAME uses its own configuration/)).toBeInTheDocument();
      await user.selectOptions(within(editor).getByRole("combobox"), "window");
      await user.selectOptions(
        within(editor).getByRole("combobox", { name: /^Window mode/ }),
        "Off (-nowindow)",
      );
      await user.click(screen.getByRole("button", { name: "Save options" }));

      await waitFor(() => {
        expect(fake.config().machineOverrides).toEqual({ pacman: { window: false } });
      });
      expect(
        await screen.findByRole("button", { name: /Options for this machine/ }),
      ).toHaveTextContent("1");
      await waitFor(() => {
        expect(calls("preview_launch").length).toBeGreaterThan(previews);
      });
    });

    it("blocks invalid values and reports backend rejections", async () => {
      launcherBackend({
        overrides: {
          save_launch_config: () => {
            throw commandError("invalidRequest", { message: "option 'bios': is not valid" });
          },
        },
      });
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);
      await findPlay();
      await user.click(screen.getByRole("button", { name: "Options for this machine" }));
      const editor = screen.getByRole("group", { name: "Options for this machine" });

      await user.selectOptions(within(editor).getByRole("combobox"), "volume");
      const volume = within(editor).getByRole("spinbutton", { name: /^Volume/ });
      await user.clear(volume);
      await user.type(volume, "50");
      expect(within(editor).getByText("Enter an integer from -96 to 12.")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Save options" })).toBeDisabled();

      await user.clear(volume);
      await user.type(volume, "-12");
      await user.click(screen.getByRole("button", { name: "Save options" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("option 'bios': is not valid");
    });
  });

  describe("software launch", () => {
    it("requires an explicit machine choice when none is recommended", async () => {
      launcherBackend({ info: softwareInfo() });
      const { user } = renderWithLauncher(
        <SoftwareLaunchPanel list="apple2_flop_orig" name="ultima4" refreshKey={0} />,
      );

      const machine = await screen.findByLabelText("Machine");
      expect(machine).toHaveValue("");
      expect(screen.getByText(/does not identify a single suitable one/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Play" })).toBeDisabled();
      expect(
        within(machine).getByRole("option", { name: /apple2tape.*no media device/ }),
      ).toBeDisabled();
      expect(
        within(machine).getByRole("option", { name: /apple2\).*compatible list, filter mismatch/ }),
      ).toBeEnabled();

      await user.selectOptions(machine, "apple2e");
      expect(screen.getByText("Original list")).toBeInTheDocument();
      const media = screen.getByRole("group", { name: "Media" });
      expect(within(media).getByLabelText(/floppydisk1/)).toHaveValue("flop1");
      expect(within(media).getByLabelText(/floppydisk2/)).toHaveValue("flop2");

      await user.click(await findPlay());
      expect(lastRequest("start_launch")).toEqual({
        installationId: NEW_ID,
        machine: "apple2e",
        software: {
          list: "apple2_flop_orig",
          name: "ultima4",
          media: [
            { device: "flop1", part: "flop1" },
            { device: "flop2", part: "flop2" },
          ],
        },
      });
      expect(await screen.findByRole("status", { name: "Session" })).toHaveTextContent("Running");
    });

    it("preselects the only valid machine and lets the user change the parts", async () => {
      launcherBackend({ info: softwareInfo({ recommended: "apple2e" }) });
      const { user } = renderWithLauncher(
        <SoftwareLaunchPanel list="apple2_flop_orig" name="ultima4" refreshKey={0} />,
        true,
      );
      expect(await screen.findByLabelText("Machine")).toHaveValue("apple2e");
      expect(screen.getByText(/Preselected/)).toBeInTheDocument();
      expect(await screen.findByText("The software may not work fully.")).toBeInTheDocument();

      const media = screen.getByRole("group", { name: "Media" });
      // Moving side B to the first drive frees the second drive.
      await user.selectOptions(within(media).getByLabelText(/floppydisk1/), "flop2");
      expect(within(media).getByLabelText(/floppydisk2/)).toHaveValue("");
      await user.click(await findPlay());
      expect(lastRequest("start_launch")?.software?.media).toEqual([
        { device: "flop1", part: "flop2" },
      ]);

      await user.selectOptions(within(media).getByLabelText(/floppydisk1/), "");
      expect(screen.getByText("Mount at least one part to launch.")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /Play|Running/ })).toBeDisabled();
    });

    it("explains when no machine declares the list", async () => {
      launcherBackend({ info: softwareInfo({ candidates: [] }) });
      renderWithLauncher(<SoftwareLaunchPanel list="pcecd" name="ys12" refreshKey={0} />);
      expect(await screen.findByText(/No machine in the catalog declares/)).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Play" })).not.toBeInTheDocument();
    });

    it("retries when the candidates cannot be loaded", async () => {
      let attempts = 0;
      launcherBackend({
        overrides: {
          get_software_launch_info: () => {
            attempts++;
            if (attempts === 1) throw commandError("internal", { message: "disk I/O error" });
            return softwareInfo({ recommended: "apple2e" });
          },
        },
      });
      const { user } = renderWithLauncher(
        <SoftwareLaunchPanel list="apple2_flop_orig" name="ultima4" refreshKey={0} />,
      );
      expect(await screen.findByRole("alert")).toHaveTextContent("disk I/O error");
      await user.click(screen.getByRole("button", { name: "Retry" }));
      expect(await screen.findByLabelText("Machine")).toHaveValue("apple2e");
    });
  });

  describe("settings", () => {
    it("saves global options", async () => {
      const fake = launcherBackend();
      const { user } = renderWithLauncher(<LaunchSettings />);
      const global = await screen.findByRole("group", { name: "Global options" });

      await user.selectOptions(within(global).getByRole("combobox"), "rompath");
      expect(screen.getByRole("button", { name: "Save global options" })).toBeDisabled();
      await user.type(
        within(global).getByRole("textbox", { name: /^ROM paths/ }),
        "roms{Enter}D:/Émulateurs MAME ✓/roms",
      );
      await user.click(screen.getByRole("button", { name: "Save global options" }));

      await waitFor(() => {
        expect(fake.config().globalOptions).toEqual({
          rompath: ["roms", "D:/Émulateurs MAME ✓/roms"],
        });
      });
    });

    it("creates, edits, makes default and deletes profiles", async () => {
      const fake = launcherBackend();
      const { user } = renderWithLauncher(<LaunchSettings />);
      await user.type(await screen.findByLabelText("New profile name"), "Big Screen ✓");
      await user.click(screen.getByRole("button", { name: "Create profile" }));

      await waitFor(() => {
        expect(fake.config().profiles.map((profile) => profile.id)).toEqual([
          "default",
          "big-screen",
        ]);
      });
      const profile = await screen.findByRole("group", { name: "Profile Big Screen ✓" });
      await user.selectOptions(within(profile).getByRole("combobox"), "speed");
      await user.click(within(profile).getByRole("button", { name: "Save profile" }));
      await waitFor(() => {
        expect(fake.config().profiles[1]?.options).toEqual({ speed: 1 });
      });

      await user.click(await screen.findByRole("button", { name: "Make default" }));
      await waitFor(() => {
        expect(fake.config().defaultProfileId).toBe("big-screen");
      });

      await user.click(await screen.findByRole("button", { name: "Delete profile" }));
      await waitFor(() => {
        expect(fake.config().profiles.map((entry) => entry.id)).toEqual(["default"]);
      });
      expect(fake.config().defaultProfileId).toBe("default");
      // The default profile cannot be deleted.
      expect(await screen.findByRole("group", { name: "Profile Default" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Delete profile" })).not.toBeInTheDocument();
    });

    it("lists and removes machine and software options", async () => {
      const fake = launcherBackend({
        config: launchConfig({
          machineOverrides: { pacman: { window: true } },
          softwareOverrides: { "nes:smb": { speed: 2 } },
        }),
      });
      const { user } = renderWithLauncher(<LaunchSettings />);
      const list = await screen.findByRole("list", { name: "Machine and software options" });
      expect(list).toHaveTextContent("pacman · window: On");
      expect(list).toHaveTextContent("nes:smb · speed: 2");

      await user.click(within(list).getByRole("button", { name: "Remove options of pacman" }));
      await waitFor(() => {
        expect(fake.config().machineOverrides).toEqual({});
      });
      expect(fake.config().softwareOverrides).toEqual({ "nes:smb": { speed: 2 } });
    });

    it("warns when launch.json is invalid", async () => {
      backend({
        get_launch_config: () => ({
          ...launchConfigView(),
          loadError: "launch configuration is invalid",
        }),
        get_launch_sessions: () => [],
      });
      renderWithLauncher(<LaunchSettings />);
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "launch configuration is invalid. The defaults are shown; saving replaces the file.",
      );
    });
  });

  describe("installations", () => {
    it("tells sessions of other installations apart", async () => {
      const fake = launcherBackend();
      fake.setSessions([session(7, "pacman", { state: "running" }, null, OLD_ID)]);
      const { user } = renderWithLauncher(<MachineLaunchPanel name="pacman" isRunnable />);

      // Pac-Man runs from MAME 0.252: it can still start from MAME 0.280.
      expect(await findPlay()).toHaveTextContent("Play");
      expect(screen.queryByRole("status", { name: "Session" })).not.toBeInTheDocument();
      await user.click(await findPlay());
      expect(await screen.findByText(/MAME 0.280 is running since/)).toBeInTheDocument();
      expect(lastRequest("start_launch")?.installationId).toBe(NEW_ID);
    });

    it("configures each installation separately", async () => {
      const fake = launcherBackend({
        config: launchConfig({ machineOverrides: { pacman: { window: true } } }),
      });
      const { user } = renderWithLauncher(<LaunchSettings />);
      const chooser = await screen.findByRole("combobox", { name: "Installation" });
      expect(
        within(chooser)
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual(["MAME 0.280 (active)", "MAME 0.252"]);
      expect(
        await screen.findByRole("list", { name: "Machine and software options" }),
      ).toHaveTextContent("pacman · window: On");

      await user.selectOptions(chooser, OLD_ID);
      await waitFor(() => {
        expect(calls("get_launch_config").at(-1)).toEqual({ installationId: OLD_ID });
      });
      expect(
        await screen.findByText(/No machine or software has its own options/),
      ).toBeInTheDocument();
      const global = await screen.findByRole("group", { name: "Global options" });
      await user.selectOptions(within(global).getByRole("combobox"), "window");
      await user.click(screen.getByRole("button", { name: "Save global options" }));

      await waitFor(() => {
        expect(fake.configOf(OLD_ID).globalOptions).toEqual({ window: true });
      });
      // The other installation keeps its own options.
      expect(fake.config().globalOptions).toEqual({});
      expect(fake.config().machineOverrides).toEqual({ pacman: { window: true } });
    });
  });
});
