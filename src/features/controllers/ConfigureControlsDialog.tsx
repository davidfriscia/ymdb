import { useState } from "react";
import { describeError } from "../../shared/ipc";
import { Modal } from "../../shared/Modal";

interface ConfigureControlsDialogProps {
  /** What MAME starts: the machine, or the software and its machine. */
  subject: string;
  /** The controller profile the launch uses, with where it comes from. */
  profileName: string;
  profileSource: string;
  /** `false` for the `MAME configuration` profile, which has no directory of YMDB. */
  managed: boolean;
  /** The profile's MAME configuration directory, when known. */
  cfgDirectory: string | null;
  /** The players of the launch in words, when known. */
  players: string | null;
  /** Starts the same launch as Play; rejects with the backend error. */
  onStart: () => Promise<void>;
  onClose: () => void;
}

/**
 * Explains how to assign inputs in MAME's own menus before starting the
 * normal launch with the effective controller profile. MAME has no option
 * to open its input menu directly, and YMDB has no button editor.
 */
export function ConfigureControlsDialog({
  subject,
  profileName,
  profileSource,
  managed,
  cfgDirectory,
  players,
  onStart,
  onClose,
}: ConfigureControlsDialogProps) {
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function start() {
    setStarting(true);
    setError(null);
    try {
      await onStart();
      onClose();
    } catch (failure) {
      setError(describeError(failure));
      setStarting(false);
    }
  }

  return (
    <Modal title="Configure Controls in MAME" onClose={onClose}>
      <p className="message">
        Controller profile: <strong>{profileName}</strong> ({profileSource}).
      </p>
      {players !== null && (
        <p className="message">
          Players: <strong>{players}</strong>. MAME starts with these controllers mapped to their
          players, so the assignments you make apply to the devices you will play with.
        </p>
      )}
      <ol className="controls-steps">
        <li>
          YMDB starts {subject} with this controller profile, exactly as Play does (software, media
          and launch options included).
        </li>
        <li>In MAME, open the menu to configure the buttons.</li>
        <li>
          The menu key is normally <kbd>Tab</kbd>, but it may have been remapped (for example by a
          controller configuration file).
        </li>
        <li>For the current machine, choose Input Settings → Input Assignments (this system).</li>
        <li>For assignments that apply to every machine, choose Input Assignments (general).</li>
        <li>MAME saves the changes when it exits normally (for example from its menu).</li>
        <li>
          Stopping MAME from YMDB terminates it at once: the changes of the session are not saved.
        </li>
      </ol>
      {managed ? (
        <p className="message message--muted">
          MAME saves the changes in the profile&apos;s own configuration directory
          {cfgDirectory ? (
            <>
              {" "}
              (<span className="code wrap">{cfgDirectory}</span>)
            </>
          ) : null}
          , used only by this profile and this MAME installation.
        </p>
      ) : (
        <p className="message message--warning">
          {profileName} uses MAME&apos;s own configuration directories, not an isolated context of
          YMDB: the changes apply to every launch that uses MAME&apos;s configuration.
        </p>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      <div className="actions">
        <button type="button" className="button" disabled={starting} onClick={() => void start()}>
          {starting ? "Starting…" : "Start MAME"}
        </button>
        <button type="button" className="button button--secondary" onClick={onClose}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}
