import { useCallback } from "react";
import { useLoader, type Loader } from "../../shared/useLoader";
import type { LaunchTarget } from "../launcher/launcherApi";
import { softwareKey } from "../launcher/launcherApi";
import { useLauncher } from "../launcher/launcherContext";
import {
  getControllers,
  machineSoftwareKey,
  resolveControls,
  type AssignmentTarget,
  type ControllersView,
  type ControlsResolution,
} from "./controllersApi";

export interface ControlsData {
  /** The controllers configuration (profiles, controllers, assignments). */
  view: Loader<ControllersView>;
  /**
   * The saved controls of the target and their inheritance, resolved by the
   * backend; `null` while no machine is chosen.
   */
  resolution: Loader<ControlsResolution | null>;
}

/**
 * Loads the controllers configuration and the resolution of a launch target
 * for an installation. Both reload after every saved change of the
 * controllers configuration (`controlsVersion`); stale responses are ignored.
 */
export function useControls(installationId: string, target: LaunchTarget): ControlsData {
  const { controlsVersion } = useLauncher();
  const loadView = useCallback(() => getControllers(), []);
  const view = useLoader(loadView, controlsVersion);
  const { machine } = target;
  const list = target.software?.list;
  const name = target.software?.name;
  const loadResolution = useCallback(
    () =>
      machine === ""
        ? Promise.resolve(null)
        : resolveControls(
            installationId,
            machine,
            list !== undefined && name !== undefined ? { list, name } : undefined,
          ),
    [installationId, machine, list, name],
  );
  const resolution = useLoader(loadResolution, controlsVersion);
  return { view, resolution };
}

/** A level at which a preference of the target can be saved. */
export interface PreferenceScope {
  target: AssignmentTarget;
  label: string;
}

/**
 * The levels a preference of a machine or software can be saved at. A
 * machine + software preference needs the machine of the launch.
 */
export function preferenceScopes(target: LaunchTarget): PreferenceScope[] {
  const { machine, software } = target;
  const scopes: PreferenceScope[] = [];
  if (software) {
    const key = softwareKey(software.list, software.name);
    scopes.push({
      target: { level: "software", key },
      label: `This software (${key}), on any machine`,
    });
    if (machine !== "") {
      scopes.push({
        target: {
          level: "machineSoftware",
          key: machineSoftwareKey(machine, software.list, software.name),
        },
        label: `This software on ${machine} only`,
      });
    }
  }
  if (machine !== "") {
    scopes.push({
      target: { level: "machine", key: machine },
      label: software
        ? `The machine ${machine}, for everything it runs`
        : `This machine (${machine})`,
    });
  }
  return scopes;
}
