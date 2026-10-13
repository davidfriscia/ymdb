import { useId, useMemo, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { CommandPreview } from "./CommandPreview";
import type {
  LaunchConfig,
  LaunchRequest,
  LaunchTarget,
  OptionDefinition,
  OptionSet,
} from "./launcherApi";
import { softwareKey } from "./launcherApi";
import { latestSession, useLauncher } from "./launcherContext";
import { OptionSetEditor } from "./OptionSetEditor";
import { SessionStatus } from "./SessionStatus";
import { useInstallationScope } from "../installations/installationsContext";
import { RomDownloadSection } from "../roms/RomDownloadSection";

interface LaunchControlsProps {
  target: LaunchTarget;
  /**
   * The launch without the installation and the profile; `null` while the
   * user must still choose.
   */
  request: Omit<LaunchRequest, "profileId" | "installationId"> | null;
  /** Shown before the buttons, for example the machine selection. */
  children?: ReactNode;
}

/**
 * Play button, profile, ROM downloads and session status, for the
 * installation whose catalog is browsed, with the overrides and the command
 * line preview in a collapsed Advanced section.
 */
export function LaunchControls({ target, request, children }: LaunchControlsProps) {
  const installationId = useInstallationScope();
  const launcher = useLauncher();
  const { sessions, start, stop, showCommandPreview, setShowCommandPreview } = launcher;
  // The configuration of another installation is never used, even briefly
  // while switching.
  const view = launcher.view?.installationId === installationId ? launcher.view : null;
  const [profileId, setProfileId] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);
  // Mounted once opened, so that closing the section keeps an unsaved draft.
  const [advancedOpened, setAdvancedOpened] = useState(false);
  const advancedId = useId();
  const [editing, setEditing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const profiles = view?.config.profiles ?? [];
  // A profile deleted in the meantime falls back to the default profile.
  const selectedProfile =
    profileId !== null && profiles.some((profile) => profile.id === profileId) ? profileId : null;
  const fullRequest = useMemo<LaunchRequest | null>(
    () =>
      request
        ? {
            ...request,
            installationId,
            ...(selectedProfile ? { profileId: selectedProfile } : {}),
          }
        : null,
    [request, installationId, selectedProfile],
  );
  const session = latestSession(sessions, target, installationId);
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
      </div>

      <RomDownloadSection request={fullRequest} target={target} />

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

      {/* Overrides and the command line: collapsed by default, below the main actions. */}
      <div className="launch-panel__advanced">
        <button
          type="button"
          className="launch-panel__disclosure"
          aria-expanded={advanced}
          aria-controls={advancedId}
          onClick={() => {
            setAdvanced(!advanced);
            setAdvancedOpened(true);
          }}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" strokeWidth="1.75" />
          </svg>
          Advanced
          {overrideCount > 0 && (
            <span
              className="counter"
              title={`${String(overrideCount)} options for ${overrideLabel}`}
            >
              {overrideCount}
            </span>
          )}
        </button>
        <div id={advancedId} className="launch-panel__advanced-body" hidden={!advanced}>
          {advancedOpened && (
            <>
              <div className="launch-panel__row">
                <button
                  type="button"
                  className="button button--secondary button--small"
                  aria-expanded={editing}
                  disabled={!view}
                  onClick={() => {
                    setEditing(!editing);
                  }}
                >
                  Options for {overrideLabel}
                  {overrideCount > 0 && <span className="counter">{overrideCount}</span>}
                </button>
                <button
                  type="button"
                  className="button button--secondary button--small"
                  aria-pressed={showCommandPreview}
                  onClick={() => {
                    setShowCommandPreview(!showCommandPreview);
                  }}
                >
                  Command line
                </button>
              </div>

              {editing && view && (
                <OverrideEditor
                  key={launcher.configVersion}
                  label={`Options for ${overrideLabel}`}
                  installationId={installationId}
                  definitions={view.definitions}
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
                  <p className="message message--muted">
                    Choose what to launch to see the command line.
                  </p>
                ))}
            </>
          )}
        </div>
      </div>
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
  installationId: string;
  definitions: OptionDefinition[];
  config: LaunchConfig;
  target: LaunchTarget;
  onClose: () => void;
}

function OverrideEditor({
  label,
  installationId,
  definitions,
  config,
  target,
  onClose,
}: OverrideEditorProps) {
  const { saveConfig } = useLauncher();
  const [draft, setDraft] = useState<OptionSet>(() => overridesOf(config, target));
  const [valid, setValid] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await saveConfig(installationId, withOverrides(config, target, draft));
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
        definitions={definitions}
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
