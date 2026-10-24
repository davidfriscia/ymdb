import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  createControllerProfile,
  deleteControllerProfile,
  importControllerProfileBase,
  MAME_PROFILE_ID,
  removeControllerProfileBase,
  renameControllerProfile,
  type AssignmentTarget,
  type ControllerProfile,
  type ControllersView,
} from "./controllersApi";
import { inUseReferences, profileReferences, validateName } from "./controlsMessages";
import { References } from "./References";

interface BlockProps {
  view: ControllersView;
  /** Called after every saved change. */
  onChanged: () => void;
}

/** Settings → Controllers: the controller profiles. */
export function ProfilesBlock({ view, onChanged }: BlockProps) {
  const defaultProfileId = view.assignments.default.profileId ?? MAME_PROFILE_ID;
  return (
    <>
      <h3 className="panel__subtitle">Controller Profiles</h3>
      <p className="message message--muted">
        A controller profile is an input configuration for MAME. Each profile has its own MAME
        configuration directory per MAME installation, where MAME saves the input assignments made
        in its menus, and optionally a baseline controller configuration file.
      </p>
      <CreateProfileForm onChanged={onChanged} />
      <ul className="controllers-list" aria-label="Controller profiles">
        {view.profiles.map((profile) => (
          <ProfileItem
            key={profile.id}
            view={view}
            profile={profile}
            isDefault={profile.id === defaultProfileId}
            onChanged={onChanged}
          />
        ))}
      </ul>
    </>
  );
}

function CreateProfileForm({ onChanged }: { onChanged: () => void }) {
  const [name, setName] = useState("");
  const [importBase, setImportBase] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ text: string; tone: "error" | "muted" } | null>(null);
  const id = useId();

  async function create() {
    const invalid = validateName(name);
    setNameError(invalid);
    if (invalid) return;
    setSaving(true);
    setMessage(null);
    try {
      const created = await createControllerProfile(name.trim(), importBase);
      if (created === null) {
        setMessage({ text: "No file was chosen: the profile was not created.", tone: "muted" });
        return;
      }
      setName("");
      setImportBase(false);
      onChanged();
    } catch (error) {
      setMessage({ text: describeError(error), tone: "error" });
    } finally {
      setSaving(false);
    }
  }

  return (
    <form
      className="inline-form"
      aria-label="Create controller profile"
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <div className="field">
        <label className="field__label" htmlFor={`${id}-name`}>
          New controller profile
        </label>
        <input
          id={`${id}-name`}
          className="input"
          maxLength={60}
          placeholder="Profile name, for example Arcade Six Buttons"
          value={name}
          aria-invalid={nameError !== null}
          aria-describedby={nameError ? `${id}-name-error` : undefined}
          onChange={(event) => {
            setName(event.target.value);
            setNameError(null);
          }}
        />
        {nameError && (
          <span id={`${id}-name-error`} className="message message--error">
            {nameError}
          </span>
        )}
      </div>
      <label className="choice">
        <input
          type="checkbox"
          checked={importBase}
          onChange={(event) => {
            setImportBase(event.target.checked);
          }}
        />
        Start from a MAME controller configuration file (.cfg), chosen next
      </label>
      {message && (
        <p
          className={`message message--${message.tone}`}
          role={message.tone === "error" ? "alert" : "status"}
        >
          {message.text}
        </p>
      )}
      <div className="actions">
        <button type="submit" className="button" disabled={saving}>
          {saving ? "Creating…" : importBase ? "Choose file and create" : "Create profile"}
        </button>
      </div>
    </form>
  );
}

type Busy = "rename" | "import" | "removeBase" | "delete";
type Confirming = "removeBase" | "delete" | null;

interface ProfileItemProps {
  view: ControllersView;
  profile: ControllerProfile;
  isDefault: boolean;
  onChanged: () => void;
}

function ProfileItem({ view, profile, isDefault, onChanged }: ProfileItemProps) {
  const [busy, setBusy] = useState<Busy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [blockers, setBlockers] = useState<AssignmentTarget[] | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState<Confirming>(null);
  const titleId = useId();
  const system = profile.kind === "system";
  const references = profileReferences(view, profile.id);
  // The default is shown by its badge; it still prevents a deletion.
  const keyed = references.filter((target) => target.level !== "default");

  /** Runs a change; resolves to whether it was saved. */
  async function run(action: Busy, work: () => Promise<unknown>): Promise<boolean> {
    setBusy(action);
    setError(null);
    setBlockers(null);
    try {
      await work();
      onChanged();
      return true;
    } catch (failure) {
      setError(describeError(failure));
      setBlockers(inUseReferences(failure));
      return false;
    } finally {
      setBusy(null);
    }
  }

  async function importBase() {
    setBusy("import");
    setError(null);
    try {
      // `null`: the user closed the file dialog; nothing changed.
      if ((await importControllerProfileBase(profile.id)) !== null) onChanged();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(null);
    }
  }

  return (
    <li className="controllers-list__item" aria-labelledby={titleId} aria-busy={busy !== null}>
      <p id={titleId} className="installation-list__title">
        {profile.name} {system && <span className="badge">System</span>}{" "}
        {isDefault && <span className="badge badge--success">Default</span>}
      </p>
      {system ? (
        <p className="message message--muted">
          This profile uses MAME&apos;s existing configuration without overriding its controller
          settings.
        </p>
      ) : profile.hasBase ? (
        <dl className="details-list">
          <dt>Baseline file</dt>
          <dd>
            <span className="code wrap">{profile.baseFileName ?? "base.cfg"}</span>
            <span className="message--muted">
              {" "}
              — MAME starts from its inputs. Changes made in MAME&apos;s menus are saved in the
              profile&apos;s configuration directory, not in this file.
            </span>
          </dd>
        </dl>
      ) : (
        <p className="message message--muted">
          No baseline file: MAME starts from its default inputs. Changes made in MAME&apos;s menus
          are saved in the profile&apos;s configuration directory.
        </p>
      )}
      {profile.baseError !== null && (
        <p className="message message--error" role="alert">
          The baseline file cannot be used: {profile.baseError}. Launches with this profile are
          refused until a corrected file is imported; the current file is kept as it is.
        </p>
      )}
      {keyed.length > 0 && (
        <>
          <p className="message message--muted">
            Used by {keyed.length} saved {keyed.length === 1 ? "assignment" : "assignments"}:
          </p>
          <References label={`Assignments using ${profile.name}`} targets={keyed} />
        </>
      )}

      {!system && !renaming && (
        <div className="actions">
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={busy !== null}
            onClick={() => {
              setRenaming(true);
              setConfirming(null);
            }}
          >
            Rename…
          </button>
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={busy !== null}
            onClick={() => void importBase()}
          >
            {busy === "import"
              ? "Importing…"
              : profile.hasBase
                ? "Replace baseline file…"
                : "Import baseline file…"}
          </button>
          {profile.hasBase && (
            <button
              type="button"
              className="button button--secondary button--small"
              aria-expanded={confirming === "removeBase"}
              disabled={busy !== null}
              onClick={() => {
                setConfirming(confirming === "removeBase" ? null : "removeBase");
              }}
            >
              Remove baseline file…
            </button>
          )}
          <button
            type="button"
            className="button button--secondary button--small"
            aria-expanded={confirming === "delete"}
            disabled={busy !== null}
            onClick={() => {
              setConfirming(confirming === "delete" ? null : "delete");
            }}
          >
            Delete…
          </button>
        </div>
      )}

      {renaming && (
        <RenameForm
          profile={profile}
          busy={busy === "rename"}
          onSave={async (name) => {
            if (await run("rename", () => renameControllerProfile(profile.id, name))) {
              setRenaming(false);
            }
          }}
          onCancel={() => {
            setRenaming(false);
          }}
        />
      )}

      {error && (
        <div role="alert">
          <p className="message message--error">{error}</p>
          {blockers && blockers.length > 0 && (
            <References label="Assignments preventing the change" targets={blockers} />
          )}
        </div>
      )}

      {confirming === "removeBase" && (
        <div className="confirm-box" role="group" aria-label="Remove baseline file">
          <p className="message message--warning">
            Remove the baseline file of {profile.name}? The profile continues to exist and the input
            assignments MAME saved in its configuration directory stay available, but the effective
            inputs may change because MAME no longer starts from this baseline. MAME files are not
            modified.
          </p>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy !== null}
              onClick={() =>
                void run("removeBase", () => removeControllerProfileBase(profile.id)).then(
                  (saved) => {
                    if (saved) setConfirming(null);
                  },
                )
              }
            >
              {busy === "removeBase" ? "Removing…" : "Remove baseline file"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setConfirming(null);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {confirming === "delete" &&
        (references.length > 0 ? (
          <div className="confirm-box" role="group" aria-label="Delete controller profile">
            <p className="message message--warning">
              {profile.name} cannot be deleted while saved assignments use it. Change or remove them
              first, in Default Assignments below or on the detail pages: YMDB does not change them
              for you.
            </p>
            <References label={`Assignments using ${profile.name}`} targets={references} />
            <div className="actions">
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  setConfirming(null);
                }}
              >
                Close
              </button>
            </div>
          </div>
        ) : (
          <div className="confirm-box" role="group" aria-label="Delete controller profile">
            <p className="message message--warning">
              Delete the controller profile {profile.name}? The MAME configuration saved in its
              directories and its baseline file are kept on disk.
            </p>
            <div className="actions">
              <button
                type="button"
                className="button"
                disabled={busy !== null}
                onClick={() =>
                  void run("delete", () => deleteControllerProfile(profile.id)).then((saved) => {
                    if (saved) setConfirming(null);
                  })
                }
              >
                {busy === "delete" ? "Deleting…" : "Delete profile"}
              </button>
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  setConfirming(null);
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        ))}
    </li>
  );
}

interface RenameFormProps {
  profile: ControllerProfile;
  busy: boolean;
  onSave: (name: string) => Promise<void>;
  onCancel: () => void;
}

function RenameForm({ profile, busy, onSave, onCancel }: RenameFormProps) {
  const [name, setName] = useState(profile.name);
  const [error, setError] = useState<string | null>(null);
  const id = useId();
  return (
    <form
      className="inline-form"
      aria-label={`Rename ${profile.name}`}
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        const invalid = validateName(name);
        setError(invalid);
        if (!invalid) void onSave(name.trim());
      }}
    >
      <div className="field">
        <label className="field__label" htmlFor={`${id}-name`}>
          Name
        </label>
        <input
          id={`${id}-name`}
          className="input"
          maxLength={60}
          value={name}
          aria-invalid={error !== null}
          onChange={(event) => {
            setName(event.target.value);
            setError(null);
          }}
        />
        {error && <span className="message message--error">{error}</span>}
      </div>
      <div className="actions">
        <button type="submit" className="button" disabled={busy}>
          {busy ? "Saving…" : "Save name"}
        </button>
        <button type="button" className="button button--secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
