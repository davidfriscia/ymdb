import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { HealthCheck } from "./HealthCheck";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

describe("HealthCheck", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("invokes the check_health command and displays the report", async () => {
    invokeMock.mockResolvedValue({
      status: "ok",
      os: "windows",
      arch: "x86_64",
      version: "0.1.0",
    });
    const user = userEvent.setup();
    render(<HealthCheck />);

    await user.click(screen.getByRole("button", { name: "Check backend connection" }));

    expect(invokeMock).toHaveBeenCalledWith("check_health");
    expect(await screen.findByText("windows")).toBeInTheDocument();
    expect(screen.getByText("ok")).toBeInTheDocument();
    expect(screen.getByText("x86_64")).toBeInTheDocument();
    expect(screen.getByText("0.1.0")).toBeInTheDocument();
  });

  it("displays an error when the command fails", async () => {
    invokeMock.mockRejectedValue("IPC unavailable");
    const user = userEvent.setup();
    render(<HealthCheck />);

    await user.click(screen.getByRole("button", { name: "Check backend connection" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Backend check failed: IPC unavailable",
    );
  });
});
