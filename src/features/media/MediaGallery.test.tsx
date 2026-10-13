import { readFileSync } from "node:fs";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaSourcesView } from "../../test/mediaFixtures";
import type { MediaConfig, MediaSourcesView, MediaTarget } from "./mediaApi";
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

function mediaBackend(initial: MediaSourcesView) {
  let view = initial;
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "get_media_sources":
        return Promise.resolve(view);
      case "save_media_sources":
        view = mediaSourcesView((args as { config: MediaConfig }).config.sources);
        return Promise.resolve(view);
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  });
}

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
  const onAvailabilityChange = vi.fn();
  const tree = (next: MediaTarget, nextTitle: string) => (
    <MediaProvider>
      <SaveSources sources={{ flyers: ALL.flyers }} />
      <MediaGallery target={next} title={nextTitle} onAvailabilityChange={onAvailabilityChange} />
    </MediaProvider>
  );
  const view = render(tree(target, title));
  return {
    user,
    onAvailabilityChange,
    rerender: (next: MediaTarget, nextTitle: string) => {
      view.rerender(tree(next, nextTitle));
    },
  };
}

/** The probe image of a category (the thumbnail), requested once per category. */
function probe(category: string): HTMLImageElement {
  const image = document.querySelector<HTMLImageElement>(`img[data-category="${category}"]`);
  if (!image) throw new Error(`no image requested for ${category}`);
  return image;
}

function answer(results: Record<string, "load" | "error">) {
  for (const [category, result] of Object.entries(results)) {
    if (result === "load") fireEvent.load(probe(category));
    else fireEvent.error(probe(category));
  }
}

const mainImage = () => document.querySelector<HTMLImageElement>(".media-gallery__image");
const stage = () => screen.getByRole("button", { name: /^Enlarge/ });
function thumbs(): HTMLElement {
  // Queried directly: the list is hidden (and has no accessible name) with a single image.
  const list = document.querySelector<HTMLElement>(".media-gallery__thumbs");
  if (!list) throw new Error("no thumbnail list");
  return list;
}

describe("MediaGallery", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("requests each configured category once and shows the snapshot first", async () => {
    mediaBackend(mediaSourcesView(ALL));
    const { onAvailabilityChange } = renderGallery();
    await screen.findByText("Loading artwork…");

    const requested = [...document.querySelectorAll("img[data-category]")].map((image) =>
      image.getAttribute("src"),
    );
    expect(requested).toEqual(
      ["snap", "titles", "flyers", "cabinets", "marquees"].map(
        (category) => `ymdb-media://localhost/${category}%2Fmachine%2Fpacman?v=0`,
      ),
    );

    // A later category answering first does not become the main image.
    answer({ flyers: "load", marquees: "load" });
    expect(mainImage()).toBeNull();
    answer({ snap: "load", titles: "load", cabinets: "load" });

    expect(mainImage()).toHaveAttribute("src", probe("snap").getAttribute("src"));
    expect(mainImage()).toHaveAttribute("alt", "Pac-Man: Snapshots");
    expect(screen.getByText("Snapshots", { selector: "figcaption" })).toBeInTheDocument();
    const list = within(thumbs());
    expect(thumbs()).toBeVisible();
    expect(list.getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "Snapshots",
      "Title screens",
      "Flyers",
      "Cabinets",
      "Marquees",
    ]);
    expect(list.getByRole("button", { name: "Snapshots" })).toHaveAttribute("aria-pressed", "true");
    expect(onAvailabilityChange).toHaveBeenLastCalledWith(true);
  });

  it("shows a single snapshot without thumbnails", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, flyers: ALL.flyers }));
    renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", flyers: "error" });

    expect(mainImage()).toHaveAttribute("src", expect.stringContaining("snap%2Fmachine%2Fpacman"));
    expect(thumbs()).not.toBeVisible();
    // A missing image is not requested again.
    expect(document.querySelector('img[data-category="flyers"]')).toBeNull();
  });

  it("falls back to the first available category, for example a flyer", async () => {
    mediaBackend(mediaSourcesView(ALL));
    renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "error", titles: "error", flyers: "load", cabinets: "error" });

    expect(screen.getByText("Flyers", { selector: "figcaption" })).toBeInTheDocument();
    expect(stage()).toHaveAccessibleName("Enlarge flyers");
    expect(mainImage()).toHaveAttribute("src", expect.stringContaining("flyers%2Fmachine"));
    // The marquee is still unknown: no thumbnail is shown for it yet.
    expect(within(thumbs()).queryByRole("button", { name: "Marquees" })).toBeNull();
    answer({ marquees: "load" });
    expect(within(thumbs()).getByRole("button", { name: "Marquees" })).toBeVisible();
    expect(screen.getByText("Flyers", { selector: "figcaption" })).toBeInTheDocument();
  });

  it("shows a discreet empty state when no image exists", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, titles: ALL.titles }));
    const { onAvailabilityChange } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "error", titles: "error" });

    expect(
      screen.getByText("No artwork was found for this title in the media sources."),
    ).toBeInTheDocument();
    expect(mainImage()).toBeNull();
    expect(screen.queryByRole("button", { name: /^Enlarge/ })).toBeNull();
    expect(onAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("explains that no media directory is configured, without requesting images", async () => {
    mediaBackend(mediaSourcesView({}));
    const { onAvailabilityChange } = renderGallery();
    expect(
      await screen.findByText(
        "No media directories are configured. Add them in Settings, Media Sources.",
      ),
    ).toBeInTheDocument();
    expect(document.querySelectorAll("img")).toHaveLength(0);
    expect(onAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("moves to the next image when the main image cannot be read", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, titles: ALL.titles }));
    renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", titles: "load" });
    const main = mainImage();
    if (!main) throw new Error("no main image");

    fireEvent.error(main);
    expect(screen.getByText("Title screens", { selector: "figcaption" })).toBeInTheDocument();
    expect(thumbs()).not.toBeVisible();
  });

  it("selects an image from its thumbnail", async () => {
    mediaBackend(mediaSourcesView(ALL));
    const { user } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", titles: "load", flyers: "load", cabinets: "load", marquees: "load" });

    await user.click(within(thumbs()).getByRole("button", { name: "Marquees" }));
    expect(within(thumbs()).getByRole("button", { name: "Marquees" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(thumbs()).getByRole("button", { name: "Snapshots" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(mainImage()).toHaveAttribute("src", expect.stringContaining("marquees%2Fmachine"));
    expect(screen.getByText("Marquees", { selector: "figcaption" })).toBeInTheDocument();
  });

  it("enlarges images in a lightbox navigated with the keyboard", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, titles: ALL.titles, flyers: ALL.flyers }));
    const { user } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", titles: "load", flyers: "load" });

    await user.click(stage());
    const dialog = screen.getByRole("dialog", { name: "Pac-Man: Snapshots" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const close = within(dialog).getByRole("button", { name: "Close" });
    expect(close).toHaveFocus();
    expect(within(dialog).getByText(/1 \/ 3/)).toBeInTheDocument();
    expect(within(dialog).getByRole("img")).toHaveAttribute(
      "src",
      expect.stringContaining("snap%2Fmachine"),
    );

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Title screens" })).toBeInTheDocument();
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Next image" }),
    );
    expect(screen.getByRole("dialog", { name: "Pac-Man: Flyers" })).toBeInTheDocument();
    // Wraps around in both directions.
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Snapshots" })).toBeInTheDocument();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Flyers" })).toBeInTheDocument();

    // The focus stays inside the lightbox.
    const controls = within(screen.getByRole("dialog")).getAllByRole("button");
    for (let step = 0; step < controls.length + 1; step++) await user.tab();
    expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);

    // Escape closes, the focus returns to the gallery, the last image stays selected.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(stage()).toHaveFocus();
    expect(screen.getByText("Flyers", { selector: "figcaption" })).toBeInTheDocument();

    // The visible Close button closes it too.
    await user.click(stage());
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("has no image navigation in the lightbox for a single image", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap }));
    const { user } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load" });

    await user.click(stage());
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.queryByRole("button", { name: "Next image" })).toBeNull();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("dialog", { name: "Pac-Man: Snapshots" })).toBeInTheDocument();
  });

  it("starts over for another title, never showing the previous title's images", async () => {
    mediaBackend(mediaSourcesView(ALL));
    const { user, rerender } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", titles: "load", flyers: "load", cabinets: "load", marquees: "load" });
    await user.click(within(thumbs()).getByRole("button", { name: "Flyers" }));

    rerender({ kind: "machine", name: "galaga" }, "Galaga");
    expect(mainImage()).toBeNull();
    expect(screen.getByText("Loading artwork…")).toBeInTheDocument();
    expect(probe("snap")).toHaveAttribute(
      "src",
      expect.stringContaining("snap%2Fmachine%2Fgalaga"),
    );
    expect(document.querySelector('img[src*="pacman"]')).toBeNull();

    // The initial image follows the preferred order again.
    answer({ snap: "error", titles: "load", flyers: "load", cabinets: "load", marquees: "load" });
    expect(screen.getByText("Title screens", { selector: "figcaption" })).toBeInTheDocument();
    expect(mainImage()).toHaveAttribute("alt", "Galaga: Title screens");
  });

  it("resolves software artwork by list and software name", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap, titles: ALL.titles }));
    const { rerender } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load", titles: "load" });

    rerender({ kind: "software", list: "nes", name: "smb" }, "Super Mario Bros.");
    expect(probe("snap")).toHaveAttribute(
      "src",
      "ymdb-media://localhost/snap%2Fsoftware%2Fnes%2Fsmb?v=0",
    );
    expect(probe("titles")).toHaveAttribute(
      "src",
      "ymdb-media://localhost/titles%2Fsoftware%2Fnes%2Fsmb?v=0",
    );
    answer({ snap: "error", titles: "load" });
    expect(mainImage()).toHaveAttribute("alt", "Super Mario Bros.: Title screens");
  });

  it("requests the images again when the media sources are saved", async () => {
    mediaBackend(mediaSourcesView({ snap: ALL.snap }));
    const { user } = renderGallery();
    await screen.findByText("Loading artwork…");
    answer({ snap: "load" });
    expect(mainImage()).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "Save sources" }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(await screen.findByText("Loading artwork…")).toBeInTheDocument();
    expect(probe("flyers")).toHaveAttribute("src", expect.stringContaining("?v=1"));
    expect(document.querySelector('img[data-category="snap"]')).toBeNull();
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
  it("keep the proportions of every image", () => {
    // Contained, never cropped or stretched, in the gallery and the lightbox.
    expect(rule(".media-gallery__image")).toMatch(/object-fit:\s*contain/);
    expect(rule(".media-gallery__thumb-frame img")).toMatch(/object-fit:\s*contain/);
    expect(rule(".lightbox__image")).toMatch(/object-fit:\s*contain/);
    expect(mediaCss).not.toMatch(/object-fit:\s*(cover|fill)/);
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
