import { useMemo, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { CommandPreview } from "./CommandPreview";
import type { LaunchConfig, LaunchRequest, LaunchTarget, OptionSet } from "./launcherApi";
import { softwareKey } from "./launcherApi";
import { latestSession, useLauncher } from "./launcherContext";
import { OptionSetEditor } from "./OptionSetEditor";
import { SessionStatus } from "./SessionStatus";

interface LaunchControlsProps {
  target: LaunchTarget;
  /** The launch without the profile; `null` while the user must still choose. */
  request: Omit<LaunchRequest, "profileId"> | null;
  /** Shown before the buttons, for example the machine selection. */
  children?: ReactNode;
}

/** Play button, profile, overrides, command line preview and session status. */
export function LaunchControls({ target, request, children }: LaunchControlsProps) {
  const launcher = useLauncher();
  const { view, sessions, start, stop, showCommandPreview, setShowCommandPreview } = launcher;
  const [profileId, setProfileId] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const profiles = view?.config.profiles ?? [];
  // A profile deleted in the meantime falls back to the default profile.
  const selectedProfile =
    profileId !== null && profiles.some((profile) => profile.id === profileId) ? profileId : null;
  const fullRequest = useMemo<LaunchRequest | null>(
    () =>
      request ? { ...request, ...(selectedProfile ? { profileId: selectedProfile } : {}) } : null,
    [request, selectedProfile],
  );
  const session = latestSession(sessions, target);
  const running = session?.state.state === "running";
  const overrideLabel = target.software ? "this software" : "this machine";
  const overrides = view ? overridesOf(view.config, target) : {};
  const overrideCount = Object.keys(overrides).length;

  async function play() {
    if (!fullRequest) return;
    setStarting(true);
    setError(null);
    try {
      await start(fullRequest);
    } catch (startError) {
      setError(describeError(startError));
    } finally {
      setStarting(false);
    }
  }

  return (
    <section className="launch-panel" aria-label="Launch">
      {children}
      <div className="launch-panel__row">
        <button
          type="button"
          className="button launch-panel__play"
          disabled={!fullRequest || starting || running || !view}
          onClick={() => void play()}
        >
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path d="M4 2.5v11l9-5.5z" fill="currentColor" />
          </svg>
          {starting ? "Starting…" : running ? "Running" : "Play"}
        </button>
        <label className="launch-panel__field">
          <span>Profile</span>
          <select
            className="input"
            value={selectedProfile ?? ""}
            disabled={!view}
            onChange={(event) => {
              setProfileId(event.target.value === "" ? null : event.target.value);
            }}
          >
            <option value="">
              Default (
              {profiles.find((profile) => profile.id === view?.config.defaultProfileId)?.name ??
                "Default"}
              )
            </option>
            {profiles
              .filter((profile) => profile.id !== view?.config.defaultProfileId)
              .map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
          </select>
        </label>
        <button
          type="button"
          className="button button--secondary"
          aria-pressed={showCommandPreview}
          onClick={() => {
            setShowCommandPreview(!showCommandPreview);
          }}
        >
          Command line
        </button>
        <button
          type="button"
          className="button button--secondary"
          aria-expanded={editing}
          disabled={!view}
          onClick={() => {
            setEditing(!editing);
          }}
        >
          Options for {overrideLabel}
          {overrideCount > 0 && <span className="counter">{overrideCount}</span>}
        </button>
      </div>

      {launcher.error !== null && (
        <p className="message message--error" role="alert">
          The launch options could not be loaded: {describeError(launcher.error)}
        </p>
      )}
      {view?.loadError && (
        <p className="message message--warning" role="alert">
          {view.loadError}. The default options are used until they are saved in Settings.
        </p>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      {session && <SessionStatus session={session} onStop={stop} />}

      {editing && view && (
        <OverrideEditor
          key={launcher.configVersion}
          label={`Options for ${overrideLabel}`}
          config={view.config}
          target={target}
          onClose={() => {
            setEditing(false);
          }}
        />
      )}

      {showCommandPreview &&
        (fullRequest ? (
          <CommandPreview request={fullRequest} configVersion={launcher.configVersion} />
        ) : (
          <p className="message message--muted">Choose what to launch to see the command line.</p>
        ))}
    </section>
  );
}

function overridesOf(config: LaunchConfig, target: LaunchTarget): OptionSet {
  return (
    (target.software
      ? config.softwareOverrides[softwareKey(target.software.list, target.software.name)]
      : config.machineOverrides[target.machine]) ?? {}
  );
}

function withOverrides(
  config: LaunchConfig,
  target: LaunchTarget,
  options: OptionSet,
): LaunchConfig {
  if (target.software) {
    const key = softwareKey(target.software.list, target.software.name);
    return { ...config, softwareOverrides: { ...config.softwareOverrides, [key]: options } };
  }
  return { ...config, machineOverrides: { ...config.machineOverrides, [target.machine]: options } };
}

interface OverrideEditorProps {
  label: string;
  config: LaunchConfig;
  target: LaunchTarget;
  onClose: () => void;
}

function OverrideEditor({ label, config, target, onClose }: OverrideEditorProps) {
  const { view, saveConfig } = useLauncher();
  const [draft, setDraft] = useState<OptionSet>(() => overridesOf(config, target));
  const [valid, setValid] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await saveConfig(withOverrides(config, target, draft));
      onClose();
    } catch (saveError) {
      setError(describeError(saveError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="launch-panel__overrides">
      <p className="message message--muted">
        These options take precedence over the global options and the profile. Precedence: MAME
        configuration, global, profile, machine, software.
      </p>
      <OptionSetEditor
        label={label}
        definitions={view?.definitions ?? []}
        options={draft}
        onChange={setDraft}
        onValidityChange={setValid}
      />
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={!valid || saving}
          onClick={() => void save()}
        >
          {saving ? "Saving…" : "Save options"}
        </button>
        <button type="button" className="button button--secondary" onClick={onClose}>
          Cancel
        </button>
      </div>
    </div>
  );
}
