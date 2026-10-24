import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  updateControllerAssignment,
  type ControllerProfile,
  type ControlsResolution,
} from "./controllersApi";
import { describeSource } from "./controlsMessages";
import type { PreferenceScope } from "./useControls";

interface ControlsSelectProps {
  profiles: ControllerProfile[];
  /** The saved controls of the target; `null` while unknown. */
  resolution: ControlsResolution | null;
  software: boolean;
  /** The temporary controller profile of the next launch; `null` for the saved one. */
  value: string | null;
  onChange: (profileId: string | null) => void;
  disabled: boolean;
  /** The controller profiles could not be loaded (see Advanced → Controls). */
  unavailable?: boolean;
}

/**
 * The Controls selector of the launch bar. Like the launch profile, a choice
 * applies to the launches from this page only and is never saved.
 */
export function ControlsSelect({
  profiles,
  resolution,
  software,
  value,
  onChange,
  disabled,
  unavailable = false,
}: ControlsSelectProps) {
  const saved = unavailable
    ? "Unavailable"
    : resolution
      ? `${resolution.profileName} — ${describeSource(resolution.profileSource, software)}`
      : "Saved preference";
  const selected = profiles.find((profile) => profile.id === value);
  const title = unavailable
    ? "The controller profiles could not be loaded: see Advanced, Controls"
    : selected
      ? `Controller profile: ${selected.name} — temporary for this launch`
      : `Controller profile: ${saved}`;
  return (
    <label className="launch-bar__profile launch-bar__controls" title={title}>
      <span className="launch-bar__label">Controls</span>
      <select
        className="input"
        value={value ?? ""}
        disabled={disabled}
        onChange={(event) => {
          onChange(event.target.value === "" ? null : event.target.value);
        }}
      >
        <option value="">{saved}</option>
        {profiles
          .filter((profile) => profile.id !== resolution?.profileId)
          .map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profile.name}
            </option>
          ))}
      </select>
    </label>
  );
}

interface SaveControlsPreferenceProps {
  /** The temporary controller profile. */
  profile: ControllerProfile;
  scopes: PreferenceScope[];
  /** Called once the preference is saved. */
  onSaved: (scope: PreferenceScope) => void;
}

/**
 * Shown while a temporary controller profile is selected: says that it is
 * not saved and offers to save it, at a scope chosen explicitly. Only the
 * profile of the chosen level changes; its other fields are kept.
 */
export function SaveControlsPreference({ profile, scopes, onSaved }: SaveControlsPreferenceProps) {
  const [open, setOpen] = useState(false);
  const [scopeIndex, setScopeIndex] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const name = useId();
  const scope = scopes[scopeIndex] ?? scopes[0];

  async function save() {
    if (!scope) return;
    setSaving(true);
    setError(null);
    try {
      await updateControllerAssignment(scope.target, {
        profileId: { kind: "set", value: profile.id },
      });
      setOpen(false);
      onSaved(scope);
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="controls-save" role="group" aria-label="Temporary controls">
      <p className="message message--muted">
        Controls: <strong>{profile.name}</strong> for the launches from this page only; nothing is
        saved.{" "}
        {!open && scopes.length > 0 && (
          <button
            type="button"
            className="link-button"
            onClick={() => {
              setOpen(true);
            }}
          >
            Save as preference…
          </button>
        )}
      </p>
      {open && (
        <form
          className="inline-form"
          aria-label="Save as preference"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <fieldset className="choice-group">
            <legend>Save {profile.name} as the controller profile of</legend>
            {scopes.map((entry, index) => (
              <label key={entry.target.level} className="choice">
                <input
                  type="radio"
                  name={name}
                  checked={index === scopeIndex}
                  onChange={() => {
                    setScopeIndex(index);
                  }}
                />
                {entry.label}
              </label>
            ))}
          </fieldset>
          {error && (
            <p className="message message--error" role="alert">
              {error}
            </p>
          )}
          <div className="actions">
            <button type="submit" className="button" disabled={saving}>
              {saving ? "Saving…" : "Save preference"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setOpen(false);
                setError(null);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
    </div>
  );
}
