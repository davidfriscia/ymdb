import { readFileSync } from "node:fs";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaSourcesView } from "../../test/mediaFixtures";
import type { CategoryImages, MediaConfig, MediaSourcesView, MediaTarget } from "./mediaApi";
import { MediaGallery } from "./MediaGallery";
import { MediaProvider } from "./MediaProvider";
import { useMedia } from "./mediaContext";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  convertFileSrc: (path: string, protocol: string) =>
    `${protocol}://localhost/${encodeURIComponent(path)}`,
}));

const ALL = {
  snap: ["D:/Artwork/snap"],
  titles: ["D:/Artwork/titles"],
  flyers: ["D:/Artwork/flyers"],
  cabinets: ["D:/Artwork/cabinets"],
  marquees: ["D:/Artwork/marquees"],
};

type Listing = (target: MediaTarget) => CategoryImages[] | Promise<CategoryImages[]>;

function mediaBackend(initial: MediaSourcesView, listing: Listing = () => []) {
  let view = initial;
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "get_media_sources":
        return Promise.resolve(view);
      case "save_media_sources":
        view = mediaSourcesView((args as { config: MediaConfig }).config.sources);
        return Promise.resolve(view);
      case "list_media":
        return Promise.resolve(listing((args as { target: MediaTarget }).target));
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  });
}

const listCalls = (): unknown[] =>
  invokeMock.mock.calls
    .filter(([name]) => name === "list_media")
    .map(([, args]) => args as unknown);

const PACMAN: MediaTarget = { kind: "machine", name: "pacman" };

/** Saves the media sources from inside the provider, as Settings does. */
function SaveSources({ sources }: { sources: MediaConfig["sources"] }) {
  const { save } = useMedia();
  return (
    <button type="button" onClick={() => void save({ schemaVersion: 1, sources })}>
      Save sources
    </button>
  );
}

function renderGallery(target: MediaTarget = PACMAN, title = "Pac-Man") {
  const user = userEvent.setup();
  const tree = (next: MediaTarget, nextTitle: string) => (
    <MediaProvider>
      <SaveSources sources={{ flyers: ALL.flyers }} />
      <MediaGallery target={next} title={nextTitle} />
    </MediaProvider>
  );
  const view = render(tree(target, title));
  return {
    user,
    rerender: (next: MediaTarget, nextTitle: string) => {
      view.rerender(tree(next, nextTitle));
    },
  };
}

const url = (path: string, version = 0) =>
  `ymdb-media://localhost/${encodeURIComponent(path)}?v=${String(version)}`;
const mainImage = () => document.querySelector<HTMLImageElement>(".media-gallery__image");
const stage = () => screen.getByRole("button", { name: /^Enlarge/ });
const categories = () => within(screen.getByRole("list", { name: "Image categories" }));
const dots = () => within(screen.getByRole("group", { name: "Images" }));

/** The categories of the gallery, as named by their buttons. */
function categoryNames(): (string | null)[] {
  return categories()
    .getAllByRole("button")
    .map((button) => button.getAttribute("aria-label"));
}

describe("MediaGallery", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("shows only the categories with images and starts with the first one in order", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [
      { category: "titles", count: 1 },
      { category: "flyers", count: 3 },
      { category: "marquees", count: 1 },
    ]);
    renderGallery();
    expect(screen.getByText("Loading artwork…")).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: "Image categories" })).toBeInTheDocument();

    expect(listCalls()).toEqual([{ target: PACMAN }]);
    // No placeholder for the snapshots and cabinets without images.
    expect(categoryNames()).toEqual(["Title screens (1)", "Flyers (3)", "Marquees (1)"]);
    expect(categories().getByRole("button", { name: "Title screens (1)" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(mainImage()).toHaveAttribute("src", url("titles/machine/pacman"));
    expect(mainImage()).toHaveAttribute("alt", "Pac-Man: Title screens");
    // Each category shows its first image as thumbnail.
    expect(document.querySelector('img[data-category="flyers"]')?.getAttribute("src")).toBe(
      url("flyers/machine/pacman"),
    );
    // A single image needs no navigation.
    expect(screen.queryByRole("button", { name: "Next image" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Images" })).toBeNull();
  });

  it("browses only the images of the selected category", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [
      { category: "snap", count: 3 },
      { category: "flyers", count: 2 },
    ]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });

    const previous = screen.getByRole("button", { name: "Previous image" });
    const next = screen.getByRole("button", { name: "Next image" });
    expect(previous).toBeDisabled();
    expect(dots().getAllByRole("button")).toHaveLength(3);
    expect(dots().getByRole("button", { name: "Image 1 of 3" })).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(next);
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/1"));
    expect(mainImage()).toHaveAttribute("alt", "Pac-Man: Snapshots 2");
    expect(dots().getByRole("button", { name: "Image 2 of 3" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await user.click(next);
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/2"));
    // The last image: no wrapping around.
    expect(next).toBeDisabled();
    expect(previous).toBeEnabled();
    await user.click(dots().getByRole("button", { name: "Image 1 of 3" }));
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman"));

    // Another category starts from its first image, with its own indicators.
    await user.click(next);
    await user.click(categories().getByRole("button", { name: "Flyers (2)" }));
    expect(categories().getByRole("button", { name: "Flyers (2)" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(categories().getByRole("button", { name: "Snapshots (3)" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(mainImage()).toHaveAttribute("src", url("flyers/machine/pacman"));
    expect(dots().getAllByRole("button")).toHaveLength(2);
    expect(screen.getByRole("region", { name: "Flyers" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Next image" }));
    expect(mainImage()).toHaveAttribute("src", url("flyers/machine/pacman/1"));
    expect(document.querySelectorAll('img[src*="snap%2F"]')).toHaveLength(1); // its thumbnail only
  });

  it("moves between images with the keyboard when the carousel has the focus", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [{ category: "snap", count: 4 }]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });

    stage().focus();
    await user.keyboard("{ArrowLeft}");
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman"));
    await user.keyboard("{ArrowRight}{ArrowRight}");
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/2"));
    await user.keyboard("{End}");
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/3"));
    await user.keyboard("{ArrowRight}");
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/3"));
    await user.keyboard("{Home}");
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman"));
    expect(screen.getByRole("region", { name: "Snapshots" })).toHaveTextContent(
      "Snapshots 1, image 1 of 4",
    );
  });

  it("shows the position as text when there are many images", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [{ category: "snap", count: 40 }]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });
    expect(screen.queryByRole("group", { name: "Images" })).toBeNull();
    expect(screen.getByText("1 / 40")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Next image" }));
    expect(screen.getByText("2 / 40")).toBeInTheDocument();
  });

  it("shows a discreet empty state without categories when no image exists", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, titles: ALL.titles }), () => []);
    renderGallery();
    expect(
      await screen.findByText("No artwork was found for this title in the media sources."),
    ).toBeInTheDocument();
    expect(mainImage()).toBeNull();
    expect(screen.queryByRole("list", { name: "Image categories" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Enlarge/ })).toBeNull();
  });

  it("explains that no media directory is configured, without looking for images", async () => {
    mediaBackend(mediaSourcesView({}));
    renderGallery();
    expect(
      await screen.findByText(
        "No media directories are configured. Add them in Settings, Media Sources.",
      ),
    ).toBeInTheDocument();
    expect(listCalls()).toHaveLength(0);
    expect(document.querySelectorAll("img")).toHaveLength(0);
  });

  it("reports an image that cannot be read and keeps the others", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [{ category: "snap", count: 2 }]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });
    const main = mainImage();
    if (!main) throw new Error("no main image");

    fireEvent.error(main);
    expect(screen.getByText("This image could not be read.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Next image" }));
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman/1"));
  });

  it("offers to retry when the images cannot be listed", async () => {
    let attempts = 0;
    mediaBackend(mediaSourcesView(ALL), () => {
      attempts++;
      return attempts === 1
        ? Promise.reject(new Error("share offline"))
        : [{ category: "snap", count: 1 }];
    });
    const { user } = renderGallery();
    expect(await screen.findByRole("alert")).toHaveTextContent("share offline");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("list", { name: "Image categories" })).toBeInTheDocument();
  });

  it("enlarges the images of the category in a lightbox navigated with the keyboard", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [
      { category: "snap", count: 3 },
      { category: "flyers", count: 1 },
    ]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });

    await user.click(stage());
    const dialog = screen.getByRole("dialog", { name: "Pac-Man: Snapshots 1" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const close = within(dialog).getByRole("button", { name: "Close" });
    expect(close).toHaveFocus();
    expect(within(dialog).getByText(/1 \/ 3/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Previous image" })).toBeDisabled();

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Snapshots 2" })).toBeInTheDocument();
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Next image" }),
    );
    expect(screen.getByRole("dialog", { name: "Pac-Man: Snapshots 3" })).toBeInTheDocument();
    // Only the images of the selected category, without wrapping around.
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Snapshots 3" })).toBeInTheDocument();

    // The focus stays inside the lightbox.
    const controls = within(screen.getByRole("dialog")).getAllByRole("button");
    for (let step = 0; step < controls.length + 1; step++) await user.tab();
    expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);

    // Escape closes, the focus returns to the carousel, which shows the same image.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(stage()).toHaveFocus();
    expect(mainImage()).toHaveAttribute("alt", "Pac-Man: Snapshots 3");

    // A single image has no navigation.
    await user.click(categories().getByRole("button", { name: "Flyers (1)" }));
    await user.click(stage());
    const single = within(screen.getByRole("dialog", { name: "Pac-Man: Flyers" }));
    expect(single.queryByRole("button", { name: "Next image" })).toBeNull();
    await user.click(single.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("starts over for another title, never showing the previous title's images", async () => {
    let release: (() => void) | null = null;
    mediaBackend(mediaSourcesView(ALL), (target) =>
      target.kind === "machine" && target.name === "galaga"
        ? new Promise((resolve) => {
            release = () => {
              resolve([
                { category: "titles", count: 2 },
                { category: "flyers", count: 1 },
              ]);
            };
          })
        : [
            { category: "snap", count: 2 },
            { category: "flyers", count: 1 },
          ],
    );
    const { user, rerender } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });
    await user.click(categories().getByRole("button", { name: "Flyers (1)" }));

    rerender({ kind: "machine", name: "galaga" }, "Galaga");
    expect(screen.getByText("Loading artwork…")).toBeInTheDocument();
    expect(mainImage()).toBeNull();
    expect(document.querySelector('img[src*="pacman"]')).toBeNull();

    await act(async () => {
      release?.();
      await Promise.resolve();
    });
    // The first available category and image again.
    expect(categoryNames()).toEqual(["Title screens (2)", "Flyers (1)"]);
    expect(mainImage()).toHaveAttribute("src", url("titles/machine/galaga"));
    expect(mainImage()).toHaveAttribute("alt", "Galaga: Title screens 1");
  });

  it("lists and shows software artwork by list and software name", async () => {
    mediaBackend(mediaSourcesView(ALL), () => [{ category: "snap", count: 2 }]);
    const { user } = renderGallery(
      { kind: "software", list: "nes", name: "smb" },
      "Super Mario Bros.",
    );
    await screen.findByRole("list", { name: "Image categories" });
    expect(listCalls()).toEqual([{ target: { kind: "software", list: "nes", name: "smb" } }]);
    expect(mainImage()).toHaveAttribute("src", url("snap/software/nes/smb"));
    await user.click(screen.getByRole("button", { name: "Next image" }));
    expect(mainImage()).toHaveAttribute("src", url("snap/software/nes/smb/1"));
  });

  it("lists the images again when the media sources are saved", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap }), () => [{ category: "snap", count: 1 }]);
    const { user } = renderGallery();
    await screen.findByRole("list", { name: "Image categories" });
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman"));

    await user.click(screen.getByRole("button", { name: "Save sources" }));
    await act(async () => {
      await Promise.resolve();
    });
    await screen.findByRole("list", { name: "Image categories" });
    expect(listCalls()).toHaveLength(2);
    expect(mainImage()).toHaveAttribute("src", url("snap/machine/pacman", 1));
  });
});

// Stylesheets are read from disk (relative to the project root, where
// Vitest runs): Vitest does not process CSS imports.
const mediaCss = readFileSync("src/features/media/media.css", "utf8");
const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const body = new RegExp(`(?:^|\\n)${escaped}\\s*\\{([^}]*)\\}`).exec(mediaCss)?.[1];
  if (body === undefined) throw new Error(`no rule for ${selector}`);
  return body;
}

describe("media gallery styles", () => {
  it("keep the proportions of every image in a stage of constant size", () => {
    // Contained, never cropped or stretched, in the gallery and the lightbox.
    expect(rule(".media-gallery__image")).toMatch(/object-fit:\s*contain/);
    expect(rule(".media-gallery__category-frame img")).toMatch(/object-fit:\s*contain/);
    expect(rule(".lightbox__image")).toMatch(/object-fit:\s*contain/);
    expect(mediaCss).not.toMatch(/object-fit:\s*(cover|fill)/);
    expect(rule(".media-gallery__stage")).toMatch(/height:\s*var\(--media-stage-height\)/);
  });

  it("use only theme tokens defined by the light and dark themes", () => {
    expect(mediaCss).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
    const tokens = new Set(mediaCss.match(/--(?:color|shadow)-[a-z-]+/g));
    expect(tokens).toContain("--color-overlay");
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
