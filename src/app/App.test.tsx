import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({ executablePath: null, executableExists: false });
  });

  it("renders the application identity and version", async () => {
    render(<App />);

    expect(screen.getByRole("heading", { level: 1, name: "Your MAME DB" })).toBeInTheDocument();
    expect(screen.getByText("YMDB")).toBeInTheDocument();
    expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeInTheDocument();
    expect(screen.getByText(/early development build/i)).toBeInTheDocument();
    expect(await screen.findByText("No MAME executable configured.")).toBeInTheDocument();
  });

  it("renders the MAME installation and backend connection sections", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { level: 2, name: "MAME Installation" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check backend connection" })).toBeEnabled();
    expect(await screen.findByRole("button", { name: "Select executable…" })).toBeEnabled();
  });
});
