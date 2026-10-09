import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  DetectedInstallation,
  MameConfiguration,
  ValidationReport,
  ValidationStatus,
} from "./mameApi";
import { MameInstallation } from "./MameInstallation";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const MAME_PATH = "C:\\Emulators\\MAME 0.280\\mame.exe";
const OTHER_PATH = "C:\\Tools\\other.exe";

type Handler = (args?: Record<string, unknown>) => unknown;

/** Routes mocked IPC calls by command name. */
function mockCommands(handlers: Record<string, Handler>) {
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    const handler = handlers[command];
    if (!handler) {
      return Promise.reject(new Error(`unexpected command ${command}`));
    }
    return Promise.resolve(handler(args));
  });
}

function configuration(path: string | null, exists = path !== null): MameConfiguration {
  return { executablePath: path, executableExists: exists };
}

function validReport(path = MAME_PATH): ValidationReport {
  return {
    executablePath: path,
    status: { kind: "valid" },
    version: { release: "0.280", revision: "mame0280" },
    output: { exitCode: 0, stdout: "0.280 (mame0280)\r\n", stderr: "" },
    checkedAt: Date.UTC(2026, 9, 9, 12, 0, 0),
  };
}

function invalidReport(path: string, status: ValidationStatus): ValidationReport {
  return { executablePath: path, status, version: null, output: null, checkedAt: 1 };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

async function renderLoaded() {
  const user = userEvent.setup();
  render(<MameInstallation />);
  await screen.findByLabelText("MAME configuration");
  return user;
}

function details() {
  return within(screen.getByLabelText("MAME configuration"));
}

describe("MameInstallation", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("shows a loading state, then the unconfigured state", async () => {
    mockCommands({ get_mame_configuration: () => configuration(null) });
    render(<MameInstallation />);

    expect(screen.getByText("Loading configuration…")).toBeInTheDocument();
    expect(await screen.findByText("No MAME executable configured.")).toBeInTheDocument();
    expect(details().getByText("Not configured")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Verify" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove configuration" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Select executable…" })).toBeEnabled();
  });

  it("renders the saved configuration without running MAME", async () => {
    mockCommands({ get_mame_configuration: () => configuration(MAME_PATH) });
    await renderLoaded();

    expect(details().getByText(MAME_PATH)).toBeInTheDocument();
    expect(details().getByText("Not verified yet.")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("warns when the configured executable no longer exists", async () => {
    mockCommands({ get_mame_configuration: () => configuration(MAME_PATH, false) });
    await renderLoaded();

    expect(details().getByText(/configured executable no longer exists/)).toBeInTheDocument();
  });

  it("selects, validates and saves an executable", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(null),
      pick_mame_executable: () => MAME_PATH,
      set_mame_executable: () => validReport(),
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Select executable…" }));

    expect(await details().findByText("MAME 0.280 (mame0280)")).toBeInTheDocument();
    expect(details().getByText(MAME_PATH)).toBeInTheDocument();
    expect(details().getByText("Valid MAME installation.")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("set_mame_executable", { path: MAME_PATH });
    expect(screen.getByRole("button", { name: "Verify" })).toBeEnabled();
  });

  it("does nothing when the file picker is cancelled", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(null),
      pick_mame_executable: () => null,
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Select executable…" }));

    expect(await screen.findByRole("button", { name: "Select executable…" })).toBeEnabled();
    expect(invokeMock).not.toHaveBeenCalledWith("set_mame_executable", expect.anything());
    expect(details().getByText("Not configured")).toBeInTheDocument();
  });

  it("reports a rejected selection and keeps the previous configuration", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(MAME_PATH),
      pick_mame_executable: () => OTHER_PATH,
      set_mame_executable: () => invalidReport(OTHER_PATH, { kind: "unrecognizedOutput" }),
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Select executable…" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The selected file was not saved");
    expect(alert).toHaveTextContent("does not appear to be MAME");
    expect(alert).toHaveTextContent(OTHER_PATH);
    expect(details().getByText(MAME_PATH)).toBeInTheDocument();
  });

  it("verifies the configured executable and shows the version and raw output", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(MAME_PATH),
      verify_mame_configuration: () => validReport(),
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Verify" }));

    expect(await details().findByText("MAME 0.280 (mame0280)")).toBeInTheDocument();
    expect(details().getByText("Last checked")).toBeInTheDocument();
    expect(screen.getByText("Version command output")).toBeInTheDocument();
    expect(screen.getByText(/0\.280 \(mame0280\)/, { selector: "pre" })).toBeInTheDocument();
  });

  it("shows validation errors of the configured executable", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(MAME_PATH),
      verify_mame_configuration: () =>
        invalidReport(MAME_PATH, { kind: "timedOut", timeoutMs: 20000 }),
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Verify" }));

    expect(
      await details().findByText("The program did not respond within 20 seconds and was stopped."),
    ).toBeInTheDocument();
    expect(details().getByText("Unknown")).toBeInTheDocument();
  });

  it("shows settings errors without blocking other actions", async () => {
    // Tauri rejects with the serialized `CommandError`, not with an `Error`.
    invokeMock.mockRejectedValueOnce({
      kind: "settings",
      message: "settings file 'settings.json' is invalid",
    });
    await renderLoaded();

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Settings error: settings file 'settings.json' is invalid",
    );
    expect(screen.getByRole("button", { name: "Select executable…" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Detect installations" })).toBeEnabled();
  });

  it("disables actions while an operation is in progress", async () => {
    const verification = deferred<ValidationReport>();
    mockCommands({
      get_mame_configuration: () => configuration(MAME_PATH),
      verify_mame_configuration: () => verification.promise,
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Verify" }));

    expect(screen.getByRole("button", { name: "Verifying…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Select executable…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Detect installations" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove configuration" })).toBeDisabled();

    verification.resolve(validReport());

    expect(await screen.findByRole("button", { name: "Verify" })).toBeEnabled();
  });

  it("removes the configuration", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(MAME_PATH),
      clear_mame_configuration: () => null,
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Remove configuration" }));

    expect(await screen.findByText("No MAME executable configured.")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("clear_mame_configuration", undefined);
    expect(screen.getByRole("button", { name: "Remove configuration" })).toBeDisabled();
  });

  it("lists detected installations and uses a valid one", async () => {
    const detected: DetectedInstallation[] = [
      { source: "path", report: validReport() },
      { source: "conventional", report: invalidReport(OTHER_PATH, { kind: "notExecutable" }) },
    ];
    mockCommands({
      get_mame_configuration: () => configuration(null),
      detect_mame_installations: () => detected,
      set_mame_executable: () => validReport(),
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Detect installations" }));

    const list = await screen.findByRole("list", { name: "Detected installations" });
    expect(within(list).getByText(MAME_PATH)).toBeInTheDocument();
    expect(within(list).getByText(/Found in PATH · MAME 0\.280/)).toBeInTheDocument();
    expect(
      within(list).getByText(/Standard location · The file is not an executable program/),
    ).toBeInTheDocument();
    expect(within(list).getAllByRole("button")).toHaveLength(1);

    await user.click(within(list).getByRole("button", { name: `Use ${MAME_PATH}` }));

    expect(await details().findByText("Valid MAME installation.")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("set_mame_executable", { path: MAME_PATH });
    expect(within(list).getByText("In use")).toBeInTheDocument();
  });

  it("explains when detection finds nothing", async () => {
    mockCommands({
      get_mame_configuration: () => configuration(null),
      detect_mame_installations: () => [],
    });
    const user = await renderLoaded();

    await user.click(screen.getByRole("button", { name: "Detect installations" }));

    expect(await screen.findByText(/No MAME executable was found/)).toBeInTheDocument();
  });
});
