import { useId, useState, type ReactNode } from "react";
import { Modal } from "../../shared/Modal";
import { themeNameError } from "./themeNames";

interface ThemeNameModalProps {
  title: string;
  initialName: string;
  /** Names of every other theme: a taken name is refused, never replaced. */
  takenNames: readonly string[];
  submitLabel: string;
  children?: ReactNode;
  /** Resolves when done; a rejection is shown and keeps the window open. */
  onSubmit: (name: string) => Promise<void>;
  onClose: () => void;
}

/** Asks for the name of a theme being cloned, renamed or imported. */
export function ThemeNameModal({
  title,
  initialName,
  takenNames,
  submitLabel,
  children,
  onSubmit,
  onClose,
}: ThemeNameModalProps) {
  const id = useId();
  const [name, setName] = useState(initialName);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const nameError = themeNameError(name, takenNames);

  async function submit() {
    if (nameError !== null || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(name.trim());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setBusy(false);
    }
  }

  return (
    <Modal title={title} onClose={onClose} size="narrow">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        {children}
        <div className="field">
          <label className="field__label" htmlFor={`${id}-name`}>
            Theme name
          </label>
          <input
            id={`${id}-name`}
            className="input"
            value={name}
            aria-invalid={nameError !== null}
            aria-describedby={nameError ? `${id}-error` : undefined}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          {nameError ? (
            <span id={`${id}-error`} className="token-field__error">
              {nameError}
            </span>
          ) : null}
        </div>
        {error ? (
          <p className="message message--error" role="alert">
            {error}
          </p>
        ) : null}
        <div className="actions theme-name__actions">
          <button type="submit" className="button" disabled={nameError !== null || busy}>
            {submitLabel}
          </button>
          <button type="button" className="button button--secondary" onClick={onClose}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
