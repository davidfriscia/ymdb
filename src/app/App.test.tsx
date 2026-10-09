import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the application identity and version", () => {
    render(<App />);

    expect(screen.getByRole("heading", { level: 1, name: "Your MAME DB" })).toBeInTheDocument();
    expect(screen.getByText("YMDB")).toBeInTheDocument();
    expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeInTheDocument();
    expect(screen.getByText(/early development build/i)).toBeInTheDocument();
  });

  it("renders the backend connection check button", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "Check backend connection" })).toBeEnabled();
  });
});
