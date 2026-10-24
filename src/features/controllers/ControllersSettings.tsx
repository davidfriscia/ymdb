import { useCallback } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { useLauncher } from "../launcher/launcherContext";
import { getControllers } from "./controllersApi";
import { AssignmentsBlock } from "./AssignmentsBlock";
import { ControllersBlock } from "./ControllersBlock";
import { ProfilesBlock } from "./ProfilesBlock";

/**
 * Settings page section: controller profiles, controllers and the default
 * assignment, shared by every MAME installation. Each change is saved by the
 * backend, which returns the new configuration; detail pages reload their
 * resolutions through the launcher's `controlsVersion`.
 */
export function ControllersSettings() {
  const { controlsVersion, controlsChanged } = useLauncher();
  const load = useCallback(() => getControllers(), []);
  const { state, latest, reload } = useLoader(load, controlsVersion);

  return (
    <section className="panel controllers-settings" aria-label="Controllers">
      <p className="message message--muted">
        MAME configures the inputs: YMDB chooses which controller profile a launch uses, and MAME
        saves the assignments you make in its menus in the profile&apos;s own configuration
        directory. A controller profile (inputs) is not a launch profile (launch options): both are
        chosen independently.
      </p>
      {state.status === "error" && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The controllers configuration could not be loaded: {describeError(state.error)}
          </p>
          <p className="message message--muted">
            YMDB never replaces controllers.json: fix or remove the file, then retry. Nothing can be
            changed until it can be read.
          </p>
          <button type="button" className="button button--secondary" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {latest === null && state.status === "loading" && (
        <p className="message message--muted">Loading…</p>
      )}
      {latest !== null && state.status !== "error" && (
        <>
          <ProfilesBlock view={latest} onChanged={controlsChanged} />
          <ControllersBlock view={latest} onChanged={controlsChanged} />
          <AssignmentsBlock view={latest} onChanged={controlsChanged} />
        </>
      )}
    </section>
  );
}
