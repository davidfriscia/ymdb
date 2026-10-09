import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MachineSearchResponse, SoftwareSearchResponse } from "./searchApi";
import { SearchPreview } from "./SearchPreview";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

function machines(total: number, names: string[]): MachineSearchResponse {
  return {
    items: names.map((name) => ({
      name,
      description: `${name} description`,
      year: "1980",
      manufacturer: "Namco",
      cloneOf: null,
      isBios: false,
      isDevice: false,
      isMechanical: false,
      isRunnable: true,
      driverStatus: "good",
      emulationStatus: "good",
    })),
    total,
    page: 0,
    pageSize: 20,
    pageCount: Math.ceil(total / 20),
    facets: [],
    elapsedMs: 1.25,
  };
}

const SOFTWARE: SoftwareSearchResponse = {
  items: [
    {
      listName: "nes",
      listDescription: "Nintendo Entertainment System cartridges",
      name: "smb",
      description: "Super Mario Bros.",
      year: "1985",
      publisher: "Nintendo",
      cloneOf: null,
      supported: "yes",
    },
  ],
  total: 1,
  page: 0,
  pageSize: 20,
  pageCount: 1,
  facets: [],
  elapsedMs: 2,
};

interface Request {
  request: { text: string; pageSize: number };
}

/** A rejection carrying a serialized `CommandError`, as Tauri produces. */
function commandError(kind: string): Promise<never> {
  return Promise.reject(Object.assign(new Error(kind), { kind }));
}

function renderPreview() {
  const user = userEvent.setup();
  render(<SearchPreview debounceMs={0} />);
  return user;
}

describe("SearchPreview", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("shows the total, the time and the first results", async () => {
    invokeMock.mockResolvedValue(machines(48893, ["pacman", "puckman"]));
    renderPreview();

    expect(
      await screen.findByText("48,893 results in 1.3 ms (showing the first 2)"),
    ).toBeInTheDocument();
    const results = within(screen.getByRole("list", { name: "Search results" }));
    expect(results.getByText("pacman")).toBeInTheDocument();
    expect(results.getAllByText(/1980 · Namco/)).toHaveLength(2);
    expect(invokeMock).toHaveBeenCalledWith("search_catalog_machines", {
      request: { text: "", pageSize: 20 },
    });
  });

  it("searches the typed text", async () => {
    invokeMock.mockImplementation((_command: string, args: Request) =>
      Promise.resolve(
        args.request.text === "pac" ? machines(1, ["pacman"]) : machines(2, ["a", "b"]),
      ),
    );
    const user = renderPreview();

    await user.type(screen.getByRole("searchbox", { name: "Search text" }), "pac");

    expect(await screen.findByText("1 results in 1.3 ms")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenLastCalledWith("search_catalog_machines", {
      request: { text: "pac", pageSize: 20 },
    });
  });

  it("searches software separately", async () => {
    invokeMock.mockImplementation((command: string) =>
      Promise.resolve(command === "search_catalog_software" ? SOFTWARE : machines(0, [])),
    );
    const user = renderPreview();

    await user.selectOptions(screen.getByRole("combobox", { name: "Search scope" }), "software");

    expect(await screen.findByText("nes/smb")).toBeInTheDocument();
    expect(screen.getByText(/1985 · Nintendo/)).toBeInTheDocument();
  });

  it("explains search errors", async () => {
    invokeMock.mockImplementation((command: string) =>
      command === "search_catalog_software"
        ? commandError("softwareListsNotImported")
        : commandError("catalogNotImported"),
    );
    const user = renderPreview();

    expect(await screen.findByRole("alert")).toHaveTextContent("No catalog has been imported yet.");

    await user.selectOptions(screen.getByRole("combobox", { name: "Search scope" }), "software");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The software lists have not been imported yet.",
    );
  });

  it("ignores the responses of superseded searches", async () => {
    let releaseSlow: (value: MachineSearchResponse) => void = () => undefined;
    invokeMock.mockImplementation((_command: string, args: Request) =>
      args.request.text === "p"
        ? new Promise((resolve) => {
            releaseSlow = resolve;
          })
        : Promise.resolve(machines(1, ["pacman"])),
    );
    const user = renderPreview();
    const input = screen.getByRole("searchbox", { name: "Search text" });

    await user.type(input, "p");
    await user.type(input, "a");
    expect(await screen.findByText("pacman")).toBeInTheDocument();
    releaseSlow(machines(5, ["stale"]));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByText("stale")).not.toBeInTheDocument();
    expect(screen.getByText("1 results in 1.3 ms")).toBeInTheDocument();
  });
});
