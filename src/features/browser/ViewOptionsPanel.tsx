import type { BrowserPreferences } from "./browserPreferences";
import { RENDERERS, rendererById } from "./renderers/registry";
import type { RendererOptionDefinition } from "./renderers/types";

interface ViewOptionsPanelProps {
  preferences: BrowserPreferences;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
}

/**
 * View mode and the options of the selected renderer, built from the renderer
 * definitions: registering a renderer needs no change here.
 */
export function ViewOptionsPanel({ preferences, onPreferencesChange }: ViewOptionsPanelProps) {
  const renderer = rendererById(preferences.renderer);
  const values = preferences.rendererOptions[renderer.id] ?? {};

  function setOption(key: string, value: number | boolean | string) {
    onPreferencesChange({
      rendererOptions: {
        ...preferences.rendererOptions,
        [renderer.id]: { ...values, [key]: value },
      },
    });
  }

  return (
    <aside className="view-panel" aria-label="View options">
      <fieldset className="view-panel__section">
        <legend className="view-panel__title">View mode</legend>
        <div className="view-modes" role="radiogroup" aria-label="View mode">
          {RENDERERS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="radio"
              aria-checked={entry.id === renderer.id}
              className="view-modes__option"
              onClick={() => {
                onPreferencesChange({ renderer: entry.id });
              }}
            >
              <svg
                viewBox="0 0 24 24"
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d={entry.icon} />
              </svg>
              <span>{entry.label}</span>
            </button>
          ))}
        </div>
      </fieldset>

      {renderer.options.length > 0 && (
        <fieldset className="view-panel__section">
          <legend className="view-panel__title">{renderer.label} options</legend>
          {renderer.options.map((definition) => (
            <OptionControl
              key={`${renderer.id}:${definition.key}`}
              definition={definition}
              value={values[definition.key] ?? definition.default}
              onChange={(value) => {
                setOption(definition.key, value);
              }}
            />
          ))}
        </fieldset>
      )}
    </aside>
  );
}

interface OptionControlProps {
  definition: RendererOptionDefinition;
  value: number | boolean | string;
  onChange: (value: number | boolean | string) => void;
}

function OptionControl({ definition, value, onChange }: OptionControlProps) {
  switch (definition.type) {
    case "range":
      return (
        <label className="view-panel__option">
          <span className="view-panel__label">
            {definition.label}
            <span className="view-panel__value">
              {String(value)}
              {definition.unit ? ` ${definition.unit}` : ""}
            </span>
          </span>
          <input
            type="range"
            min={definition.min}
            max={definition.max}
            step={definition.step}
            value={Number(value)}
            onChange={(event) => {
              onChange(Number(event.target.value));
            }}
          />
        </label>
      );
    case "toggle":
      return (
        <label className="view-panel__option view-panel__option--toggle">
          <input
            type="checkbox"
            checked={value === true}
            onChange={(event) => {
              onChange(event.target.checked);
            }}
          />
          <span>{definition.label}</span>
        </label>
      );
    case "choice":
      return (
        <label className="view-panel__option">
          <span className="view-panel__label">{definition.label}</span>
          <select
            className="input"
            value={String(value)}
            onChange={(event) => {
              onChange(event.target.value);
            }}
          >
            {definition.choices.map((choice) => (
              <option key={choice.value} value={choice.value}>
                {choice.label}
              </option>
            ))}
          </select>
        </label>
      );
  }
}
