import { useCallback, useId, useState } from "react";
import {
  COLOR_GROUPS,
  COLOR_TOKENS,
  EFFECTS,
  TOKENS_VERSION,
  copyTokens,
  sameTokens,
  type ColorGroup,
  type ColorScheme,
  type ColorToken,
  type EffectToken,
  type Scrollbars,
  type ThemeTokens,
} from "../../app/theme/themeTokens";
import { Modal } from "../../shared/Modal";
import { ThemeEditorPreview } from "./ThemeEditorPreview";
import { themeNameError } from "./themeNames";
import { updateCustomTheme, type CustomTheme } from "./themesApi";
import { ColorField, EffectField } from "./TokenFields";

interface ThemeEditorProps {
  theme: CustomTheme;
  /** The saved tokens: the starting point, Reset changes and Restore. */
  tokens: ThemeTokens;
  /** Names of every other theme, built-in and custom. */
  takenNames: readonly string[];
  onSaved: (themes: CustomTheme[]) => void;
  onClose: () => void;
}

/**
 * Edits a custom theme. Changes are drawn only in the preview of the editor:
 * nothing is applied to the application or saved before Save, so Cancel
 * leaves everything exactly as it was.
 */
export function ThemeEditor({ theme, tokens, takenNames, onSaved, onClose }: ThemeEditorProps) {
  const id = useId();
  const [name, setName] = useState(theme.name);
  const [draft, setDraft] = useState<ThemeTokens>(() => copyTokens(tokens));
  const [invalid, setInvalid] = useState<ReadonlySet<ColorToken>>(new Set());
  const [group, setGroup] = useState<ColorGroup>("backgrounds");
  // Remounts the fields, so that a reset also clears the values being typed.
  const [resetVersion, setResetVersion] = useState(0);
  const [confirmingDiscard, setConfirmingDiscard] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const nameError = themeNameError(name, takenNames);
  const dirty = name.trim() !== theme.name || !sameTokens(draft, tokens);
  const canSave = dirty && nameError === null && invalid.size === 0 && !saving;

  const onColorChange = useCallback((token: ColorToken, value: string) => {
    setDraft((current) =>
      current.colors[token] === value
        ? current
        : { ...current, colors: { ...current.colors, [token]: value } },
    );
  }, []);
  const onInvalidChange = useCallback((token: ColorToken, isInvalid: boolean) => {
    setInvalid((current) => {
      if (current.has(token) === isInvalid) return current;
      const next = new Set(current);
      if (isInvalid) next.add(token);
      else next.delete(token);
      return next;
    });
  }, []);
  const onEffectChange = useCallback((effect: EffectToken, value: number) => {
    setDraft((current) =>
      current.effects[effect] === value
        ? current
        : { ...current, effects: { ...current.effects, [effect]: value } },
    );
  }, []);

  function reset() {
    setName(theme.name);
    setDraft(copyTokens(tokens));
    setInvalid(new Set());
    setResetVersion((version) => version + 1);
    setError(null);
  }

  function requestClose() {
    if (dirty && !saving) setConfirmingDiscard(true);
    else onClose();
  }

  async function save() {
    if (!canSave) return;
    setSaving(true);
    setError(null);
    try {
      const themes = await updateCustomTheme(theme.id, name.trim(), TOKENS_VERSION, draft);
      onSaved(themes);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setSaving(false);
    }
  }

  return (
    <Modal title={`Edit theme: ${theme.name}`} onClose={requestClose} size="wide">
      <div className="theme-editor">
        <div className="theme-editor__controls">
          <div className="theme-editor__general">
            <div className="field">
              <label className="field__label" htmlFor={`${id}-name`}>
                Name
              </label>
              <input
                id={`${id}-name`}
                className="input"
                value={name}
                aria-invalid={nameError !== null}
                aria-describedby={nameError ? `${id}-name-error` : undefined}
                onChange={(event) => {
                  setName(event.target.value);
                }}
              />
              {nameError ? (
                <span id={`${id}-name-error`} className="token-field__error">
                  {nameError}
                </span>
              ) : null}
            </div>
            <div className="field">
              <label className="field__label" htmlFor={`${id}-scheme`}>
                Color scheme
              </label>
              <select
                id={`${id}-scheme`}
                className="input"
                value={draft.colorScheme}
                onChange={(event) => {
                  const colorScheme = event.target.value as ColorScheme;
                  setDraft((current) => ({ ...current, colorScheme }));
                }}
              >
                <option value="light">Light (native controls and scrollbars)</option>
                <option value="dark">Dark (native controls and scrollbars)</option>
              </select>
            </div>
            <div className="field">
              <label className="field__label" htmlFor={`${id}-scrollbars`}>
                Scrollbars
              </label>
              <select
                id={`${id}-scrollbars`}
                className="input"
                value={draft.scrollbars}
                onChange={(event) => {
                  const scrollbars = event.target.value as Scrollbars;
                  setDraft((current) => ({ ...current, scrollbars }));
                }}
              >
                <option value="system">System style</option>
                <option value="themed">Scrollbar colors of the theme</option>
              </select>
            </div>
          </div>

          <div className="theme-editor__tabs" role="tablist" aria-label="Token groups">
            {COLOR_GROUPS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                role="tab"
                id={`${id}-tab-${entry.id}`}
                aria-selected={entry.id === group}
                aria-controls={`${id}-panel-${entry.id}`}
                className="theme-editor__tab"
                onClick={() => {
                  setGroup(entry.id);
                }}
              >
                {entry.label}
              </button>
            ))}
          </div>

          {/* Every panel stays mounted, so that a value being typed survives a tab change. */}
          <div key={resetVersion}>
            {COLOR_GROUPS.map((entry) => {
              const effects = EFFECTS.filter((effect) => effect.group === entry.id);
              return (
                <div
                  key={entry.id}
                  id={`${id}-panel-${entry.id}`}
                  role="tabpanel"
                  aria-labelledby={`${id}-tab-${entry.id}`}
                  className="theme-editor__fields"
                  hidden={entry.id !== group}
                >
                  {COLOR_TOKENS.filter((token) => token.group === entry.id).map((token) => (
                    <ColorField
                      key={token.key}
                      token={token.key}
                      label={token.label}
                      description={token.description}
                      value={draft.colors[token.key]}
                      original={tokens.colors[token.key]}
                      onChange={onColorChange}
                      onInvalidChange={onInvalidChange}
                    />
                  ))}
                  {effects.length > 0 ? (
                    <h3 className="theme-editor__subheading">
                      {entry.id === "typography" ? "Spacing" : "Effects"}
                    </h3>
                  ) : null}
                  {effects.map((effect) => (
                    <EffectField
                      key={effect.key}
                      effect={effect.key}
                      value={draft.effects[effect.key]}
                      original={tokens.effects[effect.key]}
                      onChange={onEffectChange}
                    />
                  ))}
                </div>
              );
            })}
          </div>
        </div>

        <div className="theme-editor__preview">
          <h3 className="theme-editor__subheading">Preview</h3>
          <ThemeEditorPreview tokens={draft} />
          <p className="message message--muted">
            Changes are shown here only. Save applies them; Cancel discards them.
          </p>
        </div>
      </div>

      <div className="theme-editor__footer">
        {error ? (
          <p className="message message--error" role="alert">
            {error}
          </p>
        ) : null}
        {invalid.size > 0 ? (
          <p className="message message--warning">Fix the invalid HEX values before saving.</p>
        ) : null}
        {confirmingDiscard ? (
          <div className="confirm-box" role="alertdialog" aria-label="Discard changes">
            <p className="message">Discard the unsaved changes of this theme?</p>
            <div className="actions">
              <button type="button" className="button" onClick={onClose}>
                Discard changes
              </button>
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  setConfirmingDiscard(false);
                }}
              >
                Keep editing
              </button>
            </div>
          </div>
        ) : (
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={!canSave}
              onClick={() => void save()}
            >
              {saving ? "Saving…" : "Save"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              disabled={!dirty || saving}
              onClick={reset}
            >
              Reset changes
            </button>
            <button type="button" className="button button--secondary" onClick={onClose}>
              Cancel
            </button>
          </div>
        )}
      </div>
    </Modal>
  );
}
