import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const responses: Record<string, unknown> = {
  get_mame_configuration: { executablePath: null, executableExists: false },
  get_catalog_status: {
    catalog: null,
    catalogError: null,
    freshness: null,
    mameConfigured: false,
    import: { state: "idle" },
  },
};

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) =>
      command in responses
        ? Promise.resolve(responses[command])
        : Promise.reject(new Error(`unexpected command ${command}`)),
    );
  });

  it("renders the application identity and version", async () => {
    render(<App />);

    expect(screen.getByRole("heading", { level: 1, name: "Your MAME DB" })).toBeInTheDocument();
    expect(screen.getByText("YMDB")).toBeInTheDocument();
    expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeInTheDocument();
    expect(screen.getByText(/early development build/i)).toBeInTheDocument();
    expect(await screen.findByText("No MAME executable configured.")).toBeInTheDocument();
  });

  it("renders the MAME installation, catalog and backend connection sections", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { level: 2, name: "MAME Installation" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "MAME Catalog" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check backend connection" })).toBeEnabled();
    expect(await screen.findByRole("button", { name: "Select executable…" })).toBeEnabled();
    expect(await screen.findByText("No catalog has been imported yet.")).toBeInTheDocument();
  });
});
