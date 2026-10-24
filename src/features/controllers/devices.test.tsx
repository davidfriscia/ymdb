import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  commandError,
  controller,
  controllersBackend,
  controllersView,
  MAME_PROFILE,
  userProfile,
} from "../../test/controllerFixtures";
import { installation, installationsView, NEW_ID, OLD_ID } from "../../test/installationFixtures";
import { launchConfig, launchConfigView, planControls, session } from "../../test/launcherFixtures";
import {
  InstallationsContext,
  InstallationScope,
  type InstallationsContextValue,
} from "../installations/installationsContext";
import type { LaunchPreview, LaunchRequest, SessionInfo } from "../launcher/launcherApi";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { MachineLaunchPanel } from "../launcher/MachineLaunchPanel";
import type { ControllersView, DetectedDevice, DeviceDetection } from "./controllersApi";
import { ControllersSettings } from "./ControllersSettings";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

const PORT_ORDER =
  "This controller is identified by USB port order. Swapping ports may change player assignments.";
const GP100_A =
  "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000";
const GP100_B =
  "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_5e6f7a8b-2222-11f1-8001-444553540000";

function device(index: number, overrides: Partial<DetectedDevice> = {}): DetectedDevice {
  return {
    index,
    name: "USB gamepad",
    provider: "winhybrid",
    deviceId: index === 1 ? GP100_A : GP100_B,
    deviceClass: "joystick",
    identity: "directInputInstance",
    vendorId: "081F",
    productId: "E401",
    serial: null,
    distinguishable: true,
    allowedBindings: ["device", "portOrder"],
    suggestedBinding: "portOrder",
    registeredControllers: [],
    notes: ["The id follows the USB port unless the device has its own serial number."],
    ...overrides,
  };
}

/** The session 3A hardware: two identical GP100 and an F310 in X mode. */
function detection(provider = "winhybrid"): DeviceDetection {
  return {
    installationId: NEW_ID,
    provider: provider as DeviceDetection["provider"],
    devices: [
      device(1),
      device(2),
      device(3, {
        name: "XInput Player 1",
        deviceId: "XInput Player 1",
        identity: "xinputSlot",
        vendorId: null,
        productId: null,
        allowedBindings: ["portOrder"],
      }),
    ],
    warnings: [],
  };
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (error: unknown) => void = () => undefined;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

/** Controllers with configured devices, as registered from a detection. */
function mapped(overrides: Partial<ControllersView> = {}): ControllersView {
  return controllersView({
    profiles: [MAME_PROFILE, userProfile("fighting", "Fighting")],
    controllers: [
      controller("gp100-a", "GP100 A", {
        kind: "gamepad",
        deviceId: GP100_A,
        provider: "winhybrid",
        binding: "portOrder",
      }),
      controller("f310", "F310", {
        kind: "gamepad",
        deviceId: "Controller (Gamepad F310) product_c21d046d instance_9c0d",
        provider: "winhybrid",
        binding: "device",
      }),
      controller("legacy", "Legacy stick", { deviceId: "Stick", provider: "dinput" }),
    ],
    ...overrides,
  });
}

function backend(
  options: { view?: ControllersView; overrides?: Partial<Record<string, Handler>> } = {},
) {
  const controllers = controllersBackend(options.view);
  let sessions: SessionInfo[] = [];
  const handlers: Partial<Record<string, Handler>> = {
    ...controllers.handlers,
    detect_controller_devices: (args) => detection((args.provider as string | null) ?? "winhybrid"),
    get_launch_config: () => launchConfigView(launchConfig(), NEW_ID),
    get_launch_sessions: () => sessions,
    get_download_status: () => null,
    preview_launch: (args): LaunchPreview => {
      const request = (args as { request: LaunchRequest }).request;
      const players =
        request.controls?.players ??
        controllers.handlers.resolve_controls({ machine: request.machine, software: null }).players;
      const view = controllers.view();
      const mappings = players.flatMap((id, index) => {
        const entry = view.controllers.find((candidate) => candidate.id === id);
        return entry?.deviceId && entry.provider && entry.binding
          ? [
              {
                player: index + 1,
                controllerId: entry.id,
                controllerName: entry.name,
                provider: "winhybrid" as const,
                deviceId: entry.deviceId,
                binding: entry.binding,
              },
            ]
          : [];
      });
      return {
        plan: {
          installationId: request.installationId,
          profileId: "default",
          executable: "C:\\MAME\\mame.exe",
          workingDirectory: "C:\\MAME",
          controls: planControls({
            players,
            playersSource: request.controls?.players ? "request" : "default",
            mappings,
            joystickProvider: mappings.length > 0 ? "winhybrid" : null,
            controllerFile:
              mappings.length > 0
                ? {
                    name: "ymdb-0011223344556677",
                    path: "C:\\YMDB\\controller-files\\ymdb-0011223344556677.cfg",
                    hash: "0011223344556677",
                    content: '<mameconfig version="10"><mapdevice /></mameconfig>',
                    baseline: null,
                  }
                : null,
          }),
          arguments: [{ role: "machine", tokens: [request.machine] }],
        },
        commandLine: [request.machine],
        warnings: mappings.some((mapping) => mapping.binding === "portOrder")
          ? [
              "Identified by USB port or connection order: GP100 A. Swapping ports may change player assignments.",
            ]
          : [],
      };
    },
    start_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      const started: SessionInfo = {
        ...session(sessions.length + 1, request.machine),
        mapping: {
          complete: true,
          players: [
            {
              player: 1,
              controllerId: "f310",
              controllerName: "F310",
              deviceId: "Controller (Gamepad F310)",
              binding: "device",
              status: "verified",
              deviceName: "Controller (Gamepad F310)",
            },
            {
              player: 2,
              controllerId: "gp100-a",
              controllerName: "GP100 A",
              deviceId: GP100_A,
              binding: "portOrder",
              status: "verifiedPortOrder",
              deviceName: "USB gamepad",
            },
            {
              player: 3,
              controllerId: "gp100-b",
              controllerName: "GP100 B",
              deviceId: GP100_B,
              binding: "portOrder",
              status: "missing",
              deviceName: null,
            },
          ],
          message: null,
        },
      };
      sessions = [started, ...sessions];
      return started;
    },
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
  return controllers;
}

function installationsValue(): InstallationsContextValue {
  const view = installationsView([installation(NEW_ID, "0.289"), installation(OLD_ID, "0.252")]);
  return {
    view,
    error: null,
    installations: view.installations,
    activeId: NEW_ID,
    active: view.installations[0] ?? null,
    labels: new Map([
      [NEW_ID, "MAME 0.289"],
      [OLD_ID, "MAME 0.252"],
    ]),
    select: vi.fn(),
    selectError: null,
    reload: vi.fn(() => Promise.resolve()),
  };
}

function renderWith(children: ReactNode, showCommandPreview = false) {
  const user = userEvent.setup();
  render(
    <InstallationsContext.Provider value={installationsValue()}>
      <LauncherProvider
        installationId={NEW_ID}
        showCommandPreview={showCommandPreview}
        onShowCommandPreviewChange={vi.fn()}
        pollIntervalMs={20}
      >
        <InstallationScope value={NEW_ID}>{children}</InstallationScope>
      </LauncherProvider>
    </InstallationsContext.Provider>,
  );
  return { user };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("Settings → Controllers: device detection", () => {
  it("detects devices only on request and registers one explicitly", async () => {
    const controllers = backend();
    const { user } = renderWith(<ControllersSettings />);
    const panel = await screen.findByRole("group", { name: "Device detection" });
    expect(calls("detect_controller_devices")).toHaveLength(0);
    expect(within(panel).getByLabelText("Joystick provider")).toHaveDisplayValue(
      "winhybrid (MAME's default)",
    );

    await user.click(within(panel).getByRole("button", { name: "Detect devices" }));
    const list = await within(panel).findByRole("list", { name: "Detected devices" });
    expect(calls("detect_controller_devices")).toEqual([
      { installationId: NEW_ID, provider: "winhybrid" },
    ]);
    const [first, second, third] = within(list).getAllByRole("listitem");
    if (!first || !second || !third) throw new Error("three devices expected");
    const items = [first, second, third];

    expect(items[0]).toHaveTextContent("Joystick #1");
    expect(items[0]).toHaveTextContent("VID 081F · PID E401");
    expect(items[0]).toHaveTextContent("Device binding, Port/order binding");
    // The XInput slot supports the port order binding only, with the warning.
    expect(items[2]).toHaveTextContent("Port/order binding");
    expect(within(third).getByText(PORT_ORDER)).toBeVisible();
    // Nothing is registered or shown as connected afterwards.
    expect(controllers.view().controllers).toEqual([]);

    await user.click(within(second).getByRole("button", { name: "Register…" }));
    const form = screen.getByRole("form", { name: "Register device" });
    expect(within(form).getByLabelText("Device ID")).toHaveValue(GP100_B);
    expect(within(form).getByLabelText("Provider")).toHaveValue("winhybrid");
    expect(within(form).getByRole("radio", { name: /^Port\/order binding/ })).toBeChecked();
    expect(within(form).getByText(PORT_ORDER)).toBeVisible();
    await user.clear(within(form).getByLabelText("Name"));
    await user.type(within(form).getByLabelText("Name"), "GP100 B");
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(calls("create_controller")).toEqual([
      {
        controller: {
          name: "GP100 B",
          kind: "gamepad",
          deviceClass: "joystick",
          deviceId: GP100_B,
          provider: "winhybrid",
          binding: "portOrder",
        },
      },
    ]);
    // The detected device now shows its registration and no second Register.
    await waitFor(() => {
      expect(items[1]).toHaveTextContent("Registered as GP100 B.");
    });
    expect(within(second).queryByRole("button", { name: "Register…" })).not.toBeInTheDocument();
    expect(within(first).getByRole("button", { name: "Register…" })).toBeEnabled();
  });

  it("refuses to register devices MAME cannot tell apart", async () => {
    backend({
      overrides: {
        detect_controller_devices: () => ({
          ...detection("sdljoy"),
          devices: [1, 2].map((index) =>
            device(index, {
              provider: "sdljoy",
              deviceId: "0300007c1f08000001e4000000000000",
              identity: "sdlModel",
              distinguishable: false,
              suggestedBinding: null,
              notes: ["Another detected device has the same id: MAME cannot tell them apart."],
            }),
          ),
        }),
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const panel = await screen.findByRole("group", { name: "Device detection" });
    await user.selectOptions(within(panel).getByLabelText("Joystick provider"), "sdljoy");
    await user.click(within(panel).getByRole("button", { name: "Detect devices" }));
    const list = await within(panel).findByRole("list", { name: "Detected devices" });
    expect(calls("detect_controller_devices")).toEqual([
      { installationId: NEW_ID, provider: "sdljoy" },
    ]);
    for (const item of within(list).getAllByRole("listitem")) {
      expect(within(item).getByText("Not distinguishable")).toBeVisible();
      expect(within(item).getByRole("button", { name: "Register…" })).toBeDisabled();
    }
  });

  it("reports a failed detection and never shows it as an empty list", async () => {
    backend({
      overrides: {
        detect_controller_devices: () => {
          throw commandError("detectionFailed", {
            message: "MAME did not finish the detection within 30 seconds and was stopped",
          });
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const panel = await screen.findByRole("group", { name: "Device detection" });
    await user.selectOptions(within(panel).getByLabelText("MAME installation"), "MAME 0.252");
    await user.click(within(panel).getByRole("button", { name: "Detect devices" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent(
      "The devices could not be detected: MAME did not finish the detection within 30 seconds",
    );
    expect(calls("detect_controller_devices")).toEqual([
      { installationId: OLD_ID, provider: "winhybrid" },
    ]);
    expect(within(panel).queryByRole("list", { name: "Detected devices" })).not.toBeInTheDocument();
  });

  it("shows a detection in progress and keeps the controls busy until it ends", async () => {
    const slow = deferred<DeviceDetection>();
    backend({ overrides: { detect_controller_devices: () => slow.promise } });
    const { user } = renderWith(<ControllersSettings />);
    const panel = await screen.findByRole("group", { name: "Device detection" });
    await user.click(within(panel).getByRole("button", { name: "Detect devices" }));
    expect(within(panel).getByRole("button", { name: "Detecting…" })).toBeDisabled();
    expect(within(panel).getByLabelText("Joystick provider")).toBeDisabled();
    expect(within(panel).getByRole("status")).toHaveTextContent("listing the connected devices");
    slow.resolve({ ...detection(), devices: [], warnings: ["No joystick was detected."] });
    expect(await within(panel).findByText("No joystick was detected.")).toBeVisible();
    expect(within(panel).getByRole("button", { name: "Detect devices" })).toBeEnabled();
  });

  it("explains controllers registered before device mapping", async () => {
    backend({ view: mapped() });
    renderWith(<ControllersSettings />);
    const list = await screen.findByRole("list", { name: "Controllers" });
    const legacy = within(list).getByRole("listitem", { name: /Legacy stick/ });
    expect(legacy).toHaveTextContent("Device not configured");
    expect(legacy).toHaveTextContent("cannot be mapped to a player");
    const f310 = within(list).getByRole("listitem", { name: /F310/ });
    expect(f310).toHaveTextContent("Device binding");
    expect(within(f310).queryByText(PORT_ORDER)).not.toBeInTheDocument();
  });

  it("refuses the device binding for an XInput slot before saving", async () => {
    backend();
    const { user } = renderWith(<ControllersSettings />);
    await user.click(await screen.findByRole("button", { name: "Add controller…" }));
    const form = screen.getByRole("form", { name: "New controller" });
    await user.type(within(form).getByLabelText("Name"), "Pad");
    await user.type(within(form).getByLabelText("Device ID"), "XInput Player 1");
    await user.selectOptions(within(form).getByLabelText("Provider"), "winhybrid");
    await user.click(within(form).getByRole("radio", { name: /^Device binding/ }));
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(within(form).getByText(/An XInput slot follows the order/)).toBeVisible();
    expect(calls("create_controller")).toHaveLength(0);
  });
});

describe("Settings → Controllers: default players", () => {
  it("saves and removes the default players without touching the profile", async () => {
    const controllers = backend({ view: mapped() });
    const { user } = renderWith(<ControllersSettings />);
    const editor = await screen.findByRole("group", { name: "Default players" });
    const player1 = within(editor).getByLabelText("Player 1");
    // Controllers without a configured device cannot be chosen.
    expect(within(player1).getByRole("option", { name: /Legacy stick/ })).toBeDisabled();
    await user.selectOptions(player1, "F310");
    // A controller chosen for one player is not offered to another.
    expect(
      within(within(editor).getByLabelText("Player 2")).getByRole("option", {
        name: "F310 (Player 1)",
      }),
    ).toBeDisabled();
    await user.selectOptions(within(editor).getByLabelText("Player 2"), "GP100 A");
    expect(within(editor).getByText(PORT_ORDER)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Save default players" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "default" },
        update: { players: { kind: "set", value: ["f310", "gp100-a"] } },
      },
    ]);
    await waitFor(() => {
      expect(controllers.view().assignments.default).toEqual({
        profileId: "mame",
        players: ["f310", "gp100-a"],
      });
    });
    await user.click(await screen.findByRole("button", { name: "Remove default players" }));
    await waitFor(() => {
      expect(controllers.view().assignments.default).toEqual({ profileId: "mame" });
    });
  });
});

describe("Advanced → Controls: players", () => {
  async function openControls(user: ReturnType<typeof userEvent.setup>) {
    await user.click(await screen.findByRole("button", { name: /^Advanced/ }));
    await user.click(screen.getByRole("button", { name: "Controls for this machine" }));
    return screen.findByRole("group", { name: "Controls" });
  }

  it("saves and resets the players of a machine independently of its profile", async () => {
    const controllers = backend({
      view: mapped({
        assignments: {
          default: { profileId: "mame" },
          machines: { sf2: { profileId: "fighting" } },
          software: {},
          machineSoftware: {},
        },
      }),
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControls(user);
    // The row is remounted with the saved value after each change.
    const editor = () =>
      within(panel).getByRole("group", { name: "Players for This machine (sf2)" });
    await within(panel).findByRole("group", { name: "Players for This machine (sf2)" });
    await user.selectOptions(within(editor()).getByLabelText("Player 2"), "F310");
    await user.click(within(panel).getByRole("button", { name: "Save players" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "machine", key: "sf2" },
        update: { players: { kind: "set", value: [null, "f310"] } },
      },
    ]);
    await waitFor(() => {
      expect(controllers.view().assignments.machines.sf2).toEqual({
        profileId: "fighting",
        players: [null, "f310"],
      });
    });
    await waitFor(() => {
      expect(panel).toHaveTextContent("Player 2: F310 — specific to this machine");
    });
    // Resetting the players keeps the profile.
    await user.click(within(panel).getByRole("button", { name: "Reset players to inherited" }));
    await waitFor(() => {
      expect(controllers.view().assignments.machines.sf2).toEqual({ profileId: "fighting" });
    });
    // Resetting the profile keeps the players.
    await user.selectOptions(within(editor()).getByLabelText("Player 1"), "GP100 A");
    await user.click(within(panel).getByRole("button", { name: "Save players" }));
    await waitFor(() => {
      expect(controllers.view().assignments.machines.sf2).toEqual({
        profileId: "fighting",
        players: ["gp100-a"],
      });
    });
    await waitFor(() => {
      expect(panel).toHaveTextContent("Player 1: GP100 A — specific to this machine");
    });
    await user.click(within(panel).getByRole("button", { name: "Reset to inherited" }));
    await waitFor(() => {
      expect(controllers.view().assignments.machines.sf2).toEqual({ players: ["gp100-a"] });
    });
  });
});

describe("launch bar: temporary players", () => {
  it("uses temporary players for the preview and the launch without saving them", async () => {
    const controllers = backend({ view: mapped() });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />, true);
    await user.click(await screen.findByRole("button", { name: /^Advanced/ }));
    const playersButton = await screen.findByRole("button", { name: "Players" });
    await waitFor(() => {
      expect(playersButton).toBeEnabled();
    });
    await user.click(playersButton);
    const temporary = screen.getByRole("group", { name: "Temporary players" });
    await user.selectOptions(within(temporary).getByLabelText("Player 1"), "F310");
    await user.selectOptions(within(temporary).getByLabelText("Player 2"), "GP100 A");
    await user.click(within(temporary).getByRole("button", { name: "Use for the next launches" }));
    expect(temporary).toHaveTextContent(
      "Players: Player 1: F310, Player 2: GP100 A for the launches from this page only",
    );
    expect(screen.getByRole("button", { name: "Players (temporary)" })).toBeVisible();

    // The preview uses the same request: mappings, provider and the file.
    const preview = await screen.findByLabelText("Command line");
    const mappings = await within(preview).findByRole("list", { name: "Player mappings" });
    expect(mappings).toHaveTextContent("Player 1: F310 — Device binding, winhybrid");
    expect(mappings).toHaveTextContent("Player 2: GP100 A — Port/order binding, winhybrid");
    expect(preview).toHaveTextContent("temporary for this launch");
    expect(preview).toHaveTextContent("ymdb-0011223344556677.cfg");
    expect(preview).toHaveTextContent("Swapping ports may change player assignments");
    const lastPreview = calls("preview_launch").at(-1) as { request: LaunchRequest };
    expect(lastPreview.request.controls).toEqual({ players: ["f310", "gp100-a"] });

    // Play sends the same temporary players; nothing is saved.
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => {
      expect(calls("start_launch")).toHaveLength(1);
    });
    expect((calls("start_launch")[0] as { request: LaunchRequest }).request.controls).toEqual({
      players: ["f310", "gp100-a"],
    });
    expect(calls("update_controller_assignment")).toHaveLength(0);
    expect(controllers.view().assignments.default).toEqual({ profileId: "mame" });

    // The session shows what MAME reported for each mapping.
    const status = await screen.findByRole("group", { name: "Player mappings" });
    expect(status).toHaveTextContent("Player 1: F310 Applied to this device");
    expect(status).toHaveTextContent(
      "Player 2: GP100 A Applied (by port order: the physical device is not guaranteed)",
    );
    expect(status).toHaveTextContent("Player 3: GP100 B Device not connected");
    expect(within(status).getByText(PORT_ORDER)).toBeVisible();
  });

  it("saves temporary players as a preference only on explicit request", async () => {
    const controllers = backend({ view: mapped() });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const playersButton = await screen.findByRole("button", { name: "Players" });
    await waitFor(() => {
      expect(playersButton).toBeEnabled();
    });
    await user.click(playersButton);
    const temporary = screen.getByRole("group", { name: "Temporary players" });
    await user.selectOptions(within(temporary).getByLabelText("Player 1"), "GP100 A");
    await user.click(within(temporary).getByRole("button", { name: "Use for the next launches" }));
    expect(calls("update_controller_assignment")).toHaveLength(0);
    await user.click(within(temporary).getByRole("button", { name: "Save players as preference" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "machine", key: "sf2" },
        update: { players: { kind: "set", value: ["gp100-a"] } },
      },
    ]);
    expect(await screen.findByText("The players are saved for: This machine (sf2).")).toBeVisible();
    expect(controllers.view().assignments.machines.sf2).toEqual({ players: ["gp100-a"] });
    expect(screen.getByRole("button", { name: "Players" })).toBeVisible();
  });

  it("drops a deleted controller from the temporary players", async () => {
    const controllers = backend({ view: mapped() });
    const { user } = renderWith(
      <>
        <MachineLaunchPanel name="sf2" isRunnable />
        <ControllersSettings />
      </>,
      true,
    );
    const playersButton = await screen.findByRole("button", { name: "Players" });
    await waitFor(() => {
      expect(playersButton).toBeEnabled();
    });
    await user.click(playersButton);
    const temporary = screen.getByRole("group", { name: "Temporary players" });
    await user.selectOptions(within(temporary).getByLabelText("Player 1"), "F310");
    await user.selectOptions(within(temporary).getByLabelText("Player 2"), "GP100 A");
    await user.click(within(temporary).getByRole("button", { name: "Use for the next launches" }));
    const list = await screen.findByRole("list", { name: "Controllers" });
    const gp100 = within(list).getByRole("listitem", { name: /GP100 A/ });
    await user.click(within(gp100).getByRole("button", { name: "Remove…" }));
    await user.click(within(gp100).getByRole("button", { name: "Remove controller" }));
    await waitFor(() => {
      expect(controllers.view().controllers.map((entry) => entry.id)).not.toContain("gp100-a");
    });
    await user.click(screen.getByRole("button", { name: /^Advanced/ }));
    await waitFor(() => {
      const last = calls("preview_launch").at(-1) as { request: LaunchRequest } | undefined;
      expect(last?.request.controls).toEqual({ players: ["f310"] });
    });
  });
});
