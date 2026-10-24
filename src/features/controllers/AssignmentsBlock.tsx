import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  MAME_PROFILE_ID,
  resetControllerAssignment,
  updateControllerAssignment,
  type AssignmentTarget,
  type ControllersView,
  type Players,
} from "./controllersApi";
import { PlayersEditor } from "./PlayersEditor";
import {
  assignmentEntries,
  describePlayers,
  describeTarget,
  LEVEL_LABELS,
  profileName,
  trimPlayers,
} from "./controlsMessages";

interface BlockProps {
  view: ControllersView;
  onChanged: () => void;
}

/** Saved assignments listed at once; a search narrows longer lists. */
const SHOWN_ASSIGNMENTS = 200;

/** Settings → Controllers: the default controller profile and the saved assignments. */
export function AssignmentsBlock({ view, onChanged }: BlockProps) {
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const selectId = useId();
  const defaults = view.assignments.default;
  const entries = assignmentEntries(view.assignments).filter(
    ({ target }) => target.level !== "default",
  );
  const needle = query.trim().toLowerCase();
  const matching = needle
    ? entries.filter(({ target }) => describeTarget(target).toLowerCase().includes(needle))
    : entries;

  /** Saves a change; `id` identifies the control that is busy meanwhile. */
  async function run(id: string, work: () => Promise<unknown>) {
    setSaving(id);
    setError(null);
    try {
      await work();
      onChanged();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setSaving(null);
    }
  }

  return (
    <>
      <h3 className="panel__subtitle">Default Assignments</h3>
      <div className="field">
        <label className="field__label" htmlFor={selectId}>
          Default controller profile
        </label>
        <select
          id={selectId}
          className="input"
          value={defaults.profileId ?? MAME_PROFILE_ID}
          disabled={saving !== null}
          onChange={(event) => {
            const value = event.target.value;
            void run("default", () =>
              updateControllerAssignment(
                { level: "default" },
                { profileId: { kind: "set", value } },
              ),
            );
          }}
        >
          {view.profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profile.name}
            </option>
          ))}
        </select>
        <span className="message message--muted">
          Applies to every launch without a more specific preference. Preferences saved for a
          machine, a software or a software on a given machine take precedence; set them from the
          detail pages. Launch profiles are independent of controller profiles.
        </span>
      </div>
      <DefaultPlayers
        // A saved change starts from the new value.
        key={JSON.stringify(defaults.players ?? null)}
        view={view}
        stored={defaults.players}
        busy={saving !== null}
        onSave={(players) =>
          void run("default-players", () =>
            updateControllerAssignment(
              { level: "default" },
              {
                players:
                  players === undefined ? { kind: "inherit" } : { kind: "set", value: players },
              },
            ),
          )
        }
      />
      {saving === "default" && (
        <p className="message message--muted" role="status">
          Saving…
        </p>
      )}

      <h4 className="controllers-settings__heading">Saved assignments</h4>
      {entries.length === 0 ? (
        <p className="message message--muted">
          No machine or software has its own controller preference.
        </p>
      ) : (
        <>
          <input
            className="input controllers-settings__search"
            type="search"
            aria-label="Find a saved assignment"
            placeholder="Find a machine or software"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
            }}
          />
          <ul className="controllers-assignments" aria-label="Saved assignments">
            {matching.slice(0, SHOWN_ASSIGNMENTS).map(({ target, assignment }) => {
              const id = assignmentId(target);
              return (
                <li key={id} className="controllers-assignments__item">
                  <span className="badge">{LEVEL_LABELS[target.level]}</span>{" "}
                  <span className="code">{targetName(target)}</span>
                  <span className="message--muted">
                    {" · "}
                    {assignment.profileId !== undefined
                      ? `Controller profile: ${profileName(view, assignment.profileId)}`
                      : "Controller profile inherited"}
                    {assignment.players !== undefined &&
                      ` · Players: ${describePlayers(view, assignment.players)}`}
                  </span>{" "}
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    aria-label={`Remove the assignment of ${describeTarget(target)}`}
                    disabled={saving !== null}
                    onClick={() => void run(id, () => resetControllerAssignment(target))}
                  >
                    {saving === id ? "Removing…" : "Remove"}
                  </button>
                </li>
              );
            })}
          </ul>
          {matching.length === 0 && (
            <p className="message message--muted">No saved assignment matches.</p>
          )}
          {matching.length > SHOWN_ASSIGNMENTS && (
            <p className="message message--muted">
              {SHOWN_ASSIGNMENTS} of {matching.length} assignments shown: search to find the others.
            </p>
          )}
          <p className="message message--muted">
            Assignments of machines or software missing from a catalog are kept.
          </p>
        </>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
    </>
  );
}

interface DefaultPlayersProps {
  view: ControllersView;
  /** `undefined` when the default assigns no player list. */
  stored: Players | undefined;
  busy: boolean;
  /** `undefined` removes the default players. */
  onSave: (players: Players | undefined) => void;
}

/**
 * The default players of every launch. Saving changes only the players:
 * the default controller profile is kept.
 */
function DefaultPlayers({ view, stored, busy, onSave }: DefaultPlayersProps) {
  const [draft, setDraft] = useState<Players>(stored ?? []);
  const changed = JSON.stringify(trimPlayers(draft)) !== JSON.stringify(stored ?? null);
  return (
    <div className="field">
      <PlayersEditor
        view={view}
        value={draft}
        onChange={setDraft}
        label="Default players"
        disabled={busy}
      />
      <span className="message message--muted">
        {stored === undefined
          ? "No default players: MAME numbers the joysticks in the order the provider reports them."
          : `Saved: ${describePlayers(view, stored)}.`}{" "}
        Machines and software can assign their own players from their detail pages.
      </span>
      <div className="actions">
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={busy || !changed}
          onClick={() => {
            onSave(trimPlayers(draft));
          }}
        >
          Save default players
        </button>
        {stored !== undefined && (
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={busy}
            onClick={() => {
              onSave(undefined);
            }}
          >
            Remove default players
          </button>
        )}
      </div>
    </div>
  );
}

function assignmentId(target: AssignmentTarget): string {
  return `${target.level}:${target.key ?? ""}`;
}

/** The MAME names of a target, as stored. */
function targetName(target: AssignmentTarget): string {
  const key = target.key ?? "";
  if (target.level !== "machineSoftware") return key;
  const [machine = "", software = ""] = key.split("/");
  return `${software} on ${machine}`;
}
