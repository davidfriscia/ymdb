import { useState } from "react";
import { describeError } from "../../shared/ipc";
import type { LoadState } from "../../shared/useLoader";
import {
  updateControllerAssignment,
  type Assignment,
  type ControllersView,
  type ControlsResolution,
} from "./controllersApi";
import { describePlayers, describeSource, profileName } from "./controlsMessages";
import type { PreferenceScope } from "./useControls";

interface ControlsPreferencesProps {
  view: ControllersView;
  /** The saved controls of the target; `null` while no machine is chosen. */
  resolution: LoadState<ControlsResolution | null>;
  scopes: PreferenceScope[];
  software: boolean;
  /** The temporary controller profile of the next launch, if any. */
  temporary: string | null;
  onChanged: () => void;
  onConfigure: () => void;
  /** Why Configure Controls in MAME is unavailable, if it is. */
  configureUnavailable: string | null;
}

/**
 * Advanced → Controls: the effective controller profile of the machine or
 * software and where it comes from (resolved by the backend), the saved
 * preference of each level with Change and Reset to inherited, and the
 * guided configuration in MAME.
 */
export function ControlsPreferences({
  view,
  resolution,
  scopes,
  software,
  temporary,
  onChanged,
  onConfigure,
  configureUnavailable,
}: ControlsPreferencesProps) {
  const resolved = resolution.status === "ready" ? resolution.value : null;
  // The stored assignment of each level: from the resolution when there is
  // one, else (no machine chosen yet) from the configuration.
  const stored = (scope: PreferenceScope): Assignment | undefined => {
    const level = resolved?.levels.find(
      (entry) => entry.level === scope.target.level && entry.key === scope.target.key,
    );
    if (level) return level.assignment ?? undefined;
    const key = scope.target.key ?? "";
    switch (scope.target.level) {
      case "machine":
        return view.assignments.machines[key];
      case "software":
        return view.assignments.software[key];
      case "machineSoftware":
        return view.assignments.machineSoftware[key];
      case "default":
        return view.assignments.default;
    }
  };

  return (
    <div className="controls-panel" role="group" aria-label="Controls">
      {resolution.status === "loading" && (
        <p className="message message--muted" role="status">
          Resolving the controls…
        </p>
      )}
      {resolution.status === "error" && (
        <p className="message message--error" role="alert">
          The controls could not be resolved: {describeError(resolution.error)}
        </p>
      )}
      {resolution.status === "ready" && resolved === null && (
        <p className="message message--muted">
          Choose a machine to see the effective controller profile. The software preference can be
          set meanwhile.
        </p>
      )}
      {resolved && (
        <dl className="details-list">
          <dt>Controller profile</dt>
          <dd>
            <strong>{resolved.profileName}</strong> —{" "}
            {describeSource(resolved.profileSource, software)}
          </dd>
          <dt>MAME configuration</dt>
          <dd>
            {resolved.cfgDirectory ? (
              <>
                <span className="code wrap">{resolved.cfgDirectory}</span>
                <span className="message--muted">
                  {" "}
                  (created when MAME first starts with this profile)
                </span>
              </>
            ) : (
              "MAME's own configuration directories of this installation"
            )}
          </dd>
          {resolved.playersSource !== null && (
            <>
              <dt>Player preferences</dt>
              <dd>
                {describePlayers(view, resolved.players)} —{" "}
                {describeSource(resolved.playersSource, software)}. Stored only: not applied to MAME
                yet.
              </dd>
            </>
          )}
        </dl>
      )}
      {temporary !== null && (
        <p className="message message--warning">
          The next launch from this page uses the temporary selection {profileName(view, temporary)}{" "}
          instead (see Controls in the launch bar).
        </p>
      )}

      <h4 className="controls-panel__heading">Saved preferences</h4>
      <p className="message message--muted">
        A preference applies to every launch it matches. Precedence: default, machine, software,
        software on a given machine, then the temporary selection of the launch bar.
      </p>
      {scopes.map((scope) => {
        const assignment = stored(scope);
        return (
          <PreferenceRow
            // A saved change shows the new value.
            key={`${scope.target.level}:${assignment?.profileId ?? ""}`}
            view={view}
            scope={scope}
            assignment={assignment}
            onChanged={onChanged}
          />
        );
      })}

      <div className="actions">
        <button
          type="button"
          className="button button--secondary"
          disabled={configureUnavailable !== null}
          title={configureUnavailable ?? undefined}
          onClick={onConfigure}
        >
          Configure Controls in MAME…
        </button>
      </div>
    </div>
  );
}

interface PreferenceRowProps {
  view: ControllersView;
  scope: PreferenceScope;
  assignment: Assignment | undefined;
  onChanged: () => void;
}

/**
 * The saved controller profile of one level. Only the profile field changes:
 * player preferences stored at the same level are kept.
 */
function PreferenceRow({ view, scope, assignment, onChanged }: PreferenceRowProps) {
  const current = assignment?.profileId ?? null;
  const [draft, setDraft] = useState(current ?? "");
  const [saving, setSaving] = useState<"save" | "reset" | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run(action: "save" | "reset") {
    setSaving(action);
    setError(null);
    try {
      await updateControllerAssignment(scope.target, {
        profileId: action === "save" ? { kind: "set", value: draft } : { kind: "inherit" },
      });
      onChanged();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setSaving(null);
    }
  }

  return (
    <div className="controls-panel__level" role="group" aria-label={scope.label}>
      <p className="controls-panel__level-title">
        {scope.label}:{" "}
        {current !== null ? (
          <strong>{profileName(view, current)}</strong>
        ) : (
          <span className="message--muted">not set, inherited</span>
        )}
      </p>
      <div className="launch-panel__row">
        <select
          className="input"
          aria-label={`Controller profile for ${scope.label}`}
          value={draft}
          disabled={saving !== null}
          onChange={(event) => {
            setDraft(event.target.value);
          }}
        >
          {current === null && <option value="">Choose a controller profile…</option>}
          {view.profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profile.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={saving !== null || draft === "" || draft === current}
          onClick={() => void run("save")}
        >
          {saving === "save" ? "Saving…" : "Save"}
        </button>
        {current !== null && (
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={saving !== null}
            onClick={() => void run("reset")}
          >
            {saving === "reset" ? "Resetting…" : "Reset to inherited"}
          </button>
        )}
      </div>
      {assignment?.players !== undefined && (
        <p className="message message--muted">
          Player preferences saved here (kept when the profile changes, not applied to MAME yet):{" "}
          {describePlayers(view, assignment.players)}
        </p>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
