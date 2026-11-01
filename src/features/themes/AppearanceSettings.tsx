import { useMemo, useState, type CSSProperties, type ReactNode } from "react";
import {
  DEFAULT_THEME,
  THEMES,
  customSelection,
  customThemeIdOf,
  isThemeId,
  type ThemeId,
  type ThemeSelection,
} from "../../app/theme/themes";
import {
  TOKENS_VERSION,
  copyTokens,
  themeProperties,
  type ThemeTokens,
} from "../../app/theme/themeTokens";
import {
  builtinSources,
  resolveTheme,
  useCustomThemes,
  type CustomThemeEntry,
  type ThemeSource,
} from "./customThemesContext";
import { buildThemeDocument, parseThemeDocument } from "./themeFile";
import { uniqueThemeName } from "./themeNames";
import { ThemeEditor } from "./ThemeEditor";
import { ThemeNameModal } from "./ThemeNameModal";
import {
  chooseThemeFile,
  createCustomTheme,
  deleteCustomTheme,
  exportThemeFile,
  renameCustomTheme,
} from "./themesApi";

interface AppearanceSettingsProps {
  theme: ThemeSelection;
  onThemeChange: (theme: ThemeSelection) => void;
}

type Dialog =
  | { kind: "clone"; source: ThemeSource }
  | { kind: "rename"; entry: CustomThemeEntry }
  | {
      kind: "import";
      fileName: string;
      name: string;
      basedOn: ThemeId | null;
      tokens: ThemeTokens;
    };

interface Notice {
  kind: "success" | "error";
  text: string;
}

const DEFAULT_LABEL = THEMES.find((theme) => theme.id === DEFAULT_THEME)?.label ?? DEFAULT_THEME;

function describeError(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * Settings → Appearance: the built-in and custom themes, with Clone, Edit,
 * Rename, Delete, Import and Export. Built-in themes are never changed;
 * every custom theme is an independent copy edited in the Theme Editor.
 */
export function AppearanceSettings({ theme, onThemeChange }: AppearanceSettingsProps) {
  const custom = useCustomThemes();
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const builtins = useMemo(() => builtinSources(), []);
  const entries = useMemo(() => custom.themes ?? [], [custom.themes]);
  const allNames = useMemo(
    () => [...THEMES.map((option) => option.label), ...entries.map((entry) => entry.theme.name)],
    [entries],
  );
  const namesExcept = (name: string) => {
    const index = allNames.indexOf(name);
    return allNames.filter((_, position) => position !== index);
  };
  const resolved = resolveTheme(theme, custom.themes, custom.loadError !== null);
  const available = custom.themes !== null;

  function sourceOf(entry: CustomThemeEntry): ThemeSource {
    return {
      selection: customSelection(entry.theme.id),
      name: entry.theme.name,
      builtin: false,
      tokens: entry.tokens,
    };
  }

  function originOf(entry: CustomThemeEntry): string {
    const source = entry.theme.sourceThemeId;
    if (source === undefined) return "Imported custom theme.";
    if (isThemeId(source)) {
      return `Custom theme, cloned from ${THEMES.find((option) => option.id === source)?.label ?? source}.`;
    }
    const sourceId = customThemeIdOf(source as ThemeSelection);
    const parent = sourceId ? custom.byId(sourceId) : undefined;
    return parent
      ? `Custom theme, cloned from ${parent.theme.name}.`
      : "Custom theme, cloned from a theme that no longer exists.";
  }

  async function exportTheme(source: ThemeSource) {
    if (!source.tokens) return;
    setNotice(null);
    const basedOn = source.builtin
      ? source.selection
      : custom.byId(customThemeIdOf(source.selection) ?? "")?.theme.sourceThemeId;
    try {
      const path = await exportThemeFile(
        source.name,
        buildThemeDocument(source.name, source.tokens, basedOn),
      );
      if (path !== null)
        setNotice({ kind: "success", text: `“${source.name}” was exported to ${path}.` });
    } catch (cause) {
      setNotice({ kind: "error", text: describeError(cause) });
    }
  }

  async function startImport() {
    setNotice(null);
    try {
      const file = await chooseThemeFile();
      if (file === null) return;
      const parsed = parseThemeDocument(file.document);
      if (!parsed.ok) {
        setNotice({ kind: "error", text: `${file.fileName} cannot be imported: ${parsed.error}` });
        return;
      }
      setDialog({
        kind: "import",
        fileName: file.fileName,
        name: parsed.name,
        basedOn: parsed.basedOn,
        tokens: parsed.tokens,
      });
    } catch (cause) {
      setNotice({ kind: "error", text: describeError(cause) });
    }
  }

  async function confirmDelete(entry: CustomThemeEntry) {
    setNotice(null);
    try {
      custom.replace(await deleteCustomTheme(entry.theme.id));
      setDeletingId(null);
      // Explicit fallback: deleting the theme in use selects the default theme.
      if (theme === customSelection(entry.theme.id)) {
        onThemeChange(DEFAULT_THEME);
        setNotice({
          kind: "success",
          text: `“${entry.theme.name}” was deleted. ${DEFAULT_LABEL} is now used.`,
        });
      } else {
        setNotice({ kind: "success", text: `“${entry.theme.name}” was deleted.` });
      }
    } catch (cause) {
      setNotice({ kind: "error", text: describeError(cause) });
    }
  }

  const editing = editingId ? custom.byId(editingId) : undefined;

  return (
    <section className="panel" aria-label="Appearance">
      {resolved.kind === "builtin" && resolved.unavailable ? (
        <p className="message message--warning" role="status">
          The selected custom theme is not available, so {DEFAULT_LABEL} is shown. Choose a theme
          below.
        </p>
      ) : null}
      {custom.loadError ? (
        <div className="message message--error" role="alert">
          <p className="appearance__error-text">
            The custom themes cannot be read: {describeError(custom.loadError)} The file is kept
            unchanged; built-in themes remain available.
          </p>
          <button
            type="button"
            className="button button--secondary button--small"
            onClick={custom.reload}
          >
            Try again
          </button>
        </div>
      ) : null}

      <fieldset className="choice-group theme-options">
        <legend>Built-in themes</legend>
        {builtins.map((source) => {
          const option = THEMES.find((entry) => entry.id === source.selection);
          return (
            <ThemeOption
              key={source.selection}
              selection={source.selection}
              name={source.name}
              description={option?.description ?? ""}
              checked={theme === source.selection}
              onSelect={onThemeChange}
              preview={<ThemePreview builtin={source.selection as ThemeId} />}
            >
              <button
                type="button"
                className="button button--secondary button--small"
                disabled={!available}
                aria-label={`Clone ${source.name}`}
                onClick={() => {
                  setNotice(null);
                  setDialog({ kind: "clone", source });
                }}
              >
                Clone
              </button>
              <button
                type="button"
                className="button button--secondary button--small"
                aria-label={`Export ${source.name}`}
                onClick={() => void exportTheme(source)}
              >
                Export
              </button>
            </ThemeOption>
          );
        })}
      </fieldset>

      <fieldset className="choice-group theme-options">
        <legend>Custom themes</legend>
        {available && entries.length === 0 ? (
          <p className="message message--muted theme-options__empty">
            No custom themes yet. Clone a theme to create one, or import a theme file.
          </p>
        ) : null}
        {entries.map((entry) => {
          const source = sourceOf(entry);
          const usable = entry.tokens !== null;
          return (
            <ThemeOption
              key={entry.theme.id}
              selection={source.selection}
              name={entry.theme.name}
              description={
                usable
                  ? originOf(entry)
                  : "Made by a newer version of YMDB, or damaged: it cannot be used or edited here and is kept unchanged."
              }
              checked={theme === source.selection}
              disabled={!usable}
              onSelect={onThemeChange}
              preview={
                entry.tokens ? (
                  <ThemePreview tokens={entry.tokens} />
                ) : (
                  <span className="theme-preview" />
                )
              }
            >
              {deletingId === entry.theme.id ? (
                <div
                  className="confirm-box theme-option__confirm"
                  role="alertdialog"
                  aria-label={`Delete ${entry.theme.name}`}
                >
                  <p className="message">
                    Delete “{entry.theme.name}”? This cannot be undone.
                    {theme === source.selection ? ` ${DEFAULT_LABEL} will be used instead.` : ""}
                  </p>
                  <div className="actions">
                    <button
                      type="button"
                      className="button button--small"
                      onClick={() => void confirmDelete(entry)}
                    >
                      Delete
                    </button>
                    <button
                      type="button"
                      className="button button--secondary button--small"
                      onClick={() => {
                        setDeletingId(null);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <>
                  <button
                    type="button"
                    className="button button--small"
                    disabled={!usable}
                    aria-label={`Edit ${entry.theme.name}`}
                    onClick={() => {
                      setNotice(null);
                      setEditingId(entry.theme.id);
                    }}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    disabled={!usable}
                    aria-label={`Clone ${entry.theme.name}`}
                    onClick={() => {
                      setNotice(null);
                      setDialog({ kind: "clone", source });
                    }}
                  >
                    Clone
                  </button>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    aria-label={`Rename ${entry.theme.name}`}
                    onClick={() => {
                      setNotice(null);
                      setDialog({ kind: "rename", entry });
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    disabled={!usable}
                    aria-label={`Export ${entry.theme.name}`}
                    onClick={() => void exportTheme(source)}
                  >
                    Export
                  </button>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    aria-label={`Delete ${entry.theme.name}`}
                    onClick={() => {
                      setNotice(null);
                      setDeletingId(entry.theme.id);
                    }}
                  >
                    Delete
                  </button>
                </>
              )}
            </ThemeOption>
          );
        })}
      </fieldset>

      <div className="actions">
        <button
          type="button"
          className="button button--secondary"
          disabled={!available}
          onClick={() => void startImport()}
        >
          Import theme…
        </button>
      </div>
      {notice ? (
        <p
          className={
            notice.kind === "error" ? "message message--error" : "message message--success"
          }
          role={notice.kind === "error" ? "alert" : "status"}
        >
          {notice.text}
        </p>
      ) : null}
      <p className="message message--muted">
        The theme applies immediately to the whole application. Built-in themes cannot be changed:
        clone one to create a custom theme you can edit. Interface preferences are the same for
        every MAME installation.
      </p>

      {dialog?.kind === "clone" ? (
        <ThemeNameModal
          title={`Clone ${dialog.source.name}`}
          initialName={uniqueThemeName(`${dialog.source.name} copy`, allNames)}
          takenNames={allNames}
          submitLabel="Clone and edit"
          onClose={() => {
            setDialog(null);
          }}
          onSubmit={async (name) => {
            const tokens = dialog.source.tokens;
            if (!tokens) return;
            // A full copy: later changes of the source never affect the clone.
            const created = await createCustomTheme(
              name,
              dialog.source.selection,
              TOKENS_VERSION,
              copyTokens(tokens),
            );
            custom.replace(created.themes);
            setDialog(null);
            setEditingId(created.id);
          }}
        >
          <p className="message message--muted">
            The new theme starts as an exact copy of {dialog.source.name}, independent from it.
          </p>
        </ThemeNameModal>
      ) : null}

      {dialog?.kind === "rename" ? (
        <ThemeNameModal
          title={`Rename ${dialog.entry.theme.name}`}
          initialName={dialog.entry.theme.name}
          takenNames={namesExcept(dialog.entry.theme.name)}
          submitLabel="Rename"
          onClose={() => {
            setDialog(null);
          }}
          onSubmit={async (name) => {
            custom.replace(await renameCustomTheme(dialog.entry.theme.id, name));
            setDialog(null);
          }}
        />
      ) : null}

      {dialog?.kind === "import" ? (
        <ThemeNameModal
          title="Import theme"
          initialName={uniqueThemeName(dialog.name, allNames)}
          takenNames={allNames}
          submitLabel="Import"
          onClose={() => {
            setDialog(null);
          }}
          onSubmit={async (name) => {
            const created = await createCustomTheme(
              name,
              dialog.basedOn,
              TOKENS_VERSION,
              dialog.tokens,
            );
            custom.replace(created.themes);
            setDialog(null);
            setNotice({ kind: "success", text: `${dialog.fileName} was imported as “${name}”.` });
          }}
        >
          <p className="message message--muted">
            {dialog.fileName} becomes a new custom theme. Existing themes are never replaced.
          </p>
        </ThemeNameModal>
      ) : null}

      {editing?.tokens ? (
        <ThemeEditor
          key={editing.theme.id}
          theme={editing.theme}
          tokens={editing.tokens}
          takenNames={namesExcept(editing.theme.name)}
          onSaved={custom.replace}
          onClose={() => {
            setEditingId(null);
          }}
        />
      ) : null}
    </section>
  );
}

interface ThemeOptionProps {
  selection: ThemeSelection;
  name: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onSelect: (selection: ThemeSelection) => void;
  preview: ReactNode;
  children: ReactNode;
}

function ThemeOption({
  selection,
  name,
  description,
  checked,
  disabled = false,
  onSelect,
  preview,
  children,
}: ThemeOptionProps) {
  const key = selection.replace(":", "-");
  const nameId = `theme-${key}-name`;
  const descriptionId = `theme-${key}-description`;
  return (
    <div className="theme-option">
      <label className="theme-option__choice">
        <input
          type="radio"
          name="theme"
          value={selection}
          checked={checked}
          disabled={disabled}
          aria-labelledby={nameId}
          aria-describedby={descriptionId}
          onChange={() => {
            onSelect(selection);
          }}
        />
        {preview}
        <span className="theme-option__text">
          <span id={nameId} className="theme-option__name">
            {name}
          </span>
          <span id={descriptionId} className="theme-option__description">
            {description}
          </span>
        </span>
      </label>
      <div className="theme-option__actions">{children}</div>
    </div>
  );
}

/** A small sample of a theme, drawn with that theme's own tokens. */
function ThemePreview({ builtin, tokens }: { builtin?: ThemeId; tokens?: ThemeTokens }) {
  const style = useMemo(
    () => (tokens ? (themeProperties(tokens) as CSSProperties) : undefined),
    [tokens],
  );
  return (
    <span className="theme-preview" data-theme-preview={builtin} style={style} aria-hidden="true">
      <span className="theme-preview__sidebar" />
      <span className="theme-preview__surface">
        <span className="theme-preview__line" />
        <span className="theme-preview__line theme-preview__line--muted" />
        <span className="theme-preview__accents">
          <span className="theme-preview__accent" />
          <span className="theme-preview__accent theme-preview__accent--secondary" />
        </span>
      </span>
    </span>
  );
}
