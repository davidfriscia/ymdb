import { useId, useMemo, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { PlayIcon } from "../../shared/icons";
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
import { Disclosure } from "./Disclosure";
import { LaunchStatus } from "./LaunchStatus";
import { useInstallationScope } from "../installations/installationsContext";
import { RomDownloadSection } from "../roms/RomDownloadSection";
import { ConfigureControlsDialog } from "../controllers/ConfigureControlsDialog";
import { ControlsPreferences } from "../controllers/ControlsPreferences";
import {
  ControlsSelect,
  SaveControlsPreference,
  TemporaryPlayers,
} from "../controllers/ControlsSelect";
import type { Players } from "../controllers/controllersApi";
import { describePlayers, describeSource, trimPlayers } from "../controllers/controlsMessages";
import { preferenceScopes, useControls } from "../controllers/useControls";

interface LaunchBarProps {
  /** Short information about the title, at the start of the bar. */
  summary?: ReactNode;
  /** The actions, at the end of the bar. */
  actions: ReactNode;
  /** A second row of the bar, for example the machine and media of a software. */
  details?: ReactNode;
}

/** The compact bar below the media: the title information and the launch actions. */
export function LaunchBar({ summary, actions, details }: LaunchBarProps) {
  return (
    <section className="launch-bar" aria-label="Launch">
      <div className="launch-bar__main">
        <div className="launch-bar__summary">{summary}</div>
        <div className="launch-bar__actions">{actions}</div>
      </div>
      {details && <div className="launch-bar__details">{details}</div>}
    </section>
  );
}

interface LaunchControlsProps {
  target: LaunchTarget;
  /**
   * The launch without the installation and the profile; `null` while the
   * user must still choose.
   */
  request: Omit<LaunchRequest, "profileId" | "installationId"> | null;
  /** Short information about the title, at the start of the launch bar. */
  summary?: ReactNode;
  /** Shown in the launch bar below the actions, for example the machine selection. */
  children?: ReactNode;
}

/**
 * The launch bar (ROM downloads, Play and the profile) for the installation
 * whose catalog is browsed, the collapsible launch status below it, and the
 * overrides and the command line preview in a collapsed Advanced section.
 */
export function LaunchControls({ target, request, summary, children }: LaunchControlsProps) {
  const installationId = useInstallationScope();
  const launcher = useLauncher();
  const {
    sessions,
    start,
    stop,
    showCommandPreview,
    setShowCommandPreview,
    controlsVersion,
    controlsChanged,
  } = launcher;
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
  // The temporary controller profile of the launches from this page; never saved.
  const [controlsProfileId, setControlsProfileId] = useState<string | null>(null);
  // The temporary players of the launches from this page; never saved.
  const [controlsPlayers, setControlsPlayers] = useState<Players | null>(null);
  const [playersOpen, setPlayersOpen] = useState(false);
  const [controlsNotice, setControlsNotice] = useState<string | null>(null);
  const [controlsOpen, setControlsOpen] = useState(false);
  const [configuring, setConfiguring] = useState(false);

  const controls = useControls(installationId, target);
  const controllers = controls.view.latest;
  const resolution =
    controls.resolution.state.status === "ready" ? controls.resolution.state.value : null;
  const scopes = useMemo(() => preferenceScopes(target), [target]);
  // A controller profile deleted in the meantime falls back to the saved one.
  const temporaryProfile =
    controllers?.profiles.find((profile) => profile.id === controlsProfileId) ?? null;
  const temporaryProfileId = temporaryProfile?.id ?? null;
  // Controllers deleted in the meantime leave their player without controller.
  const temporaryPlayers = useMemo<Players | null>(
    () =>
      controlsPlayers === null || !controllers
        ? controlsPlayers
        : trimPlayers(
            controlsPlayers.map((id) =>
              id !== null && controllers.controllers.some((entry) => entry.id === id) ? id : null,
            ),
          ),
    [controlsPlayers, controllers],
  );

  const profiles = view?.config.profiles ?? [];
  // A profile deleted in the meantime falls back to the default profile.
  const selectedProfile =
    profileId !== null && profiles.some((profile) => profile.id === profileId) ? profileId : null;
  const fullRequest = useMemo<LaunchRequest | null>(() => {
    if (!request) return null;
    const controls = {
      ...(temporaryProfileId ? { profileId: temporaryProfileId } : {}),
      ...(temporaryPlayers ? { players: temporaryPlayers } : {}),
    };
    return {
      ...request,
      installationId,
      ...(selectedProfile ? { profileId: selectedProfile } : {}),
      ...(temporaryProfileId || temporaryPlayers ? { controls } : {}),
    };
  }, [request, installationId, selectedProfile, temporaryProfileId, temporaryPlayers]);
  const session = latestSession(sessions, target, installationId);
  const running = session?.state.state === "running";
  const overrideLabel = target.software ? "this software" : "this machine";
  const overrides = view ? overridesOf(view.config, target) : {};
  const overrideCount = Object.keys(overrides).length;
  const defaultProfile =
    profiles.find((profile) => profile.id === view?.config.defaultProfileId)?.name ?? "Default";

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

  const messages: { text: string; tone: "error" | "warning" }[] = [];
  if (launcher.error !== null) {
    messages.push({
      text: `The launch options could not be loaded: ${describeError(launcher.error)}`,
      tone: "error",
    });
  }
  if (view?.loadError) {
    messages.push({
      text: `${view.loadError}. The default options are used until they are saved in Settings.`,
      tone: "warning",
    });
  }
  if (error) messages.push({ text: error, tone: "error" });

  const configureUnavailable = !fullRequest
    ? "Choose what to launch first"
    : running
      ? "MAME is already running"
      : !view
        ? "The launch options are not loaded"
        : !temporaryProfile && !resolution
          ? "The controls are not resolved yet"
          : null;
  const software = target.software !== null;
  const subject = target.software
    ? `${target.software.list}:${target.software.name} on ${target.machine}`
    : target.machine;

  const showPlayers = playersOpen && controllers !== null;
  const details =
    children || temporaryProfile || controlsNotice || showPlayers ? (
      <>
        {children}
        {showPlayers && (
          <TemporaryPlayers
            view={controllers}
            saved={resolution?.players ?? []}
            value={temporaryPlayers}
            onChange={(next) => {
              setControlsPlayers(next);
              setControlsNotice(null);
            }}
            scopes={scopes}
            onSaved={(scope) => {
              setControlsPlayers(null);
              setPlayersOpen(false);
              setControlsNotice(`The players are saved for: ${scope.label}.`);
              controlsChanged();
            }}
          />
        )}
        {temporaryProfile && (
          <SaveControlsPreference
            key={temporaryProfile.id}
            profile={temporaryProfile}
            scopes={scopes}
            onSaved={(scope) => {
              setControlsProfileId(null);
              setControlsNotice(
                `${temporaryProfile.name} is saved as the controller profile of: ${scope.label}.`,
              );
              controlsChanged();
            }}
          />
        )}
        {controlsNotice && (
          <p className="message message--success" role="status">
            {controlsNotice}
          </p>
        )}
      </>
    ) : undefined;

  const playLabel = starting ? "Starting…" : running ? "Running" : "Play";
  const playTitle = starting
    ? "Starting MAME…"
    : running
      ? "MAME is running: stop it from the launch status below"
      : !fullRequest
        ? "Play: choose what to launch first"
        : "Play: start MAME with the selected profile";

  return (
    <>
      <LaunchBar
        summary={summary}
        details={details}
        actions={
          <>
            <RomDownloadSection request={fullRequest} target={target} />
            {/* The main action: the only filled control of the bar. */}
            <button
              type="button"
              className="button icon-button launch-bar__play"
              aria-label={playLabel}
              title={playTitle}
              aria-busy={starting || undefined}
              disabled={!fullRequest || starting || running || !view}
              onClick={() => void play()}
            >
              {starting ? (
                <span className="launch-bar__spinner" aria-hidden="true" />
              ) : (
                <PlayIcon size={20} />
              )}
            </button>
            <label className="launch-bar__profile" title="Launch profile: the launch options">
              <span className="launch-bar__label">Launch profile</span>
              <select
                className="input"
                value={selectedProfile ?? ""}
                disabled={!view}
                onChange={(event) => {
                  setProfileId(event.target.value === "" ? null : event.target.value);
                }}
              >
                <option value="">Default ({defaultProfile})</option>
                {profiles
                  .filter((profile) => profile.id !== view?.config.defaultProfileId)
                  .map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.name}
                    </option>
                  ))}
              </select>
            </label>
            <ControlsSelect
              profiles={controllers?.profiles ?? []}
              resolution={resolution}
              software={software}
              value={temporaryProfileId}
              disabled={!controllers}
              unavailable={controls.view.state.status === "error" && !controllers}
              onChange={(next) => {
                setControlsProfileId(next);
                setControlsNotice(null);
              }}
            />
            <button
              type="button"
              className="button button--secondary button--small"
              aria-expanded={playersOpen}
              disabled={!controllers}
              title={
                !controllers
                  ? "Players"
                  : temporaryPlayers
                    ? `Players: ${describePlayers(controllers, temporaryPlayers)} — temporary for this launch`
                    : `Players: ${describePlayers(controllers, resolution?.players ?? [])}`
              }
              onClick={() => {
                setPlayersOpen(!playersOpen);
              }}
            >
              Players{temporaryPlayers ? " (temporary)" : ""}
            </button>
          </>
        }
      />

      <LaunchStatus messages={messages} session={session} onStop={stop} />

      {/* Overrides and the command line: collapsed by default, below the launch status. */}
      <section className="launch-section" aria-label="Advanced launch options">
        <Disclosure
          expanded={advanced}
          controls={advancedId}
          onToggle={() => {
            setAdvanced(!advanced);
            setAdvancedOpened(true);
          }}
        >
          Advanced
          {overrideCount > 0 && (
            <span
              className="counter"
              title={`${String(overrideCount)} options for ${overrideLabel}`}
            >
              {overrideCount}
            </span>
          )}
        </Disclosure>
        <div id={advancedId} className="launch-section__body" hidden={!advanced}>
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
                  aria-expanded={controlsOpen}
                  onClick={() => {
                    setControlsOpen(!controlsOpen);
                  }}
                >
                  Controls for {overrideLabel}
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

              {controlsOpen &&
                (controllers ? (
                  <ControlsPreferences
                    view={controllers}
                    resolution={controls.resolution.state}
                    scopes={scopes}
                    software={software}
                    temporary={temporaryProfileId}
                    temporaryPlayers={temporaryPlayers}
                    onChanged={controlsChanged}
                    onConfigure={() => {
                      setConfiguring(true);
                    }}
                    configureUnavailable={configureUnavailable}
                  />
                ) : controls.view.state.status === "error" ? (
                  <p className="message message--error" role="alert">
                    The controller profiles could not be loaded:{" "}
                    {describeError(controls.view.state.error)}
                  </p>
                ) : (
                  <p className="message message--muted" role="status">
                    Loading the controller profiles…
                  </p>
                ))}

              {showCommandPreview &&
                (fullRequest ? (
                  <CommandPreview
                    request={fullRequest}
                    // Both only grow, so their sum changes with either.
                    configVersion={launcher.configVersion + controlsVersion}
                    software={software}
                  />
                ) : (
                  <p className="message message--muted">
                    Choose what to launch to see the command line.
                  </p>
                ))}
            </>
          )}
        </div>
      </section>

      {configuring && configureUnavailable === null && (
        <ConfigureControlsDialog
          subject={subject}
          profileName={temporaryProfile?.name ?? resolution?.profileName ?? ""}
          profileSource={
            temporaryProfile
              ? describeSource("request", software)
              : resolution
                ? describeSource(resolution.profileSource, software)
                : ""
          }
          managed={
            temporaryProfile
              ? temporaryProfile.managed
              : resolution !== null && resolution.cfgDirectory !== null
          }
          cfgDirectory={temporaryProfile ? null : (resolution?.cfgDirectory ?? null)}
          players={
            controllers
              ? describePlayers(controllers, temporaryPlayers ?? resolution?.players ?? [])
              : null
          }
          onStart={async () => {
            if (!fullRequest) return;
            setError(null);
            await start(fullRequest);
          }}
          onClose={() => {
            setConfiguring(false);
          }}
        />
      )}
    </>
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
