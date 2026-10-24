import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
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
import { NEW_ID } from "../../test/installationFixtures";
import { launchConfig, launchConfigView, planControls, session } from "../../test/launcherFixtures";
import { InstallationScope } from "../installations/installationsContext";
import type {
  LaunchRequest,
  MachineCandidate,
  SessionInfo,
  SoftwareLaunchInfo,
} from "../launcher/launcherApi";
import { LauncherProvider } from "../launcher/LauncherProvider";
import { MachineLaunchPanel } from "../launcher/MachineLaunchPanel";
import { SoftwareLaunchPanel } from "../launcher/SoftwareLaunchPanel";
import type { ControllersView, ControlsResolution } from "./controllersApi";
import { ControllersSettings } from "./ControllersSettings";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

/**
 * A fake backend: the controller commands of `controllersBackend`, and a
 * launcher whose preview reports the controls of the request like the
 * backend (`launcher::controls`).
 */
function backend(
  options: {
    view?: ControllersView;
    overrides?: Partial<Record<string, Handler>>;
    info?: SoftwareLaunchInfo;
  } = {},
) {
  const controllers = controllersBackend(options.view);
  let sessions: SessionInfo[] = [];
  const handlers: Partial<Record<string, Handler>> = {
    ...controllers.handlers,
    get_launch_config: () => launchConfigView(launchConfig(), NEW_ID),
    get_launch_sessions: () => sessions,
    get_download_status: () => null,
    get_software_launch_info: () => options.info ?? null,
    preview_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      const saved = controllers.handlers.resolve_controls({
        installationId: request.installationId,
        machine: request.machine,
        software: request.software
          ? { list: request.software.list, name: request.software.name }
          : null,
      });
      const temporary = request.controls?.profileId;
      const profile = controllers
        .view()
        .profiles.find((entry) => entry.id === (temporary ?? saved.profileId));
      return {
        plan: {
          installationId: request.installationId,
          profileId: "default",
          executable: "C:\\MAME\\mame.exe",
          workingDirectory: "C:\\MAME",
          controls: planControls({
            profileId: profile?.id ?? "mame",
            profileName: profile?.name ?? "MAME configuration",
            profileSource: temporary ? "request" : saved.profileSource,
            cfgDirectory: profile?.managed ? `C:\\YMDB\\${profile.id}\\cfg` : null,
            baseFile: profile?.hasBase
              ? `C:\\YMDB\\controller-profiles\\${profile.id}\\base.cfg`
              : null,
          }),
          arguments: [{ role: "machine", tokens: [request.machine] }],
        },
        commandLine: [request.machine],
        warnings: [],
      };
    },
    start_launch: (args) => {
      const request = (args as { request: LaunchRequest }).request;
      const started: SessionInfo = {
        ...session(sessions.length + 1, request.machine),
        warnings: [
          "Another running MAME session uses the controller profile 'Arcade Six Buttons': when both exit, the general input settings (default.cfg) of the last one to exit are kept.",
        ],
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

function renderWith(children: ReactNode, showCommandPreview = false) {
  const user = userEvent.setup();
  render(
    <LauncherProvider
      installationId={NEW_ID}
      showCommandPreview={showCommandPreview}
      onShowCommandPreviewChange={vi.fn()}
      pollIntervalMs={20}
    >
      <InstallationScope value={NEW_ID}>{children}</InstallationScope>
    </LauncherProvider>,
  );
  return { user };
}

/** Two user profiles, one with a baseline file, and two controllers. */
function populated(overrides: Partial<ControllersView> = {}): ControllersView {
  return controllersView({
    profiles: [
      MAME_PROFILE,
      userProfile("six-buttons", "Arcade Six Buttons", {
        hasBase: true,
        baseFileName: "xarcade.cfg",
      }),
      userProfile("gamepad", "Gamepad Standard"),
    ],
    controllers: [
      controller("stick-p1", "Arcade Stick P1", {
        deviceId: "VID_D209&PID_1601",
        provider: "dinput",
      }),
      controller("pad", "Pad", { kind: "gamepad" }),
    ],
    ...overrides,
  });
}

async function findProfile(name: string) {
  const list = await screen.findByRole("list", { name: "Controller profiles" });
  return within(list).getByRole("listitem", { name: new RegExp(`^${name}`) });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("Settings → Controllers", () => {
  it("shows the three areas, the system profile and the empty states", async () => {
    backend();
    renderWith(<ControllersSettings />);
    expect(await screen.findByRole("heading", { name: "Controller Profiles" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Controllers" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Default Assignments" })).toBeVisible();

    const mame = await findProfile("MAME configuration");
    expect(within(mame).getByText("System")).toBeVisible();
    expect(within(mame).getByText("Default", { selector: ".badge" })).toBeVisible();
    expect(
      within(mame).getByText(
        "This profile uses MAME's existing configuration without overriding its controller settings.",
      ),
    ).toBeVisible();
    // The system profile cannot be renamed, deleted or given a baseline file.
    expect(within(mame).queryByRole("button")).not.toBeInTheDocument();

    expect(screen.getByText(/No controller registered/)).toBeVisible();
    // Detection needs a MAME installation and never runs by itself.
    expect(screen.getByRole("button", { name: "Detect devices" })).toBeDisabled();
    expect(screen.getByText("Register a MAME installation to detect devices.")).toBeVisible();
    expect(calls("detect_controller_devices")).toHaveLength(0);
    expect(screen.getByLabelText("Default controller profile")).toHaveValue("mame");
    expect(screen.getByText(/Launch profiles are independent/)).toBeVisible();
    expect(
      screen.getByText("No machine or software has its own controller preference."),
    ).toBeVisible();
  });

  it("reports an invalid controllers file and offers nothing that could overwrite it", async () => {
    let broken = true;
    const controllers = backend({
      overrides: {
        get_controllers: () => {
          if (broken) {
            throw commandError("settings", {
              message:
                "the controllers file 'C:\\YMDB\\controllers.json' is invalid: expected value",
            });
          }
          return controllersView();
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/controllers.json' is invalid/);
    expect(alert).toHaveTextContent(/never replaces controllers.json/);
    expect(screen.queryByRole("button", { name: /Create profile/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add controller…" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Default controller profile")).not.toBeInTheDocument();

    broken = false;
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: "Create profile" })).toBeVisible();
    expect(controllers.view().profiles).toEqual([MAME_PROFILE]);
  });

  it("creates an empty profile without asking for an identifier", async () => {
    const controllers = backend();
    const { user } = renderWith(<ControllersSettings />);
    await user.click(await screen.findByRole("button", { name: "Create profile" }));
    expect(screen.getByText("Enter a name.")).toBeVisible();
    expect(calls("create_controller_profile")).toHaveLength(0);

    await user.type(screen.getByLabelText("New controller profile"), "Gamepad Standard");
    await user.click(screen.getByRole("button", { name: "Create profile" }));
    expect(calls("create_controller_profile")).toEqual([
      { name: "Gamepad Standard", importBase: false },
    ]);
    const item = await findProfile("Gamepad Standard");
    expect(within(item).getByText(/No baseline file/)).toBeVisible();
    expect(screen.getByLabelText("New controller profile")).toHaveValue("");
    expect(controllers.view().profiles.map((profile) => profile.id)).toEqual([
      "mame",
      "gamepad-standard",
    ]);
  });

  it("creates a profile from a controller file, or nothing when the choice is cancelled", async () => {
    const controllers = backend();
    const { user } = renderWith(<ControllersSettings />);
    await user.type(await screen.findByLabelText("New controller profile"), "Arcade Six Buttons");
    await user.click(screen.getByRole("checkbox", { name: /Start from a MAME controller/ }));

    controllers.setNextBase(null);
    await user.click(screen.getByRole("button", { name: "Choose file and create" }));
    expect(
      await screen.findByText("No file was chosen: the profile was not created."),
    ).toBeVisible();
    expect(controllers.view().profiles).toHaveLength(1);
    // The name is kept for another attempt.
    expect(screen.getByLabelText("New controller profile")).toHaveValue("Arcade Six Buttons");

    controllers.setNextBase("xarcade.cfg");
    await user.click(screen.getByRole("button", { name: "Choose file and create" }));
    const item = await findProfile("Arcade Six Buttons");
    expect(within(item).getByText("xarcade.cfg")).toBeVisible();
    expect(within(item).getByText(/not in this file/)).toBeVisible();
    expect(calls("create_controller_profile").at(-1)).toEqual({
      name: "Arcade Six Buttons",
      importBase: true,
    });
  });

  it("reports an invalid controller file", async () => {
    backend({
      overrides: {
        create_controller_profile: () => {
          throw commandError("invalidFile", {
            message: "unsupported configuration file version 9: MAME only loads version 10",
          });
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    await user.type(await screen.findByLabelText("New controller profile"), "Old");
    await user.click(screen.getByRole("checkbox", { name: /Start from a MAME controller/ }));
    await user.click(screen.getByRole("button", { name: "Choose file and create" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The file cannot be imported: unsupported configuration file version 9",
    );
  });

  it("renames a profile", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Gamepad Standard");
    await user.click(within(item).getByRole("button", { name: "Rename…" }));
    const name = within(item).getByLabelText("Name");
    await user.clear(name);
    await user.click(within(item).getByRole("button", { name: "Save name" }));
    expect(within(item).getByText("Enter a name.")).toBeVisible();
    await user.type(name, "Pad");
    await user.click(within(item).getByRole("button", { name: "Save name" }));
    expect(calls("rename_controller_profile")).toEqual([{ profileId: "gamepad", name: "Pad" }]);
    expect(await findProfile("Pad")).toBeVisible();
    expect(controllers.view().profiles[2]?.id).toBe("gamepad");
  });

  it("imports and replaces a baseline file; a cancelled dialog changes nothing", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<ControllersSettings />);
    let item = await findProfile("Gamepad Standard");
    controllers.setNextBase(null);
    await user.click(within(item).getByRole("button", { name: "Import baseline file…" }));
    await waitFor(() => {
      expect(calls("import_controller_profile_base")).toHaveLength(1);
    });
    expect(within(item).getByText(/No baseline file/)).toBeVisible();

    controllers.setNextBase("hotrod.cfg");
    await user.click(within(item).getByRole("button", { name: "Import baseline file…" }));
    item = await findProfile("Gamepad Standard");
    expect(await within(item).findByText("hotrod.cfg")).toBeVisible();

    controllers.setNextBase("slikstik.cfg");
    await user.click(within(item).getByRole("button", { name: "Replace baseline file…" }));
    expect(await within(item).findByText("slikstik.cfg")).toBeVisible();
    expect(calls("import_controller_profile_base")).toEqual([
      { profileId: "gamepad" },
      { profileId: "gamepad" },
      { profileId: "gamepad" },
    ]);
  });

  it("reports a profile deleted while its file was being chosen", async () => {
    backend({
      view: populated(),
      overrides: {
        import_controller_profile_base: () => {
          throw commandError("notFound", {
            message: "the controller profile 'gamepad' does not exist",
          });
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Gamepad Standard");
    await user.click(within(item).getByRole("button", { name: "Import baseline file…" }));
    expect(await within(item).findByRole("alert")).toHaveTextContent(
      "Not found: the controller profile 'gamepad' does not exist",
    );
  });

  it("removes a baseline file after confirmation and keeps the profile", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Arcade Six Buttons");

    await user.click(within(item).getByRole("button", { name: "Remove baseline file…" }));
    const confirm = within(item).getByRole("group", { name: "Remove baseline file" });
    expect(confirm).toHaveTextContent(/continues to exist/);
    expect(confirm).toHaveTextContent(
      /input assignments MAME saved in its configuration directory stay available/,
    );
    expect(confirm).toHaveTextContent(/effective inputs may change/);
    await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(calls("remove_controller_profile_base")).toHaveLength(0);

    await user.click(within(item).getByRole("button", { name: "Remove baseline file…" }));
    await user.click(within(item).getByRole("button", { name: "Remove baseline file" }));
    expect(calls("remove_controller_profile_base")).toEqual([{ profileId: "six-buttons" }]);
    expect(await within(item).findByText(/No baseline file/)).toBeVisible();
    expect(
      within(item).queryByRole("group", { name: "Remove baseline file" }),
    ).not.toBeInTheDocument();
    expect(controllers.view().profiles.some((profile) => profile.id === "six-buttons")).toBe(true);
  });

  it("keeps the baseline file when the backend refuses its removal", async () => {
    backend({
      view: populated(),
      overrides: {
        remove_controller_profile_base: () => {
          throw commandError("settings", { message: "failed to write 'controllers.json'" });
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Arcade Six Buttons");
    await user.click(within(item).getByRole("button", { name: "Remove baseline file…" }));
    await user.click(within(item).getByRole("button", { name: "Remove baseline file" }));
    expect(await within(item).findByRole("alert")).toHaveTextContent(
      "Settings error: failed to write 'controllers.json'",
    );
    expect(within(item).getByText("xarcade.cfg")).toBeVisible();
    // The confirmation stays, so that the user can retry or cancel.
    expect(within(item).getByRole("group", { name: "Remove baseline file" })).toBeVisible();
  });

  it("deletes an unused profile after confirmation", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Gamepad Standard");
    await user.click(within(item).getByRole("button", { name: "Delete…" }));
    const confirm = within(item).getByRole("group", { name: "Delete controller profile" });
    expect(confirm).toHaveTextContent(/kept on disk/);
    await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(calls("delete_controller_profile")).toHaveLength(0);

    await user.click(within(item).getByRole("button", { name: "Delete…" }));
    await user.click(within(item).getByRole("button", { name: "Delete profile" }));
    expect(calls("delete_controller_profile")).toEqual([{ profileId: "gamepad" }]);
    await waitFor(() => {
      expect(screen.queryByText("Gamepad Standard")).not.toBeInTheDocument();
    });
    expect(controllers.view().profiles).toHaveLength(2);
  });

  it("lists the assignments using a profile and never deletes them", async () => {
    const view = populated({
      assignments: {
        default: { profileId: "mame" },
        machines: { sf2: { profileId: "six-buttons" } },
        software: {},
        machineSoftware: { "nes/nes:smb": { profileId: "six-buttons" } },
      },
    });
    backend({ view });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Arcade Six Buttons");
    expect(within(item).getByText("Used by 2 saved assignments:")).toBeVisible();
    await user.click(within(item).getByRole("button", { name: "Delete…" }));
    const confirm = within(item).getByRole("group", { name: "Delete controller profile" });
    expect(confirm).toHaveTextContent(/cannot be deleted while saved assignments use it/);
    expect(within(confirm).getByText("Machine sf2")).toBeVisible();
    expect(within(confirm).getByText("Software nes:smb on nes")).toBeVisible();
    expect(
      within(confirm).queryByRole("button", { name: "Delete profile" }),
    ).not.toBeInTheDocument();
    expect(calls("delete_controller_profile")).toHaveLength(0);
  });

  it("shows the references returned when the backend refuses a deletion", async () => {
    backend({
      view: populated(),
      overrides: {
        // Assigned meanwhile, for example from a detail page.
        delete_controller_profile: () => {
          throw commandError("controllersInUse", {
            message: "the profile 'gamepad' is used by 1 assignment(s); change them first",
            references: [{ level: "software", key: "nes:smb" }],
          });
        },
      },
    });
    const { user } = renderWith(<ControllersSettings />);
    const item = await findProfile("Gamepad Standard");
    await user.click(within(item).getByRole("button", { name: "Delete…" }));
    await user.click(within(item).getByRole("button", { name: "Delete profile" }));
    const alert = await within(item).findByRole("alert");
    expect(alert).toHaveTextContent(
      "Cannot be removed: the profile 'gamepad' is used by 1 assignment(s)",
    );
    expect(within(alert).getByText("Software nes:smb")).toBeVisible();
    expect(await findProfile("Gamepad Standard")).toBeVisible();
  });
});

describe("Settings → Controllers: controllers", () => {
  it("adds, edits and removes controllers", async () => {
    const controllers = backend();
    const { user } = renderWith(<ControllersSettings />);
    await user.click(await screen.findByRole("button", { name: "Add controller…" }));
    const form = screen.getByRole("form", { name: "New controller" });
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(within(form).getByText("Enter a name.")).toBeVisible();

    await user.type(within(form).getByLabelText("Name"), "Arcade Stick P1");
    await user.selectOptions(within(form).getByLabelText("Kind"), "Arcade stick");
    // The providers of this system are offered.
    expect(
      within(within(form).getByLabelText("Provider"))
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Not set", "winhybrid", "dinput", "xinput", "sdljoy"]);
    // A binding needs a device id and its provider.
    await user.click(within(form).getByRole("radio", { name: /^Port\/order binding/ }));
    expect(
      within(form).getByText(
        "This controller is identified by USB port order. Swapping ports may change player assignments.",
      ),
    ).toBeVisible();
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(within(form).getByText("Enter the device ID reported by MAME.")).toBeVisible();
    await user.type(within(form).getByLabelText("Device ID"), "VID_D209&PID_1601");
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(within(form).getByText("Choose the provider the device ID comes from.")).toBeVisible();
    await user.selectOptions(within(form).getByLabelText("Provider"), "dinput");
    await user.click(within(form).getByRole("button", { name: "Save controller" }));
    expect(calls("create_controller")).toEqual([
      {
        controller: {
          name: "Arcade Stick P1",
          kind: "arcadeStick",
          deviceClass: "joystick",
          deviceId: "VID_D209&PID_1601",
          provider: "dinput",
          binding: "portOrder",
        },
      },
    ]);
    const list = await screen.findByRole("list", { name: "Controllers" });
    const item = within(list).getByRole("listitem", { name: /Arcade Stick P1/ });
    expect(within(item).getByText("VID_D209&PID_1601")).toBeVisible();
    expect(within(item).getByText("Port/order binding")).toBeVisible();
    expect(within(item).getByText(/Swapping ports may change player assignments/)).toBeVisible();
    // No connection state: devices are only detected on request.
    expect(within(item).queryByText(/connected/i)).not.toBeInTheDocument();

    await user.click(within(item).getByRole("button", { name: "Edit…" }));
    const edit = screen.getByRole("form", { name: "Edit Arcade Stick P1" });
    expect(within(edit).getByRole("radio", { name: /^Port\/order binding/ })).toBeChecked();
    await user.clear(within(edit).getByLabelText("Name"));
    await user.type(within(edit).getByLabelText("Name"), "Fight Stick");
    await user.clear(within(edit).getByLabelText("Device ID"));
    await user.click(within(edit).getByRole("radio", { name: /^Not set/ }));
    await user.click(within(edit).getByRole("button", { name: "Save controller" }));
    expect(calls("update_controller")).toEqual([
      {
        controllerId: "arcade-stick-p1",
        controller: {
          name: "Fight Stick",
          kind: "arcadeStick",
          deviceClass: "joystick",
          deviceId: null,
          provider: "dinput",
          binding: null,
        },
      },
    ]);
    const renamed = await within(list).findByRole("listitem", { name: /Fight Stick/ });

    await user.click(within(renamed).getByRole("button", { name: "Remove…" }));
    await user.click(within(renamed).getByRole("button", { name: "Cancel" }));
    await user.click(within(renamed).getByRole("button", { name: "Remove…" }));
    await user.click(within(renamed).getByRole("button", { name: "Remove controller" }));
    expect(calls("delete_controller")).toEqual([{ controllerId: "arcade-stick-p1" }]);
    expect(await screen.findByText(/No controller registered/)).toBeVisible();
    expect(controllers.view().controllers).toEqual([]);
  });

  it("explains that a controller stored as a player preference cannot be removed", async () => {
    backend({
      view: populated({
        assignments: {
          default: { profileId: "mame" },
          machines: { sf2: { players: ["stick-p1"] } },
          software: {},
          machineSoftware: {},
        },
      }),
    });
    const { user } = renderWith(<ControllersSettings />);
    const list = await screen.findByRole("list", { name: "Controllers" });
    const item = within(list).getByRole("listitem", { name: /Arcade Stick P1/ });
    expect(within(item).getByText("Assigned to players in:")).toBeVisible();
    await user.click(within(item).getByRole("button", { name: "Remove…" }));
    const confirm = within(item).getByRole("group", { name: "Remove controller" });
    expect(confirm).toHaveTextContent(/cannot be removed/);
    expect(within(confirm).getByText("Machine sf2")).toBeVisible();
    expect(
      within(confirm).queryByRole("button", { name: "Remove controller" }),
    ).not.toBeInTheDocument();

    // A reference added meanwhile: the backend refuses and lists it.
    const pad = within(list).getByRole("listitem", { name: /Pad/ });
    invokeMock.mockImplementationOnce(() =>
      Promise.reject(
        commandError("controllersInUse", {
          message: "the controller 'pad' is assigned in 1 assignment(s); remove it from them first",
          references: [{ level: "default" }],
        }),
      ),
    );
    await user.click(within(pad).getByRole("button", { name: "Remove…" }));
    await user.click(within(pad).getByRole("button", { name: "Remove controller" }));
    const alert = await within(pad).findByRole("alert");
    expect(alert).toHaveTextContent(/the controller 'pad' is assigned/);
    expect(within(alert).getByText("Default")).toBeVisible();
  });
});

describe("Settings → Controllers: default assignments", () => {
  it("changes the default controller profile", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<ControllersSettings />);
    await user.selectOptions(
      await screen.findByLabelText("Default controller profile"),
      "Gamepad Standard",
    );
    expect(calls("update_controller_assignment")).toEqual([
      { target: { level: "default" }, update: { profileId: { kind: "set", value: "gamepad" } } },
    ]);
    const item = await findProfile("Gamepad Standard");
    expect(await within(item).findByText("Default", { selector: ".badge" })).toBeVisible();
    expect(controllers.view().assignments.default).toEqual({ profileId: "gamepad" });
  });

  it("lists, finds and removes saved assignments, including names missing from catalogs", async () => {
    const controllers = backend({
      view: populated({
        assignments: {
          default: { profileId: "mame" },
          machines: { sf2: { profileId: "six-buttons", players: ["stick-p1", "pad"] } },
          software: {
            "nes:smb": { profileId: "gamepad" },
            "gone:missing": { profileId: "gamepad" },
          },
          machineSoftware: { "nes/nes:smb": { players: [] } },
        },
      }),
    });
    const { user } = renderWith(<ControllersSettings />);
    const list = await screen.findByRole("list", { name: "Saved assignments" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(4);
    expect(list).toHaveTextContent(
      "sf2 · Controller profile: Arcade Six Buttons · Players: Player 1: Arcade Stick P1, Player 2: Pad",
    );
    expect(list).toHaveTextContent("smb on nes · Controller profile inherited");
    expect(list).toHaveTextContent("gone:missing");

    await user.type(screen.getByLabelText("Find a saved assignment"), "smb");
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    await user.clear(screen.getByLabelText("Find a saved assignment"));

    await user.click(
      within(list).getByRole("button", { name: "Remove the assignment of Machine sf2" }),
    );
    expect(calls("reset_controller_assignment")).toEqual([
      { target: { level: "machine", key: "sf2" } },
    ]);
    await waitFor(() => {
      expect(within(list).getAllByRole("listitem")).toHaveLength(3);
    });
    expect(controllers.view().assignments.machines).toEqual({});
  });
});

function candidate(name: string): MachineCandidate {
  return {
    name,
    description: `${name} description`,
    year: "1985",
    manufacturer: "Nintendo",
    cloneOf: null,
    relation: "original",
    filters: [],
    filterMatch: true,
    devices: [
      {
        briefName: "cart",
        instanceName: "cartridge",
        deviceType: "cartridge",
        interfaces: ["nes_cart"],
      },
    ],
    defaultMedia: [{ device: "cart", part: "cart" }],
    launchable: true,
  };
}

function smbInfo(recommended: string | null = "nes"): SoftwareLaunchInfo {
  return {
    listName: "nes",
    name: "smb",
    description: "Super Mario Bros.",
    parts: [{ name: "cart", interface: "nes_cart", label: null, compatibility: null }],
    candidates: [candidate("nes"), candidate("nespal")],
    recommended,
  };
}

const controlsSelect = () => screen.getByRole("combobox", { name: "Controls" });

async function openControlsPanel(user: ReturnType<typeof userEvent.setup>, label: string) {
  await user.click(await screen.findByRole("button", { name: /^Advanced/ }));
  await user.click(screen.getByRole("button", { name: `Controls for ${label}` }));
  return screen.findByRole("group", { name: "Controls" });
}

describe("launch bar: Controls", () => {
  it("shows the effective controller profile and where it comes from", async () => {
    backend({
      view: populated({
        assignments: {
          default: { profileId: "mame" },
          machines: { sf2: { profileId: "six-buttons" } },
          software: {},
          machineSoftware: {},
        },
      }),
    });
    renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue("Arcade Six Buttons — specific to this machine");
    });
    // Launch profile and Controls are distinct selectors.
    expect(screen.getByRole("combobox", { name: "Launch profile" })).toBeVisible();
  });

  it("selects a temporary profile for the launch without saving it", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />, true);
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue(
        "MAME configuration — inherited from the default",
      );
    });
    await user.selectOptions(controlsSelect(), "Gamepad Standard");
    expect(screen.getByRole("group", { name: "Temporary controls" })).toHaveTextContent(
      "Controls: Gamepad Standard for the launches from this page only; nothing is saved.",
    );

    // The preview shows the plan of the temporary selection.
    await user.click(screen.getByRole("button", { name: /^Advanced/ }));
    const preview = await screen.findByLabelText("Command line");
    expect(
      await within(preview).findByText("Gamepad Standard — temporary for this launch"),
    ).toBeVisible();
    expect(calls("preview_launch").at(-1)).toMatchObject({
      request: { machine: "sf2", controls: { profileId: "gamepad" } },
    });

    await user.click(screen.getByRole("button", { name: "Play" }));
    expect(calls("start_launch")).toEqual([
      { request: { machine: "sf2", installationId: NEW_ID, controls: { profileId: "gamepad" } } },
    ]);
    // Nothing was saved.
    expect(calls("update_controller_assignment")).toHaveLength(0);
    expect(calls("set_controller_assignment")).toHaveLength(0);
    expect(controllers.view().assignments.machines).toEqual({});

    // Back to the saved preference: the request has no controls.
    await user.selectOptions(controlsSelect(), "MAME configuration — inherited from the default");
    expect(screen.queryByRole("group", { name: "Temporary controls" })).not.toBeInTheDocument();
    await waitFor(() => {
      expect(calls("preview_launch").at(-1)).toEqual({
        request: { machine: "sf2", installationId: NEW_ID },
      });
    });
  });

  it("saves a temporary selection as the machine preference on explicit request", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />, true);
    await waitFor(() => {
      expect(controlsSelect()).toBeEnabled();
    });
    await user.selectOptions(controlsSelect(), "Arcade Six Buttons");
    await user.click(screen.getByRole("button", { name: "Save as preference…" }));
    const form = screen.getByRole("form", { name: "Save as preference" });
    // A machine page offers the machine level only.
    expect(within(form).getAllByRole("radio")).toHaveLength(1);
    expect(within(form).getByRole("radio", { name: "This machine (sf2)" })).toBeChecked();
    await user.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(calls("update_controller_assignment")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Save as preference…" }));
    await user.click(screen.getByRole("button", { name: "Save preference" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "machine", key: "sf2" },
        update: { profileId: { kind: "set", value: "six-buttons" } },
      },
    ]);
    expect(
      await screen.findByText(
        "Arcade Six Buttons is saved as the controller profile of: This machine (sf2).",
      ),
    ).toBeVisible();
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue("Arcade Six Buttons — specific to this machine");
    });
    expect(controllers.view().assignments.machines).toEqual({ sf2: { profileId: "six-buttons" } });
    // The preview follows the saved preference.
    await user.click(screen.getByRole("button", { name: /^Advanced/ }));
    const preview = await screen.findByLabelText("Command line");
    expect(
      await within(preview).findByText("Arcade Six Buttons — specific to this machine"),
    ).toBeVisible();
    expect(
      within(preview).getByText("C:\\YMDB\\controller-profiles\\six-buttons\\base.cfg"),
    ).toBeVisible();
  });

  it("offers the software and machine + software levels on a software page", async () => {
    const controllers = backend({ view: populated(), info: smbInfo() });
    const { user } = renderWith(<SoftwareLaunchPanel list="nes" name="smb" refreshKey={0} />);
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue(
        "MAME configuration — inherited from the default",
      );
    });
    await user.selectOptions(controlsSelect(), "Gamepad Standard");
    await user.click(screen.getByRole("button", { name: "Save as preference…" }));
    const form = screen.getByRole("form", { name: "Save as preference" });
    expect(
      within(form)
        .getAllByRole("radio")
        .map((radio) => radio.parentElement?.textContent),
    ).toEqual([
      "This software (nes:smb), on any machine",
      "This software on nes only",
      "The machine nes, for everything it runs",
    ]);
    await user.click(within(form).getByRole("radio", { name: "This software on nes only" }));
    await user.click(within(form).getByRole("button", { name: "Save preference" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "machineSoftware", key: "nes/nes:smb" },
        update: { profileId: { kind: "set", value: "gamepad" } },
      },
    ]);
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue(
        "Gamepad Standard — specific to this software on this machine",
      );
    });
    expect(controllers.view().assignments.machineSoftware).toEqual({
      "nes/nes:smb": { profileId: "gamepad" },
    });

    // On another machine, the machine + software preference does not apply.
    await user.selectOptions(
      screen.getByLabelText("Machine"),
      "nespal description (nespal) — original list",
    );
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue(
        "MAME configuration — inherited from the default",
      );
    });
  });

  it("never creates a machine + software preference without a machine", async () => {
    backend({ view: populated(), info: smbInfo(null) });
    const { user } = renderWith(<SoftwareLaunchPanel list="nes" name="smb" refreshKey={0} />);
    const panel = await openControlsPanel(user, "this software");
    expect(
      within(panel).getByText(/Choose a machine to see the effective controller profile/),
    ).toBeVisible();
    expect(
      within(panel)
        .getAllByRole("group")
        .map((group) => group.getAttribute("aria-label")),
    ).toEqual([
      "This software (nes:smb), on any machine",
      "Players for This software (nes:smb), on any machine",
    ]);
    await user.selectOptions(
      within(panel).getByLabelText(
        "Controller profile for This software (nes:smb), on any machine",
      ),
      "Gamepad Standard",
    );
    await user.click(within(panel).getByRole("button", { name: "Save" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "software", key: "nes:smb" },
        update: { profileId: { kind: "set", value: "gamepad" } },
      },
    ]);
    expect(calls("resolve_controls")).toHaveLength(0);
  });

  it("ignores the resolution of a machine that is no longer selected", async () => {
    const slow = deferred<ControlsResolution>();
    const controllers = backend({
      view: populated({
        assignments: {
          default: { profileId: "mame" },
          machines: { nespal: { profileId: "gamepad" } },
          software: {},
          machineSoftware: {},
        },
      }),
      info: smbInfo(),
    });
    invokeMock.mockImplementation((command: string, args?: Args) => {
      if (command === "resolve_controls" && args?.machine === "nes") return slow.promise;
      const handler = (controllers.handlers as Partial<Record<string, Handler>>)[command];
      if (command === "get_software_launch_info") return Promise.resolve(smbInfo());
      if (command === "get_launch_config")
        return Promise.resolve(launchConfigView(launchConfig(), NEW_ID));
      if (command === "get_launch_sessions") return Promise.resolve([]);
      if (command === "get_download_status") return Promise.resolve(null);
      return handler ? Promise.resolve(handler(args ?? {})) : Promise.reject(new Error(command));
    });
    const { user } = renderWith(<SoftwareLaunchPanel list="nes" name="smb" refreshKey={0} />);
    await user.selectOptions(
      await screen.findByLabelText("Machine"),
      "nespal description (nespal) — original list",
    );
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue("Gamepad Standard — inherited from the machine");
    });
    // The answer for the machine selected first arrives last.
    slow.resolve({
      profileId: "six-buttons",
      profileSource: "machine",
      players: [],
      playersSource: null,
      profileName: "Arcade Six Buttons",
      levels: [],
      cfgDirectory: null,
    });
    await new Promise((done) => setTimeout(done, 20));
    expect(controlsSelect()).toHaveDisplayValue("Gamepad Standard — inherited from the machine");
  });

  it("falls back to the saved preference when the temporary profile is deleted", async () => {
    const controllers = backend({ view: populated() });
    const { user } = renderWith(
      <>
        <MachineLaunchPanel name="sf2" isRunnable />
        <ControllersSettings />
      </>,
    );
    await waitFor(() => {
      expect(controlsSelect()).toBeEnabled();
    });
    await user.selectOptions(controlsSelect(), "Gamepad Standard");
    const item = await findProfile("Gamepad Standard");
    await user.click(within(item).getByRole("button", { name: "Delete…" }));
    await user.click(within(item).getByRole("button", { name: "Delete profile" }));
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue(
        "MAME configuration — inherited from the default",
      );
    });
    expect(screen.queryByRole("group", { name: "Temporary controls" })).not.toBeInTheDocument();
    expect(controllers.view().profiles).toHaveLength(2);
  });
});

describe("Advanced → Controls", () => {
  it("shows the effective profile, its origin and the saved preferences", async () => {
    backend({
      view: populated({
        assignments: {
          default: { profileId: "gamepad" },
          machines: { nes: { players: ["pad"] } },
          software: { "nes:smb": { profileId: "six-buttons" } },
          machineSoftware: {},
        },
      }),
      info: smbInfo(),
    });
    const { user } = renderWith(<SoftwareLaunchPanel list="nes" name="smb" refreshKey={0} />);
    const panel = await openControlsPanel(user, "this software");
    await waitFor(() => {
      expect(panel).toHaveTextContent("Arcade Six Buttons — specific to this software");
    });
    expect(panel).toHaveTextContent(/created when MAME first starts with this profile/);
    expect(panel).toHaveTextContent("Player 1: Pad — inherited from the machine");
    const software = within(panel).getByRole("group", {
      name: "This software (nes:smb), on any machine",
    });
    expect(software).toHaveTextContent(
      "This software (nes:smb), on any machine: Arcade Six Buttons",
    );
    expect(within(software).getByRole("button", { name: "Reset to inherited" })).toBeVisible();
    const machineSoftware = within(panel).getByRole("group", { name: "This software on nes only" });
    expect(machineSoftware).toHaveTextContent("not set, inherited");
    expect(
      within(machineSoftware).queryByRole("button", { name: "Reset to inherited" }),
    ).not.toBeInTheDocument();
  });

  it("resets one level to inherited and recomputes the effective profile", async () => {
    const controllers = backend({
      view: populated({
        assignments: {
          default: { profileId: "gamepad" },
          machines: { sf2: { profileId: "six-buttons", players: ["stick-p1"] } },
          software: {},
          machineSoftware: {},
        },
      }),
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControlsPanel(user, "this machine");
    const machine = await within(panel).findByRole("group", { name: "This machine (sf2)" });
    await waitFor(() => {
      expect(machine).toHaveTextContent("This machine (sf2): Arcade Six Buttons");
    });
    await user.click(within(machine).getByRole("button", { name: "Reset to inherited" }));
    // Only the profile field changes: the stored players are kept.
    expect(calls("update_controller_assignment")).toEqual([
      { target: { level: "machine", key: "sf2" }, update: { profileId: { kind: "inherit" } } },
    ]);
    expect(controllers.view().assignments.machines).toEqual({ sf2: { players: ["stick-p1"] } });
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue("Gamepad Standard — inherited from the default");
    });
    const updated = within(screen.getByRole("group", { name: "Controls" })).getByRole("group", {
      name: "This machine (sf2)",
    });
    expect(updated).toHaveTextContent("not set, inherited");
    expect(updated).toHaveTextContent(/Players: Player 1: Arcade Stick P1/);
  });

  it("changes a preference and keeps the players stored at the same level", async () => {
    const controllers = backend({
      view: populated({
        assignments: {
          default: { profileId: "mame" },
          machines: { sf2: { players: ["stick-p1"] } },
          software: {},
          machineSoftware: {},
        },
      }),
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControlsPanel(user, "this machine");
    const machine = await within(panel).findByRole("group", { name: "This machine (sf2)" });
    const select = within(machine).getByLabelText("Controller profile for This machine (sf2)");
    expect(within(machine).getByRole("button", { name: "Save" })).toBeDisabled();
    await user.selectOptions(select, "Gamepad Standard");
    await user.click(within(machine).getByRole("button", { name: "Save" }));
    expect(calls("update_controller_assignment")).toEqual([
      {
        target: { level: "machine", key: "sf2" },
        update: { profileId: { kind: "set", value: "gamepad" } },
      },
    ]);
    expect(controllers.view().assignments.machines).toEqual({
      sf2: { profileId: "gamepad", players: ["stick-p1"] },
    });
    await waitFor(() => {
      expect(controlsSelect()).toHaveDisplayValue("Gamepad Standard — specific to this machine");
    });
  });

  it("reports a refused change and keeps the previous preference", async () => {
    backend({
      view: populated(),
      overrides: {
        update_controller_assignment: () => {
          throw commandError("invalidRequest", {
            message: "the controller profile 'gamepad' does not exist",
          });
        },
      },
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControlsPanel(user, "this machine");
    const machine = await within(panel).findByRole("group", { name: "This machine (sf2)" });
    await user.selectOptions(
      within(machine).getByLabelText("Controller profile for This machine (sf2)"),
      "Gamepad Standard",
    );
    await user.click(within(machine).getByRole("button", { name: "Save" }));
    expect(await within(machine).findByRole("alert")).toHaveTextContent(
      "Invalid request: the controller profile 'gamepad' does not exist",
    );
    expect(machine).toHaveTextContent("not set, inherited");
  });
});

describe("Configure Controls in MAME", () => {
  it("explains the MAME menus, then starts the normal launch with the effective profile", async () => {
    backend({
      view: populated({
        assignments: {
          default: { profileId: "six-buttons" },
          machines: {},
          software: {},
          machineSoftware: {},
        },
      }),
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControlsPanel(user, "this machine");
    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: "Configure Controls in MAME…" }),
      ).toBeEnabled();
    });
    await user.click(within(panel).getByRole("button", { name: "Configure Controls in MAME…" }));
    const dialog = screen.getByRole("dialog", { name: "Configure Controls in MAME" });
    expect(dialog).toHaveTextContent(
      "Controller profile: Arcade Six Buttons (inherited from the default).",
    );
    expect(dialog).toHaveTextContent(/normally Tab, but it may have been remapped/);
    expect(dialog).toHaveTextContent("Input Settings → Input Assignments (this system)");
    expect(dialog).toHaveTextContent("Input Assignments (general)");
    expect(dialog).toHaveTextContent(/saves the changes when it exits normally/);
    expect(dialog).toHaveTextContent(/the changes of the session are not saved/);
    expect(dialog).toHaveTextContent(/own configuration directory/);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("start_launch")).toHaveLength(0);

    await user.click(within(panel).getByRole("button", { name: "Configure Controls in MAME…" }));
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Start MAME" }),
    );
    expect(calls("start_launch")).toEqual([
      { request: { machine: "sf2", installationId: NEW_ID } },
    ]);
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("uses the temporary selection and warns about MAME's own configuration", async () => {
    backend({ view: populated(), info: smbInfo() });
    const { user } = renderWith(<SoftwareLaunchPanel list="nes" name="smb" refreshKey={0} />);
    const panel = await openControlsPanel(user, "this software");
    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: "Configure Controls in MAME…" }),
      ).toBeEnabled();
    });
    await user.click(within(panel).getByRole("button", { name: "Configure Controls in MAME…" }));
    let dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(
      "MAME configuration uses MAME's own configuration directories",
    );
    expect(dialog).toHaveTextContent("YMDB starts nes:smb on nes with this controller profile");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await user.selectOptions(controlsSelect(), "Gamepad Standard");
    await user.click(within(panel).getByRole("button", { name: "Configure Controls in MAME…" }));
    dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(
      "Controller profile: Gamepad Standard (temporary for this launch).",
    );
    await user.click(within(dialog).getByRole("button", { name: "Start MAME" }));
    expect(calls("start_launch")).toEqual([
      {
        request: {
          machine: "nes",
          software: { list: "nes", name: "smb", media: [{ device: "cart", part: "cart" }] },
          installationId: NEW_ID,
          controls: { profileId: "gamepad" },
        },
      },
    ]);
  });

  it("keeps the dialog open with the error when MAME cannot start", async () => {
    backend({
      view: populated(),
      overrides: {
        start_launch: () => {
          throw commandError("launchFailed", {
            message: "the controller profile cannot be prepared: access denied",
          });
        },
      },
    });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const panel = await openControlsPanel(user, "this machine");
    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: "Configure Controls in MAME…" }),
      ).toBeEnabled();
    });
    await user.click(within(panel).getByRole("button", { name: "Configure Controls in MAME…" }));
    await user.click(screen.getByRole("button", { name: "Start MAME" }));
    expect(await within(screen.getByRole("dialog")).findByRole("alert")).toHaveTextContent(
      "MAME could not be started: the controller profile cannot be prepared: access denied",
    );
  });
});

describe("launch warnings", () => {
  it("shows the warnings of the plan the session runs", async () => {
    backend({ view: populated() });
    const { user } = renderWith(<MachineLaunchPanel name="sf2" isRunnable />);
    const play = await screen.findByRole("button", { name: "Play" });
    await waitFor(() => {
      expect(play).toBeEnabled();
    });
    await user.click(play);
    expect(
      await screen.findByText(
        /Another running MAME session uses the controller profile 'Arcade Six Buttons'/,
      ),
    ).toBeVisible();
  });
});

const css = readFileSync("src/features/controllers/controllers.css", "utf8");
const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

describe("controllers styles", () => {
  it("use only theme tokens defined by both themes", () => {
    expect(css).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
    const tokens = new Set(css.match(/--color-[a-z-]+/g));
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
