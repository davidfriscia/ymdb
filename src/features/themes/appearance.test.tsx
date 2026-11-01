import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BUILTIN_THEME_TOKENS } from "../../app/theme/builtinThemeTokens";
import type { ThemeSelection } from "../../app/theme/themes";
import { TOKENS_VERSION, type ThemeTokens } from "../../app/theme/themeTokens";
import { customTheme, OTHER_THEME_ID, sunsetTokens, THEME_ID } from "../../test/themeFixtures";
import { AppearanceSettings } from "./AppearanceSettings";
import { CustomThemesProvider } from "./CustomThemesProvider";
import { buildThemeDocument } from "./themeFile";
import type { CustomTheme } from "./themesApi";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

/** A backend keeping the themes in memory, like `CustomThemesState`. */
let stored: CustomTheme[] = [];
let loadFails = false;
let chosenFile: unknown = null;
let nextId = 0;

function backend(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const clone = (): CustomTheme[] => structuredClone(stored);
  switch (command) {
    case "get_custom_themes":
      return loadFails
        ? Promise.reject(
            Object.assign(new Error("settings"), {
              kind: "settings",
              message: "the custom themes file is invalid",
            }),
          )
        : Promise.resolve(clone());
    case "create_custom_theme": {
      nextId += 1;
      const id = String(nextId).padStart(32, "a");
      stored.push({
        id,
        name: args.name as string,
        ...(args.sourceThemeId ? { sourceThemeId: args.sourceThemeId as string } : {}),
        tokensVersion: args.tokensVersion as number,
        tokens: structuredClone(args.tokens),
        createdAt: 2,
        updatedAt: 2,
      });
      return Promise.resolve({ id, themes: clone() });
    }
    case "update_custom_theme":
      stored = stored.map((theme) =>
        theme.id === args.id
          ? { ...theme, name: args.name as string, tokens: structuredClone(args.tokens) }
          : theme,
      );
      return Promise.resolve(clone());
    case "rename_custom_theme":
      stored = stored.map((theme) =>
        theme.id === args.id ? { ...theme, name: args.name as string } : theme,
      );
      return Promise.resolve(clone());
    case "delete_custom_theme":
      stored = stored.filter((theme) => theme.id !== args.id);
      return Promise.resolve(clone());
    case "choose_theme_file":
      return Promise.resolve(chosenFile);
    case "export_theme_file":
      return Promise.resolve("C:/exports/theme.ymdb-theme.json");
    default:
      return Promise.reject(new Error(`unexpected command ${command}`));
  }
}

function calls(command: string): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter(([name]) => name === command)
    .map(([, args]) => args as Record<string, unknown>);
}

let selections: ThemeSelection[] = [];

function Harness({ initial }: { initial: ThemeSelection }) {
  const [theme, setTheme] = useState<ThemeSelection>(initial);
  return (
    <CustomThemesProvider>
      <AppearanceSettings
        theme={theme}
        onThemeChange={(next) => {
          selections.push(next);
          setTheme(next);
        }}
      />
    </CustomThemesProvider>
  );
}

function renderAppearance(initial: ThemeSelection = "light") {
  return render(<Harness initial={initial} />);
}

function editor() {
  return within(screen.getByRole("dialog", { name: /Edit theme/ }));
}

function preview(): HTMLElement {
  return screen.getByTestId("theme-editor-preview");
}

describe("Appearance", () => {
  beforeEach(() => {
    stored = [];
    loadFails = false;
    chosenFile = null;
    nextId = 0;
    selections = [];
    document.documentElement.removeAttribute("style");
    invokeMock.mockReset();
    invokeMock.mockImplementation(backend);
  });

  it("lists built-in themes as immutable and custom themes as editable", async () => {
    stored = [customTheme()];
    renderAppearance();

    expect(await screen.findByRole("radio", { name: "Sunset" })).toHaveAccessibleDescription(
      "Custom theme, cloned from Neon 80s.",
    );
    for (const name of ["Light", "Dark", "Neon 80s"]) {
      expect(screen.getByRole("button", { name: `Clone ${name}` })).toBeEnabled();
      expect(screen.getByRole("button", { name: `Export ${name}` })).toBeEnabled();
      expect(screen.queryByRole("button", { name: `Edit ${name}` })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: `Rename ${name}` })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: `Delete ${name}` })).not.toBeInTheDocument();
    }
    for (const action of ["Edit", "Clone", "Rename", "Export", "Delete"]) {
      expect(screen.getByRole("button", { name: `${action} Sunset` })).toBeEnabled();
    }
  });

  it("clones a built-in theme as an exact, independent copy and opens the editor", async () => {
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Clone Neon 80s" }));

    const dialog = within(screen.getByRole("dialog", { name: "Clone Neon 80s" }));
    expect(dialog.getByLabelText("Theme name")).toHaveValue("Neon 80s copy");
    await user.click(dialog.getByRole("button", { name: "Clone and edit" }));

    const [created] = calls("create_custom_theme");
    expect(created).toMatchObject({
      name: "Neon 80s copy",
      sourceThemeId: "neon80s",
      tokensVersion: TOKENS_VERSION,
      tokens: BUILTIN_THEME_TOKENS.neon80s,
    });
    // A copy, never the built-in object itself.
    expect(created?.tokens).not.toBe(BUILTIN_THEME_TOKENS.neon80s);
    expect(await screen.findByRole("dialog", { name: "Edit theme: Neon 80s copy" })).toBeVisible();
    expect(preview().style.getPropertyValue("--color-accent")).toBe("#ff27d8");
    // Cloning never changes the selection.
    expect(selections).toEqual([]);
  });

  it("clones a custom theme; editing the clone leaves the original unchanged", async () => {
    stored = [customTheme()];
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Clone Sunset" }));
    await user.click(screen.getByRole("button", { name: "Clone and edit" }));

    expect(calls("create_custom_theme")[0]).toMatchObject({
      name: "Sunset copy",
      sourceThemeId: `custom:${THEME_ID}`,
      tokens: sunsetTokens(),
    });
    await screen.findByRole("dialog", { name: "Edit theme: Sunset copy" });
    await user.click(editor().getByRole("tab", { name: "Accents" }));
    const hex = editor().getByRole("textbox", { name: "Primary accent" });
    await user.clear(hex);
    await user.type(hex, "#00ff00");
    await user.click(editor().getByRole("button", { name: "Save" }));

    const [update] = calls("update_custom_theme");
    expect(update?.id).not.toBe(THEME_ID);
    expect((stored[0]?.tokens as ThemeTokens).colors.accent).toBe("#ff8800");
    expect((stored[1]?.tokens as ThemeTokens).colors.accent).toBe("#00ff00");
    expect(screen.getByRole("radio", { name: "Sunset copy" })).toHaveAccessibleDescription(
      "Custom theme, cloned from Sunset.",
    );
  });

  it("refuses a duplicate name, ignoring case, without calling the backend", async () => {
    stored = [customTheme()];
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Clone Dark" }));
    const name = screen.getByLabelText("Theme name");
    await user.clear(name);
    await user.type(name, " sunset ");

    expect(screen.getByText(/A theme named “sunset” already exists/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Clone and edit" })).toBeDisabled();
    await user.clear(name);
    await user.type(name, "dark");
    expect(screen.getByText(/already exists/)).toBeVisible();
    expect(calls("create_custom_theme")).toEqual([]);
  });

  it("renames a custom theme without changing the selection", async () => {
    stored = [customTheme()];
    const user = userEvent.setup();
    renderAppearance(`custom:${THEME_ID}`);
    await user.click(await screen.findByRole("button", { name: "Rename Sunset" }));
    const name = screen.getByLabelText("Theme name");
    await user.clear(name);
    await user.type(name, "Dusk");
    await user.click(screen.getByRole("button", { name: "Rename" }));

    expect(calls("rename_custom_theme")).toEqual([{ id: THEME_ID, name: "Dusk" }]);
    expect(await screen.findByRole("radio", { name: "Dusk" })).toBeChecked();
    expect(selections).toEqual([]);
  });

  it("deletes the theme in use after confirmation and falls back to Light", async () => {
    stored = [customTheme(), customTheme({ id: OTHER_THEME_ID, name: "Other" })];
    const user = userEvent.setup();
    renderAppearance(`custom:${THEME_ID}`);
    await user.click(await screen.findByRole("button", { name: "Delete Sunset" }));
    expect(screen.getByText(/Light will be used instead/)).toBeVisible();
    expect(calls("delete_custom_theme")).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Delete" }));

    expect(calls("delete_custom_theme")).toEqual([{ id: THEME_ID }]);
    expect(await screen.findByText("“Sunset” was deleted. Light is now used.")).toBeVisible();
    expect(selections).toEqual(["light"]);
    expect(screen.getByRole("radio", { name: "Light" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Other" })).toBeInTheDocument();
  });

  it("deletes another theme without changing the selection", async () => {
    stored = [customTheme(), customTheme({ id: OTHER_THEME_ID, name: "Other" })];
    const user = userEvent.setup();
    renderAppearance(`custom:${THEME_ID}`);
    await user.click(await screen.findByRole("button", { name: "Delete Other" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("“Other” was deleted.")).toBeVisible();
    expect(selections).toEqual([]);
  });

  it("selects a custom theme by its stable id", async () => {
    stored = [customTheme()];
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("radio", { name: "Sunset" }));
    expect(selections).toEqual([`custom:${THEME_ID}`]);
  });

  it("exports built-in and custom themes as portable documents", async () => {
    stored = [customTheme()];
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Export Neon 80s" }));
    await user.click(screen.getByRole("button", { name: "Export Sunset" }));

    const [builtin, custom] = calls("export_theme_file");
    expect(builtin).toMatchObject({
      name: "Neon 80s",
      document: {
        format: "ymdb-theme",
        formatVersion: 1,
        name: "Neon 80s",
        basedOn: "neon80s",
        tokensVersion: TOKENS_VERSION,
        tokens: BUILTIN_THEME_TOKENS.neon80s,
      },
    });
    expect(custom).toMatchObject({
      name: "Sunset",
      document: { name: "Sunset", basedOn: "neon80s", tokens: sunsetTokens() },
    });
    // Nothing local: no id, path or dates of the stored theme.
    expect(JSON.stringify(custom?.document)).not.toContain(THEME_ID);
    expect(await screen.findByText(/was exported to C:\/exports/)).toBeVisible();
  });

  it("imports a theme file as a new theme, never replacing an existing one", async () => {
    stored = [customTheme()];
    chosenFile = {
      fileName: "sunset.ymdb-theme.json",
      document: buildThemeDocument("Sunset", sunsetTokens(), "neon80s"),
    };
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Import theme…" }));

    // The name is taken: a free one is proposed.
    expect(screen.getByLabelText("Theme name")).toHaveValue("Sunset 2");
    await user.click(screen.getByRole("button", { name: "Import" }));

    expect(calls("create_custom_theme")[0]).toMatchObject({
      name: "Sunset 2",
      sourceThemeId: "neon80s",
      tokens: sunsetTokens(),
    });
    expect(stored.map((theme) => theme.name)).toEqual(["Sunset", "Sunset 2"]);
    expect(await screen.findByText(/imported as “Sunset 2”/)).toBeVisible();
  });

  it("imports an exported built-in theme as an independent custom theme", async () => {
    chosenFile = {
      fileName: "dark.ymdb-theme.json",
      document: buildThemeDocument("Dark", BUILTIN_THEME_TOKENS.dark, "dark"),
    };
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Import theme…" }));
    expect(screen.getByLabelText("Theme name")).toHaveValue("Dark 2");
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByRole("button", { name: "Edit Dark 2" })).toBeEnabled();
  });

  it("refuses incompatible and invalid theme files", async () => {
    const user = userEvent.setup();
    renderAppearance();
    const valid = buildThemeDocument("Dark", BUILTIN_THEME_TOKENS.dark, "dark");
    for (const [document, message] of [
      [{ ...valid, formatVersion: 2 }, /newer version of YMDB/],
      [{ ...valid, tokens: { ...valid.tokens, colors: {} } }, /invalid colors/],
      [{ hello: "world" }, /not a YMDB theme/],
    ] as const) {
      chosenFile = { fileName: "bad.ymdb-theme.json", document };
      await user.click(await screen.findByRole("button", { name: "Import theme…" }));
      expect(await screen.findByText(message)).toBeVisible();
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    }
    expect(calls("create_custom_theme")).toEqual([]);
  });

  it("reports an unreadable custom themes file and keeps the built-in themes", async () => {
    loadFails = true;
    renderAppearance(`custom:${THEME_ID}`);
    expect(await screen.findByText(/custom themes cannot be read/)).toBeVisible();
    expect(screen.getByText(/selected custom theme is not available/)).toBeVisible();
    expect(screen.getByRole("radio", { name: "Dark" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Import theme…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clone Dark" })).toBeDisabled();
    // Nothing is written.
    expect(invokeMock.mock.calls.map(([name]) => name as string)).toEqual(["get_custom_themes"]);
  });

  it("keeps a theme of a newer format without letting it be used or edited", async () => {
    stored = [customTheme({ tokensVersion: 2, tokens: { future: true } })];
    renderAppearance();
    expect(await screen.findByRole("radio", { name: "Sunset" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Edit Sunset" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export Sunset" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete Sunset" })).toBeEnabled();
  });
});

describe("Theme Editor", () => {
  beforeEach(() => {
    stored = [customTheme()];
    loadFails = false;
    selections = [];
    document.documentElement.removeAttribute("style");
    document.documentElement.dataset.theme = "dark";
    invokeMock.mockReset();
    invokeMock.mockImplementation(backend);
  });

  async function openEditor() {
    const user = userEvent.setup();
    renderAppearance();
    await user.click(await screen.findByRole("button", { name: "Edit Sunset" }));
    return user;
  }

  it("groups tokens by meaning, with labels instead of property names", async () => {
    await openEditor();
    const tabs = editor()
      .getAllByRole("tab")
      .map((tab) => tab.textContent);
    expect(tabs).toEqual([
      "Backgrounds",
      "Typography",
      "Accents",
      "Interactive states",
      "Status colors",
      "Borders and effects",
    ]);
    expect(editor().getByRole("textbox", { name: "Elevated surface" })).toBeVisible();
    expect(editor().queryByText(/--color/)).not.toBeInTheDocument();
  });

  it("previews HEX changes immediately, without touching the application or saving", async () => {
    const user = await openEditor();
    await user.click(editor().getByRole("tab", { name: "Accents" }));
    const hex = editor().getByRole("textbox", { name: "Primary accent" });
    expect(hex).toHaveValue("#ff8800");
    await user.clear(hex);
    await user.type(hex, "#0F0");

    expect(preview().style.getPropertyValue("--color-accent")).toBe("#00ff00");
    expect(editor().getByLabelText("Primary accent color picker")).toHaveValue("#00ff00");
    // The application keeps its theme and nothing is saved.
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(document.documentElement.style.getPropertyValue("--color-accent")).toBe("");
    expect(calls("update_custom_theme")).toEqual([]);
    // The field shows the normalized value once left.
    await user.tab();
    expect(hex).toHaveValue("#00ff00");
  });

  it("refuses invalid HEX values and blocks Save until they are fixed", async () => {
    const user = await openEditor();
    await user.click(editor().getByRole("tab", { name: "Accents" }));
    const hex = editor().getByRole("textbox", { name: "Primary accent" });
    await user.clear(hex);
    await user.type(hex, "red;}");

    expect(hex).toHaveAttribute("aria-invalid", "true");
    expect(editor().getByRole("alert")).toHaveTextContent(/Enter a HEX color/);
    expect(preview().style.getPropertyValue("--color-accent")).toBe("#ff8800");
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();

    // Still invalid after switching tabs and back.
    await user.click(editor().getByRole("tab", { name: "Backgrounds" }));
    await user.click(editor().getByRole("tab", { name: "Accents" }));
    expect(hex).toHaveAttribute("aria-invalid", "true");

    await user.click(
      editor().getByRole("button", { name: "Restore the saved value of Primary accent" }),
    );
    expect(hex).toHaveValue("#ff8800");
    expect(hex).toHaveAttribute("aria-invalid", "false");
  });

  it("changes colors with the color picker", async () => {
    const user = await openEditor();
    fireEvent.change(editor().getByLabelText("Background color picker"), {
      target: { value: "#123456" },
    });
    expect(editor().getByRole("textbox", { name: "Background" })).toHaveValue("#123456");
    expect(preview().style.getPropertyValue("--color-app-bg")).toBe("#123456");
    await user.click(editor().getByRole("button", { name: "Save" }));
    expect((stored[0]?.tokens as ThemeTokens).colors.appBg).toBe("#123456");
  });

  it("turns glow off at 0 and changes shadows with the effect sliders", async () => {
    const user = await openEditor();
    await user.click(editor().getByRole("tab", { name: "Borders and effects" }));
    fireEvent.change(editor().getByRole("slider", { name: "Glow intensity" }), {
      target: { value: "0" },
    });
    expect(preview().style.getPropertyValue("--glow-accent")).toBe("0 0 0 transparent");
    expect(preview().style.getPropertyValue("--text-glow")).toBe("none");
    fireEvent.change(editor().getByRole("slider", { name: "Shadow intensity" }), {
      target: { value: "20" },
    });
    expect(preview().style.getPropertyValue("--shadow-elevated")).toBe(
      "0 6px 20px rgb(0 0 0 / 20%)",
    );
    fireEvent.change(editor().getByRole("slider", { name: "Border emphasis" }), {
      target: { value: "100" },
    });
    expect(preview().style.getPropertyValue("--color-border")).not.toBe("#3b2863");
    expect(editor().getByText("0%")).toBeVisible();
  });

  it("resets every change, then cancels without saving or applying anything", async () => {
    const user = await openEditor();
    const name = editor().getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "Renamed");
    fireEvent.change(editor().getByLabelText("Surface color picker"), {
      target: { value: "#000000" },
    });
    await user.click(editor().getByRole("button", { name: "Reset changes" }));

    expect(editor().getByRole("textbox", { name: "Name" })).toHaveValue("Sunset");
    expect(editor().getByRole("textbox", { name: "Surface" })).toHaveValue("#1c1231");
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
    expect(editor().getByRole("button", { name: "Reset changes" })).toBeDisabled();

    fireEvent.change(editor().getByLabelText("Surface color picker"), {
      target: { value: "#000000" },
    });
    await user.click(editor().getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("update_custom_theme")).toEqual([]);
    expect((stored[0]?.tokens as ThemeTokens).colors.surface).toBe("#1c1231");
    expect(document.documentElement.getAttribute("style") ?? "").toBe("");
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");

    // Reopened: the saved values, not the cancelled ones.
    await user.click(screen.getByRole("button", { name: "Edit Sunset" }));
    expect(editor().getByRole("textbox", { name: "Surface" })).toHaveValue("#1c1231");
  });

  it("asks before discarding changes with Escape", async () => {
    const user = await openEditor();
    fireEvent.change(editor().getByLabelText("Surface color picker"), {
      target: { value: "#000000" },
    });
    await user.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog", { name: "Discard changes" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(editor().getByRole("textbox", { name: "Surface" })).toHaveValue("#000000");
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("update_custom_theme")).toEqual([]);
  });

  it("saves the name and tokens with Save", async () => {
    const user = await openEditor();
    const name = editor().getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "Sunrise");
    await user.selectOptions(editor().getByRole("combobox", { name: "Color scheme" }), "light");
    await user.click(editor().getByRole("button", { name: "Save" }));

    const [update] = calls("update_custom_theme");
    expect(update).toMatchObject({ id: THEME_ID, name: "Sunrise", tokensVersion: TOKENS_VERSION });
    expect((update?.tokens as ThemeTokens).colorScheme).toBe("light");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(await screen.findByRole("radio", { name: "Sunrise" })).toBeInTheDocument();
  });
});
