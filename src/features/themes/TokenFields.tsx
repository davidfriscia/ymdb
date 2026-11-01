import { memo, useId, useState } from "react";
import {
  effectDefinition,
  parseHexColor,
  type ColorToken,
  type EffectToken,
} from "../../app/theme/themeTokens";

interface ColorFieldProps {
  token: ColorToken;
  label: string;
  description: string;
  value: string;
  /** The saved value, restored by the restore button. */
  original: string;
  onChange: (token: ColorToken, value: string) => void;
  /** Reports whether the typed HEX value is invalid (and not applied). */
  onInvalidChange: (token: ColorToken, invalid: boolean) => void;
}

/**
 * One color: a color picker and a HEX field, both showing the same value.
 * Only valid colors reach the theme; an invalid HEX value is reported and
 * leaves the theme unchanged.
 */
export const ColorField = memo(function ColorField({
  token,
  label,
  description,
  value,
  original,
  onChange,
  onInvalidChange,
}: ColorFieldProps) {
  const id = useId();
  // The text being typed, while it differs from the value.
  const [typed, setTyped] = useState<string | null>(null);
  const invalid = typed !== null && parseHexColor(typed) === null;

  function setText(text: string | null) {
    setTyped(text);
    onInvalidChange(token, text !== null && parseHexColor(text) === null);
  }

  return (
    <div className="token-field">
      <div className="token-field__text">
        <label className="token-field__label" htmlFor={`${id}-hex`}>
          {label}
        </label>
        <span id={`${id}-description`} className="token-field__description">
          {description}
        </span>
      </div>
      <div className="token-field__controls">
        <input
          type="color"
          className="token-field__picker"
          value={value}
          aria-label={`${label} color picker`}
          onChange={(event) => {
            const color = parseHexColor(event.target.value);
            if (color) {
              setText(null);
              onChange(token, color);
            }
          }}
        />
        <input
          id={`${id}-hex`}
          type="text"
          className="input token-field__hex"
          value={typed ?? value}
          spellCheck={false}
          autoComplete="off"
          maxLength={7}
          aria-describedby={invalid ? `${id}-error` : `${id}-description`}
          aria-invalid={invalid}
          onChange={(event) => {
            const text = event.target.value;
            const color = parseHexColor(text);
            if (color) onChange(token, color);
            setText(text);
          }}
          onBlur={() => {
            // A valid value is shown normalized; an invalid one stays to be fixed.
            if (!invalid) setText(null);
          }}
        />
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={value === original && !invalid}
          aria-label={`Restore the saved value of ${label}`}
          title="Restore the saved value"
          onClick={() => {
            setText(null);
            onChange(token, original);
          }}
        >
          Restore
        </button>
      </div>
      {invalid ? (
        <p id={`${id}-error`} className="token-field__error" role="alert">
          Enter a HEX color such as #1f5fbf or #fff.
        </p>
      ) : null}
    </div>
  );
});

interface EffectFieldProps {
  effect: EffectToken;
  value: number;
  original: number;
  onChange: (effect: EffectToken, value: number) => void;
}

/** One numeric effect: a slider within the range of its definition. */
export const EffectField = memo(function EffectField({
  effect,
  value,
  original,
  onChange,
}: EffectFieldProps) {
  const id = useId();
  const definition = effectDefinition(effect);
  return (
    <div className="token-field">
      <div className="token-field__text">
        <label className="token-field__label" htmlFor={id}>
          {definition.label}
        </label>
        <span id={`${id}-description`} className="token-field__description">
          {definition.description}
        </span>
      </div>
      <div className="token-field__controls">
        <input
          id={id}
          type="range"
          className="token-field__slider"
          min={definition.min}
          max={definition.max}
          step={definition.step}
          value={value}
          aria-describedby={`${id}-description`}
          aria-valuetext={definition.format(value)}
          onChange={(event) => {
            const next = Number(event.target.value);
            if (Number.isInteger(next) && next >= definition.min && next <= definition.max) {
              onChange(effect, next);
            }
          }}
        />
        <output className="token-field__value" htmlFor={id}>
          {definition.format(value)}
        </output>
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={value === original}
          aria-label={`Restore the saved value of ${definition.label}`}
          title="Restore the saved value"
          onClick={() => {
            onChange(effect, original);
          }}
        >
          Restore
        </button>
      </div>
    </div>
  );
});
