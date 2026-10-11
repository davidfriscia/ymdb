/// <reference types="node" />
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { NEW_ID, OLD_ID, installation } from "../../test/installationFixtures";
import {
  BACKUP_ID,
  FINGERPRINT,
  NAS_ID,
  OTHER_FINGERPRINT,
  connection,
  remoteSourcesView,
} from "../../test/remoteFixtures";

import type {
  ConnectionInput,
  ConnectionSummary,
  ConnectionTestResult,
  RemoteListing,
  RemoteSourcesConfig,
} from "./remoteApi";
import { RemoteConnectionsSettings } from "./RemoteConnectionsSettings";
import { RomManagerSection } from "./RomManagerSection";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;

/** Rejection with a Tauri `CommandError` payload. */
function commandError(kind: string, fields: Record<string, unknown> = {}): Error {
  return Object.assign(new Error(kind), { kind, ...fields });
}

interface Backend {
  connections: ConnectionSummary[];
  /** Results returned by successive tests, then success. */
  tests: ConnectionTestResult[];
  /** References returned when removing a connection that is in use. */
  inUse: Set<string>;
  listings: Record<string, RemoteListing>;
  sources: Record<string, RemoteSourcesConfig>;
  picks: (string | null)[];
  calls: { command: string; args?: Args }[];
}

/** A stateful fake of the remote commands. */
function remoteBackend(initial: Partial<Backend> = {}): Backend {
  const backend: Backend = {
    connections: [],
    tests: [],
    inUse: new Set(),
    listings: {},
    sources: {},
    picks: [],
    calls: [],
    ...initial,
  };
  const view = () => ({ connections: backend.connections, loadError: null });
  invokeMock.mockImplementation((command: string, args?: Args) => {
    backend.calls.push({ command, args });
    switch (command) {
      case "get_remote_connections":
        return Promise.resolve(view());
      case "save_remote_connection": {
        const input = (args as { connection: ConnectionInput }).connection;
        const id = input.id ?? BACKUP_ID;
        const saved = connection(id, input.name, input.providerConfig);
        backend.connections = input.id
          ? backend.connections.map((entry) => (entry.id === id ? saved : entry))
          : [...backend.connections, saved];
        return Promise.resolve(view());
      }
      case "remove_remote_connection": {
        const id = (args as { connectionId: string }).connectionId;
        if (backend.inUse.has(id)) {
          return Promise.reject(
            commandError("connectionInUse", {
              references: [
                {
                  installationId: NEW_ID,
                  installationLabel: "MAME 0.280",
                  category: "machineRoms",
                },
              ],
            }),
          );
        }
        backend.connections = backend.connections.filter((entry) => entry.id !== id);
        return Promise.resolve(view());
      }
      case "test_remote_connection":
        return Promise.resolve(
          backend.tests.shift() ?? {
            status: "success",
            info: {
              homeDirectory: "/",
              capabilities: { listDirectory: true, rangedDownload: true },
            },
          },
        );
      case "approve_remote_host_key":
      case "forget_remote_host_key":
        return Promise.resolve(view());
      case "browse_remote_directory": {
        const path = (args as { path: string }).path;
        const listing = backend.listings[path];
        return listing
          ? Promise.resolve(listing)
          : Promise.reject(commandError("remote", { error: { kind: "notFound", path } }));
      }
      case "pick_private_key_file":
      case "pick_destination_directory":
        return Promise.resolve(backend.picks.shift() ?? null);
      case "get_remote_sources": {
        const id = (args as { installationId: string }).installationId;
        return Promise.resolve(remoteSourcesView(id, backend.sources[id], backend.connections));
      }
      case "save_remote_sources": {
        const { installationId, config } = args as {
          installationId: string;
          config: RemoteSourcesConfig;
        };
        backend.sources[installationId] = config;
        return Promise.resolve(remoteSourcesView(installationId, config, backend.connections));
      }
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  });
  return backend;
}

const callsOf = (backend: Backend, command: string) =>
  backend.calls.filter((call) => call.command === command);

async function findConnection(name: string) {
  return within((await screen.findByText(name, { selector: "p" })).closest("li") as HTMLElement);
}

describe("RemoteConnectionsSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("lists connections with their server, key and identity", async () => {
    remoteBackend({
      connections: [
        connection(NAS_ID, "Home NAS", {
          approvedHostKey: {
            algorithm: "ssh-ed25519",
            fingerprint: FINGERPRINT,
            publicKey: "ssh-ed25519 AAAA",
            approvedAt: 1,
          },
        }),
        connection(BACKUP_ID, "Backup", { port: 2222, hostKeyVerification: "knownHosts" }, false),
      ],
    });
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    expect(nas.getByText("SFTP")).toBeInTheDocument();
    expect(nas.getByText("mame@nas.local:22")).toBeInTheDocument();
    expect(nas.getByText(FINGERPRINT)).toBeInTheDocument();
    const backup = await findConnection("Backup");
    expect(backup.getByText("mame@nas.local:2222")).toBeInTheDocument();
    expect(backup.getByText("File not found")).toBeInTheDocument();
    expect(backup.getByText("Not approved yet")).toBeInTheDocument();
    expect(backup.getByText(/OpenSSH known_hosts first/)).toBeInTheDocument();
    // Only SFTP is offered: no control suggests other protocols.
    expect(screen.queryByText(/HTTPS|S3/)).not.toBeInTheDocument();
  });

  it("validates and creates a connection with the native key picker", async () => {
    const backend = remoteBackend({ picks: ["C:\\Users\\me\\.ssh\\id_nas ✓"] });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    await user.click(await screen.findByRole("button", { name: "Add connection…" }));
    const form = within(screen.getByRole("form", { name: "New remote connection" }));
    await user.clear(form.getByLabelText("Port"));
    await user.type(form.getByLabelText("Port"), "70000");
    await user.click(form.getByRole("button", { name: "Save connection" }));
    expect(form.getByText("Enter a name.")).toBeInTheDocument();
    expect(form.getByText("Enter a host name or IP address.")).toBeInTheDocument();
    expect(form.getByText("Enter a port from 1 to 65535.")).toBeInTheDocument();
    expect(form.getByText("Enter the user name.")).toBeInTheDocument();
    expect(form.getByText("Choose the private key file.")).toBeInTheDocument();
    expect(callsOf(backend, "save_remote_connection")).toHaveLength(0);

    await user.type(form.getByLabelText("Connection name"), "Home NAS");
    await user.type(form.getByLabelText("Host"), "user@nas");
    await user.click(form.getByRole("button", { name: "Save connection" }));
    expect(form.getByText(/Enter only the host name/)).toBeInTheDocument();
    await user.clear(form.getByLabelText("Host"));
    await user.type(form.getByLabelText("Host"), "192.168.1.10");
    await user.clear(form.getByLabelText("Port"));
    await user.type(form.getByLabelText("Port"), "2222");
    await user.type(form.getByLabelText("Username"), "mame");
    await user.click(form.getByRole("button", { name: "Choose…" }));
    expect(form.getByLabelText("Private key file")).toHaveValue("C:\\Users\\me\\.ssh\\id_nas ✓");
    await user.click(form.getByLabelText("Use OpenSSH known_hosts, then approval in YMDB"));
    await user.click(form.getByRole("button", { name: "Save connection" }));

    await screen.findByText("mame@192.168.1.10:2222");
    expect(callsOf(backend, "save_remote_connection")[0]?.args).toEqual({
      connection: {
        id: null,
        name: "Home NAS",
        providerType: "sftp",
        providerConfig: {
          host: "192.168.1.10",
          port: 2222,
          username: "mame",
          privateKeyPath: "C:\\Users\\me\\.ssh\\id_nas ✓",
          hostKeyVerification: "knownHosts",
        },
      },
    });
  });

  it("edits a connection", async () => {
    const backend = remoteBackend({ connections: [connection(NAS_ID, "Home NAS")] });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    await user.click((await findConnection("Home NAS")).getByRole("button", { name: "Edit…" }));
    const form = within(screen.getByRole("form", { name: "Edit Home NAS" }));
    expect(form.getByLabelText("Host")).toHaveValue("nas.local");
    await user.clear(form.getByLabelText("Connection name"));
    await user.type(form.getByLabelText("Connection name"), "Attic NAS");
    await user.click(form.getByRole("button", { name: "Save connection" }));
    expect(await findConnection("Attic NAS")).toBeTruthy();
    const input = callsOf(backend, "save_remote_connection")[0]?.args?.connection as
      ConnectionInput | undefined;
    expect(input?.id).toBe(NAS_ID);
    expect(input?.name).toBe("Attic NAS");
  });

  it("refuses to remove a connection used by remote sources", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS"), connection(BACKUP_ID, "Backup")],
      inUse: new Set([NAS_ID]),
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Remove…" }));
    await user.click(nas.getByRole("button", { name: "Remove connection" }));
    expect(await nas.findByRole("alert")).toHaveTextContent(
      "used by the ROM Manager of MAME 0.280 (Machine ROMs)",
    );
    expect(screen.getByText("Home NAS", { selector: "p" })).toBeInTheDocument();

    const backup = await findConnection("Backup");
    await user.click(backup.getByRole("button", { name: "Remove…" }));
    await user.click(backup.getByRole("button", { name: "Remove connection" }));
    await waitFor(() => {
      expect(screen.queryByText("Backup", { selector: "p" })).not.toBeInTheDocument();
    });
    expect(backend.connections.map((entry) => entry.id)).toEqual([NAS_ID]);
  });

  it("tests a connection and reports success and failures", async () => {
    remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      tests: [{ status: "failure", error: { kind: "authenticationFailed" } }],
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Test Connection" }));
    expect(await nas.findByRole("alert")).toHaveTextContent(
      "The server refused the private key for this user.",
    );
    await user.click(nas.getByRole("button", { name: "Test Connection" }));
    expect(await nas.findByText(/Connection successful/)).toHaveTextContent("start directory /");
  });

  it("asks to approve an unknown server identity, then connects", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      tests: [
        {
          status: "failure",
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
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Test Connection" }));
    const approval = within(await nas.findByRole("group", { name: "Approve server identity" }));
    expect(approval.getByText(FINGERPRINT)).toBeInTheDocument();
    expect(approval.getByText("ssh-ed25519")).toBeInTheDocument();
    await user.click(approval.getByRole("button", { name: "Approve and connect" }));
    expect(await nas.findByText(/Connection successful/)).toBeInTheDocument();
    expect(callsOf(backend, "approve_remote_host_key")[0]?.args).toEqual({
      connectionId: NAS_ID,
      fingerprint: FINGERPRINT,
    });
    expect(callsOf(backend, "test_remote_connection")).toHaveLength(2);
  });

  it("blocks a changed server identity without offering approval", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      tests: [
        {
          status: "failure",
          error: {
            kind: "hostKeyChanged",
            host: "nas.local",
            port: 22,
            algorithm: "ssh-ed25519",
            fingerprint: OTHER_FINGERPRINT,
            expectedFingerprint: FINGERPRINT,
            source: "ymdb",
          },
        },
      ],
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Test Connection" }));
    const alert = within(await nas.findByRole("alert"));
    expect(alert.getByText(/identity of nas.local:22 changed/)).toBeInTheDocument();
    expect(alert.getByText(OTHER_FINGERPRINT)).toBeInTheDocument();
    expect(alert.getByText(FINGERPRINT)).toBeInTheDocument();
    expect(nas.queryByRole("button", { name: /Approve/ })).not.toBeInTheDocument();
    // Forgetting the old identity is an explicit, confirmed action.
    await user.click(alert.getByRole("button", { name: "Forget approved identity…" }));
    await user.click(alert.getByRole("button", { name: "Forget approved identity" }));
    await waitFor(() => {
      expect(callsOf(backend, "forget_remote_host_key")).toHaveLength(1);
    });
    expect(callsOf(backend, "approve_remote_host_key")).toHaveLength(0);
  });

  it("asks for the passphrase of a protected key and never keeps it", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      tests: [
        { status: "failure", error: { kind: "passphraseRequired" } },
        { status: "failure", error: { kind: "passphraseIncorrect" } },
      ],
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Test Connection" }));
    let prompt = within(await nas.findByRole("form", { name: "Private key passphrase" }));
    await user.type(prompt.getByLabelText("Passphrase"), "wrong");
    await user.click(prompt.getByRole("button", { name: "Connect" }));
    prompt = within(await nas.findByRole("form", { name: "Private key passphrase" }));
    expect(await prompt.findByText(/The passphrase is not correct/)).toBeInTheDocument();
    expect(prompt.getByLabelText("Passphrase")).toHaveValue("");
    await user.type(prompt.getByLabelText("Passphrase"), "correct horse");
    await user.click(prompt.getByRole("button", { name: "Connect" }));
    expect(await nas.findByText(/Connection successful/)).toBeInTheDocument();
    expect(callsOf(backend, "test_remote_connection").map((call) => call.args?.passphrase)).toEqual(
      [null, "wrong", "correct horse"],
    );
    expect(nas.queryByLabelText("Passphrase")).not.toBeInTheDocument();
  });

  it("browses remote directories", async () => {
    remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      listings: {
        "": {
          path: "/volume1",
          parent: "/",
          entries: [
            { name: "mame", kind: "directory", size: null, modifiedAt: null },
            { name: "readme.txt", kind: "file", size: 2048, modifiedAt: null },
          ],
          truncated: false,
          capabilities: { listDirectory: true, rangedDownload: true },
        },
        "/volume1/mame": {
          path: "/volume1/mame",
          parent: "/volume1",
          entries: [{ name: "roms", kind: "directory", size: null, modifiedAt: null }],
          truncated: false,
          capabilities: { listDirectory: true, rangedDownload: true },
        },
      },
    });
    const user = userEvent.setup();
    render(<RemoteConnectionsSettings />);
    const nas = await findConnection("Home NAS");
    await user.click(nas.getByRole("button", { name: "Browse…" }));
    const browser = within(nas.getByRole("group", { name: "Remote directories" }));
    expect(await browser.findByText("readme.txt")).toBeInTheDocument();
    await user.click(browser.getByRole("button", { name: "mame/" }));
    expect(await browser.findByRole("button", { name: "roms/" })).toBeInTheDocument();
    expect(browser.getByLabelText("Remote path")).toHaveValue("/volume1/mame");
    await user.clear(browser.getByLabelText("Remote path"));
    await user.type(browser.getByLabelText("Remote path"), "/missing{Enter}");
    expect(await browser.findByRole("alert")).toHaveTextContent(
      "/missing does not exist on the server.",
    );
  });
});

const newInstallation = installation(NEW_ID, "0.280");
const oldInstallation = installation(OLD_ID, "0.252");

function sourcesCalls(backend: Backend) {
  return callsOf(backend, "save_remote_sources").map(
    (call) => call.args as { installationId: string; config: RemoteSourcesConfig },
  );
}

async function findCategory(label: string) {
  return within(await screen.findByRole("group", { name: label }));
}

describe("RomManagerSection", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("shows five independent categories with the global connections", async () => {
    remoteBackend({
      connections: [connection(NAS_ID, "Home NAS"), connection(BACKUP_ID, "Backup")],
    });
    render(<RomManagerSection installation={newInstallation} />);
    for (const label of [
      "Machine ROMs",
      "Machine CHDs",
      "Software List ROMs",
      "Software List CHDs",
      "Samples",
    ]) {
      expect(await screen.findByRole("group", { name: label })).toBeInTheDocument();
    }
    const roms = await findCategory("Machine ROMs");
    const options = within(roms.getByLabelText("Remote connection"))
      .getAllByRole("option")
      .map((option) => option.textContent);
    expect(options).toEqual(["Not set", "Home NAS (SFTP)", "Backup (SFTP)"]);
    expect(
      within(roms.getByLabelText("Collection organization"))
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Not set", "Merged", "Split", "Non-merged"]);
    const samples = await findCategory("Samples");
    expect(
      within(samples.getByLabelText("Collection organization"))
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Not set", "ZIP archives", "Directories"]);
    expect(samples.getByText(/A MAME directory \(samplepath\)/)).toBeInTheDocument();
  });

  it("saves a partial configuration and refuses incomplete enabled categories", async () => {
    const backend = remoteBackend({ connections: [connection(NAS_ID, "Home NAS")] });
    const user = userEvent.setup();
    render(<RomManagerSection installation={newInstallation} />);
    const roms = await findCategory("Machine ROMs");
    await user.click(roms.getByLabelText("Enabled"));
    await user.click(screen.getByRole("button", { name: "Save ROM Manager settings" }));
    expect(roms.getByRole("alert")).toHaveTextContent(
      "To enable it, set a remote connection, a remote base path, the collection organization, a local destination.",
    );
    expect(sourcesCalls(backend)).toHaveLength(0);

    await user.selectOptions(roms.getByLabelText("Remote connection"), "Home NAS (SFTP)");
    await user.type(roms.getByLabelText("Remote base path"), "/volume1/mame/0280/roms");
    await user.selectOptions(roms.getByLabelText("Collection organization"), "Split");
    await user.selectOptions(roms.getByLabelText(/^MAME directory/), "D:\\Shared ROMs (not found)");
    await user.click(screen.getByRole("button", { name: "Save ROM Manager settings" }));
    expect(await screen.findByText("ROM Manager settings saved.")).toBeInTheDocument();
    expect(sourcesCalls(backend)).toEqual([
      {
        installationId: NEW_ID,
        config: {
          schemaVersion: 1,
          categories: {
            machineRoms: {
              enabled: true,
              connectionId: NAS_ID,
              remoteBasePath: "/volume1/mame/0280/roms",
              organization: "split",
              destination: { mode: "mameDirectory", directory: "D:\\Shared ROMs" },
            },
          },
        },
      },
    ]);
  });

  it("chooses a custom destination with the native picker", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      picks: ["E:\\YMDB samples ✓"],
    });
    const user = userEvent.setup();
    render(<RomManagerSection installation={newInstallation} />);
    const samples = await findCategory("Samples");
    await user.click(samples.getByLabelText("A custom directory"));
    expect(samples.getByText("No directory chosen")).toBeInTheDocument();
    await user.click(samples.getByRole("button", { name: "Choose…" }));
    expect(samples.getByText("E:\\YMDB samples ✓")).toBeInTheDocument();
    expect(samples.getByText(/adds this directory to -samplepath/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save ROM Manager settings" }));
    await screen.findByText("ROM Manager settings saved.");
    expect(sourcesCalls(backend)[0]?.config.categories.samples?.destination).toEqual({
      mode: "custom",
      directory: "E:\\YMDB samples ✓",
    });
    expect(sourcesCalls(backend)[0]?.config.categories.samples?.enabled).toBe(false);
  });

  it("keeps the configuration of each installation separate and reloads it", async () => {
    const backend = remoteBackend({
      connections: [connection(NAS_ID, "Home NAS")],
      sources: {
        [OLD_ID]: {
          schemaVersion: 1,
          categories: {
            machineRoms: {
              enabled: true,
              connectionId: NAS_ID,
              remoteBasePath: "/volume1/mame/0252/roms",
              organization: "merged",
              destination: { mode: "mameDirectory", directory: "roms" },
            },
          },
        },
      },
    });
    const user = userEvent.setup();
    const { rerender, unmount } = render(
      <RomManagerSection key={NEW_ID} installation={newInstallation} />,
    );
    const roms = await findCategory("Machine ROMs");
    expect(roms.getByLabelText("Remote base path")).toHaveValue("");
    await user.type(roms.getByLabelText("Remote base path"), "/volume1/mame/0280/roms");
    await user.click(screen.getByRole("button", { name: "Save ROM Manager settings" }));
    await screen.findByText("ROM Manager settings saved.");

    // Switching installation shows that installation's own settings.
    rerender(<RomManagerSection key={OLD_ID} installation={oldInstallation} />);
    await waitFor(async () => {
      expect((await findCategory("Machine ROMs")).getByLabelText("Remote base path")).toHaveValue(
        "/volume1/mame/0252/roms",
      );
    });
    expect(backend.sources[OLD_ID]?.categories.machineRoms?.remoteBasePath).toBe(
      "/volume1/mame/0252/roms",
    );
    expect(backend.sources[NEW_ID]?.categories.machineRoms?.remoteBasePath).toBe(
      "/volume1/mame/0280/roms",
    );

    // A new mount (for example after a restart) reads the saved settings.
    unmount();
    render(<RomManagerSection installation={newInstallation} />);
    await waitFor(async () => {
      expect((await findCategory("Machine ROMs")).getByLabelText("Remote base path")).toHaveValue(
        "/volume1/mame/0280/roms",
      );
    });
  });

  it("points to Settings when no connection exists and shows warnings", async () => {
    remoteBackend();
    render(<RomManagerSection installation={newInstallation} />);
    expect(await screen.findByText(/add one in Settings → Remote Connections/)).toBeInTheDocument();
    expect(screen.getAllByText(/from C:\\MAME\\mame.ini/)).not.toHaveLength(0);
  });
});

// Stylesheets are read from disk (relative to the project root, where
// Vitest runs): Vitest does not process CSS imports.
const remoteCss = readFileSync("src/features/remote/remote.css", "utf8");
const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

describe("remote styles", () => {
  it("use only theme tokens defined by both themes", () => {
    expect(remoteCss).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
    const tokens = new Set(remoteCss.match(/--color-[a-z-]+/g));
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
