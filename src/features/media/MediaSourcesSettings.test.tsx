import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaSourcesView } from "../../test/mediaFixtures";
import type { MediaConfig, MediaSourcesView } from "./mediaApi";
import { MediaProvider } from "./MediaProvider";
import { MediaSourcesSettings } from "./MediaSourcesSettings";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;

/** A stateful fake of the media commands. */
function mediaBackend(initial: MediaSourcesView, picked: (string | null)[] = []) {
  let view = initial;
  const picks = [...picked];
  invokeMock.mockImplementation((command: string, args?: Args) => {
    switch (command) {
      case "get_media_sources":
        return Promise.resolve(view);
      case "save_media_sources": {
        const config = (args as { config: MediaConfig }).config;
        view = mediaSourcesView(config.sources);
        return Promise.resolve(view);
      }
      case "pick_media_directory":
        return Promise.resolve(picks.shift() ?? null);
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  });
  return { sources: () => view.config.sources };
}

function renderSettings() {
  const user = userEvent.setup();
  render(
    <MediaProvider>
      <MediaSourcesSettings />
    </MediaProvider>,
  );
  return user;
}

const category = (label: string) => within(screen.getByRole("group", { name: label }));
const findCategory = async (label: string) =>
  within(await screen.findByRole("group", { name: label }));

describe("MediaSourcesSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("lists every category with its directories and their status", async () => {
    mediaBackend(
      mediaSourcesView(
        { snap: ["D:/Artwork ✓/snap"], flyers: ["\\\\nas\\mame\\flyers"] },
        "missing",
      ),
    );
    renderSettings();

    for (const label of ["Snapshots", "Title screens", "Flyers", "Cabinets", "Marquees"]) {
      expect(await screen.findByRole("group", { name: label })).toBeInTheDocument();
    }
    expect(category("Snapshots").getByText("D:/Artwork ✓/snap")).toBeInTheDocument();
    expect(category("Snapshots").getByText("Not found")).toBeInTheDocument();
    expect(category("Title screens").getByText("No directory.")).toBeInTheDocument();
  });

  it("adds directories with the picker, in order of precedence", async () => {
    const fake = mediaBackend(mediaSourcesView(), ["D:/Artwork/snap", null, "E:/More snaps ✓"]);
    const user = renderSettings();
    const add = async () => {
      await user.click(
        (await findCategory("Snapshots")).getByRole("button", { name: "Add directory…" }),
      );
    };

    await add();
    await waitFor(() => {
      expect(fake.sources()).toEqual({ snap: ["D:/Artwork/snap"] });
    });
    // Cancelling the picker changes nothing.
    await add();
    await add();
    await waitFor(() => {
      expect(fake.sources()).toEqual({ snap: ["D:/Artwork/snap", "E:/More snaps ✓"] });
    });
    expect(await category("Snapshots").findByText("E:/More snaps ✓")).toBeInTheDocument();
    expect(category("Snapshots").getAllByText("Available")).toHaveLength(2);
  });

  it("reorders and removes directories", async () => {
    const fake = mediaBackend(mediaSourcesView({ snap: ["A:/first", "B:/second", "C:/third"] }));
    const user = renderSettings();

    await user.click(await screen.findByRole("button", { name: "Move up B:/second" }));
    await waitFor(() => {
      expect(fake.sources()).toEqual({ snap: ["B:/second", "A:/first", "C:/third"] });
    });
    expect(await screen.findByRole("button", { name: "Move up B:/second" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Move down A:/first" }));
    await waitFor(() => {
      expect(fake.sources()).toEqual({ snap: ["B:/second", "C:/third", "A:/first"] });
    });

    await user.click(await screen.findByRole("button", { name: "Remove C:/third" }));
    await waitFor(() => {
      expect(fake.sources()).toEqual({ snap: ["B:/second", "A:/first"] });
    });
  });

  it("refuses a directory listed twice and reports rejected saves", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_media_sources")
        return Promise.resolve(mediaSourcesView({ snap: ["D:/snap"] }));
      if (command === "pick_media_directory") return Promise.resolve(pickQueue.shift() ?? null);
      if (command === "save_media_sources")
        return Promise.reject(
          Object.assign(new Error("invalidRequest"), {
            kind: "invalidRequest",
            message: "'relative' is not an absolute directory path",
          }),
        );
      return Promise.reject(new Error(command));
    });
    const pickQueue = ["D:/snap", "relative"];
    const user = renderSettings();
    const add = (await findCategory("Snapshots")).getByRole("button", { name: "Add directory…" });

    await user.click(add);
    expect(await screen.findByRole("alert")).toHaveTextContent("already listed");
    await user.click(add);
    expect(await screen.findByRole("alert")).toHaveTextContent("not an absolute directory path");
  });

  it("warns when media.json is invalid", async () => {
    mediaBackend({ ...mediaSourcesView(), loadError: "media configuration is invalid" });
    renderSettings();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "media configuration is invalid. No directory is used until the sources are saved again.",
    );
  });
});
