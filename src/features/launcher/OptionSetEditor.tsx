import { useId, useState } from "react";
import type { OptionDefinition, OptionSet } from "./launcherApi";
import { CATEGORY_LABELS, initialValue, omit, parseValue, valueToText } from "./optionValues";

interface OptionSetEditorProps {
  /** Accessible name of the editor, for example "Global options". */
  label: string;
  definitions: OptionDefinition[];
  options: OptionSet;
  onChange: (options: OptionSet) => void;
  /** Reports whether some edited value is currently invalid. */
  onValidityChange?: (valid: boolean) => void;
}

/**
 * Edits the options set at one level. Options that are not listed stay under
 * MAME's own configuration.
 */
export function OptionSetEditor({
  label,
  definitions,
  options,
  onChange,
  onValidityChange,
}: OptionSetEditorProps) {
  const [invalid, setInvalid] = useState<ReadonlySet<string>>(new Set());
  const set = definitions.filter((definition) => options[definition.name] !== undefined);
  const available = definitions.filter((definition) => options[definition.name] === undefined);

  function markValidity(name: string, valid: boolean) {
    const next = new Set(invalid);
    if (valid) {
      next.delete(name);
    } else {
      next.add(name);
    }
    setInvalid(next);
    onValidityChange?.(next.size === 0);
  }

  function remove(name: string) {
    markValidity(name, true);
    onChange(omit(options, name));
  }

  return (
    <div className="option-editor" role="group" aria-label={label}>
      {set.length === 0 ? (
        <p className="message message--muted">No options set: MAME uses its own configuration.</p>
      ) : (
        <ul className="option-editor__list">
          {set.map((definition) => (
            <li key={definition.name} className="option-editor__row">
              <OptionInput
                definition={definition}
                options={options}
                onChange={onChange}
                onValidityChange={(valid) => {
                  markValidity(definition.name, valid);
                }}
              />
              <button
                type="button"
                className="button button--secondary button--small"
                aria-label={`Remove ${definition.label}`}
                onClick={() => {
                  remove(definition.name);
                }}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      {available.length > 0 && (
        <label className="option-editor__add">
          <span>Add option</span>
          <select
            className="input"
            value=""
            onChange={(event) => {
              const definition = definitions.find((entry) => entry.name === event.target.value);
              if (!definition) return;
              const value = initialValue(definition.kind);
              onChange({ ...options, [definition.name]: value });
              // Empty texts and path lists must be filled in before saving.
              if (value === "" || (Array.isArray(value) && value.length === 0)) {
                markValidity(definition.name, false);
              }
            }}
          >
            <option value="">Choose an option…</option>
            {(Object.keys(CATEGORY_LABELS) as (keyof typeof CATEGORY_LABELS)[]).map((category) => {
              const entries = available.filter((definition) => definition.category === category);
              if (entries.length === 0) return null;
              return (
                <optgroup key={category} label={CATEGORY_LABELS[category]}>
                  {entries.map((definition) => (
                    <option key={definition.name} value={definition.name}>
                      {definition.label} (-{definition.name})
                    </option>
                  ))}
                </optgroup>
              );
            })}
          </select>
        </label>
      )}
    </div>
  );
}

interface OptionInputProps {
  definition: OptionDefinition;
  options: OptionSet;
  onChange: (options: OptionSet) => void;
  onValidityChange: (valid: boolean) => void;
}

function OptionInput({ definition, options, onChange, onValidityChange }: OptionInputProps) {
  const { name, kind } = definition;
  const value = options[name];
  const [text, setText] = useState(valueToText(value));
  const [error, setError] = useState<string | null>(null);
  // Unique: several editors (global options, profiles, installations) can be mounted at once.
  const id = `option-${name}-${useId()}`;

  function update(next: string) {
    setText(next);
    const parsed = parseValue(kind, next);
    if ("error" in parsed) {
      setError(parsed.error);
      onValidityChange(false);
      return;
    }
    setError(null);
    onValidityChange(true);
    onChange({ ...options, [name]: parsed.value });
  }

  return (
    <div className="option-editor__field">
      <label htmlFor={id} className="option-editor__label">
        {definition.label} <span className="code message--muted">-{name}</span>
      </label>
      {kind.type === "boolean" ? (
        <select
          id={id}
          className="input"
          value={value === false ? "false" : "true"}
          onChange={(event) => {
            update(event.target.value);
          }}
        >
          <option value="true">On (-{name})</option>
          <option value="false">Off (-no{name})</option>
        </select>
      ) : kind.type === "pathList" ? (
        <textarea
          id={id}
          className="input option-editor__paths"
          rows={2}
          placeholder="One path per line"
          value={text}
          aria-invalid={error !== null}
          onChange={(event) => {
            update(event.target.value);
          }}
        />
      ) : (
        <input
          id={id}
          className="input"
          type={kind.type === "integer" || kind.type === "float" ? "number" : "text"}
          step={kind.type === "float" ? "any" : undefined}
          min={kind.type === "integer" || kind.type === "float" ? kind.min : undefined}
          max={kind.type === "integer" || kind.type === "float" ? kind.max : undefined}
          value={text}
          aria-invalid={error !== null}
          onChange={(event) => {
            update(event.target.value);
          }}
        />
      )}
      <span className="option-editor__hint">{error ?? definition.description}</span>
    </div>
  );
}
