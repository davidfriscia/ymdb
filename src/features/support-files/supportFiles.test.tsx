import { readFileSync } from "node:fs";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { importPreview, supportFilesView, supportSource } from "../../test/supportFileFixtures";
import { SupportFilesSettings } from "./SupportFilesSettings";
import type { SupportFilesView } from "./supportFilesApi";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

function backend(handlers: Partial<Record<string, Handler>>) {
  invokeMock.mockImplementation((command: string, args?: Args) => {
    const handler = handlers[command];
    if (!handler) return Promise.reject(new Error(`unexpected command ${command}`));
    try {
      return Promise.resolve(handler(args ?? {}));
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
}

const calls = (command: string) =>
  invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Args);

const EMPTY = supportFilesView([]);
const SERIES = supportFilesView([supportSource("series")]);

function renderSettings() {
  const user = userEvent.setup();
  const onChanged = vi.fn();
  render(<SupportFilesSettings onChanged={onChanged} />);
  return { user, onChanged };
}

describe("SupportFilesSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("imports a file after showing its summary", async () => {
    let view: SupportFilesView = EMPTY;
    backend({
      get_support_files: () => view,
      preview_support_file_import: () =>
        importPreview({
          warnings: [{ kind: "invalidMachineName", count: 2, examples: ["line 9: Bad Name"] }],
        }),
      apply_support_file_import: () => {
        view = SERIES;
        return view;
      },
    });
    const { user, onChanged } = renderSettings();
    expect(await screen.findByText(/No support file has been imported yet/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Import support file…" }));
    expect(calls("preview_support_file_import")).toEqual([{ expectedFormat: null }]);
    const summary = within(await screen.findByRole("group", { name: "Import summary" }));
    expect(summary.getByText("Declared MAME version: 0.288")).toBeInTheDocument();
    expect(
      summary.getByText(/Series: 720 values for 12,399 machines — will be used/),
    ).toBeInTheDocument();
    expect(summary.getByText("2 warnings")).toBeInTheDocument();
    // Nothing is stored before confirming.
    expect(calls("apply_support_file_import")).toEqual([]);

    await user.click(summary.getByRole("button", { name: "Import" }));
    expect(await screen.findByText("series.ini imported.")).toBeInTheDocument();
    expect(calls("apply_support_file_import")).toEqual([{ importId: 1 }]);
    expect(onChanged).toHaveBeenCalledTimes(1);
    const card = within(screen.getByRole("listitem", { name: "series.ini" }));
    expect(card.getByText("Active")).toBeInTheDocument();
    expect(card.getByText("0.288")).toBeInTheDocument();
  });

  it("cancels a previewed import without storing it", async () => {
    backend({ get_support_files: () => EMPTY, preview_support_file_import: () => importPreview() });
    const { user, onChanged } = renderSettings();
    await user.click(await screen.findByRole("button", { name: "Import support file…" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("group", { name: "Import summary" })).not.toBeInTheDocument();
    expect(calls("apply_support_file_import")).toEqual([]);
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("asks before replacing an imported source, and reimports the same format", async () => {
    const replaces = supportSource("series", {
      fileName: "series-old.ini",
      declaredVersion: "0.287",
    });
    backend({
      get_support_files: () => supportFilesView([replaces]),
      preview_support_file_import: () => importPreview({ importId: 3, replaces }),
      apply_support_file_import: () => SERIES,
    });
    const { user } = renderSettings();
    await user.click(await screen.findByRole("button", { name: "Reimport series.ini" }));
    expect(calls("preview_support_file_import")).toEqual([{ expectedFormat: "series" }]);
    const summary = within(await screen.findByRole("group", { name: "Import summary" }));
    expect(summary.getByRole("alert")).toHaveTextContent(
      /This replaces the series.ini imported on .* \(series-old.ini, MAME 0.287\)/,
    );
    await user.click(summary.getByRole("button", { name: "Replace" }));
    await waitFor(() => {
      expect(calls("apply_support_file_import")).toEqual([{ importId: 3 }]);
    });
  });

  it("reports files that cannot be imported", async () => {
    backend({
      get_support_files: () => EMPTY,
      preview_support_file_import: () => {
        throw Object.assign(new Error("invalidFile"), {
          kind: "invalidFile",
          message: "'mature.ini' cannot be imported: not supported",
        });
      },
    });
    const { user } = renderSettings();
    await user.click(await screen.findByRole("button", { name: "Import support file…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("'mature.ini' cannot be imported");
  });

  it("removes a source only after confirmation, leaving the dimension without one", async () => {
    let view = supportFilesView([
      supportSource("catver"),
      supportSource("catlist", {
        dimensions: [{ dimension: "category", machineCount: 1, valueCount: 1, active: false }],
      }),
    ]);
    backend({
      get_support_files: () => view,
      remove_support_file: () => {
        view = supportFilesView([
          supportSource("catlist", {
            dimensions: [{ dimension: "category", machineCount: 1, valueCount: 1, active: false }],
          }),
        ]);
        return view;
      },
    });
    const { user, onChanged } = renderSettings();
    await user.click(await screen.findByRole("button", { name: "Remove catver.ini" }));
    const confirm = within(screen.getByRole("listitem", { name: "catver.ini" }));
    expect(confirm.getByRole("alert")).toHaveTextContent(
      "Category and Version Added will have no source until you select or import another one. The file on disk is not deleted.",
    );
    expect(calls("remove_support_file")).toEqual([]);
    await user.click(confirm.getByRole("button", { name: "Remove" }));

    expect(await screen.findByText(/catver.ini removed from YMDB/)).toBeInTheDocument();
    expect(calls("remove_support_file")).toEqual([{ format: "catver" }]);
    expect(onChanged).toHaveBeenCalled();
    // The alternative is not activated implicitly: the user is asked to choose.
    expect(
      screen.getByText("Category has no active source: select one to use it."),
    ).toBeInTheDocument();
  });

  it("selects the active source of a dimension among the alternatives", async () => {
    const view = supportFilesView([
      supportSource("catver"),
      supportSource("catlist", {
        dimensions: [{ dimension: "category", machineCount: 1, valueCount: 1, active: false }],
      }),
    ]);
    backend({ get_support_files: () => view, set_active_support_source: () => view });
    const { user, onChanged } = renderSettings();
    const category = await screen.findByRole("combobox", { name: "Category" });
    expect(category).toHaveValue("catver");
    expect(
      within(category)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Not used", "catver.ini", "catlist.ini"]);
    await user.selectOptions(category, "catlist");
    await waitFor(() => {
      expect(calls("set_active_support_source")).toEqual([
        { dimension: "category", format: "catlist" },
      ]);
    });
    expect(onChanged).toHaveBeenCalled();
    // Without an imported source there is nothing to choose.
    expect(screen.getByRole("combobox", { name: "Genre" })).toBeDisabled();
  });
});

// Stylesheets are read from disk (relative to the project root, where
// Vitest runs): Vitest does not process CSS imports.
const themesCss = readFileSync("src/app/theme/themes.css", "utf8");

describe("support file and filter styles", () => {
  it.each(["src/features/support-files/supportFiles.css", "src/features/browser/browser.css"])(
    "%s uses only theme tokens defined by both themes",
    (path) => {
      const css = readFileSync(path, "utf8");
      expect(css).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
      const tokens = new Set(css.match(/--color-[a-z-]+/g));
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
    },
  );
});
