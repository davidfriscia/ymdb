import { useCallback } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { BINDING_LABELS, describeSource } from "../controllers/controlsMessages";
import {
  previewLaunch,
  type LaunchRequest,
  type OptionSource,
  type PlanArgument,
} from "./launcherApi";

const SOURCE_LABELS: Record<OptionSource, string> = {
  global: "Global",
  profile: "Profile",
  machine: "Machine",
  software: "Software",
  romManager: "ROM Manager",
  controllers: "Controls",
};

interface CommandPreviewProps {
  /** Must be memoized: a new object loads a new preview. */
  request: LaunchRequest;
  /** Incremented when the launch or controllers configuration is saved. */
  configVersion: number;
  /** Whether a software is launched, for the origin of the controls. */
  software?: boolean;
}

/** The exact command line a launch runs, built by the backend. */
export function CommandPreview({ request, configVersion, software = false }: CommandPreviewProps) {
  const load = useCallback(() => previewLaunch(request), [request]);
  // Saved options change the command line: reload the preview.
  const { state } = useLoader(load, configVersion);

  if (state.status === "loading") {
    return (
      <p className="message message--muted" role="status">
        Building the command line…
      </p>
    );
  }
  if (state.status === "error") {
    return (
      <p className="message message--error" role="alert">
        {describeError(state.error)}
      </p>
    );
  }
  const { plan, commandLine, warnings } = state.value;
  return (
    <div className="command-preview" aria-label="Command line">
      <dl className="details-list">
        <div className="details-list__row">
          <dt>Executable</dt>
          <dd className="code">{plan.executable}</dd>
        </div>
        <div className="details-list__row">
          <dt>Working directory</dt>
          <dd className="code">{plan.workingDirectory}</dd>
        </div>
        <div className="details-list__row">
          <dt>Controller profile</dt>
          <dd>
            {plan.controls.profileName} — {describeSource(plan.controls.profileSource, software)}
          </dd>
        </div>
        {plan.controls.cfgDirectory !== null && (
          <div className="details-list__row">
            <dt>MAME configuration</dt>
            <dd className="code">{plan.controls.cfgDirectory}</dd>
          </div>
        )}
        {plan.controls.baseFile !== null && (
          <div className="details-list__row">
            <dt>Controller file</dt>
            <dd className="code">{plan.controls.baseFile}</dd>
          </div>
        )}
        {plan.controls.mappings.length > 0 && plan.controls.playersSource !== null && (
          <div className="details-list__row">
            <dt>Players</dt>
            <dd>
              <ul className="command-preview__players" aria-label="Player mappings">
                {plan.controls.mappings.map((mapping) => (
                  <li key={mapping.player}>
                    Player {mapping.player}: <strong>{mapping.controllerName}</strong> —{" "}
                    {BINDING_LABELS[mapping.binding]},{" "}
                    <span className="code">{mapping.provider}</span>{" "}
                    <span className="code wrap">{mapping.deviceId}</span>
                  </li>
                ))}
              </ul>
              <span className="message--muted">
                {describeSource(plan.controls.playersSource, software)}
              </span>
            </dd>
          </div>
        )}
        {plan.controls.joystickProvider !== null && (
          <div className="details-list__row">
            <dt>Joystick provider</dt>
            <dd className="code">{plan.controls.joystickProvider}</dd>
          </div>
        )}
        {plan.controls.controllerFile !== null && (
          <div className="details-list__row">
            <dt>Generated controller file</dt>
            <dd>
              <span className="code wrap">{plan.controls.controllerFile.path}</span>
              <span className="message--muted">
                {" "}
                (written when MAME starts
                {plan.controls.controllerFile.baseline !== null
                  ? `, keeping the sections of ${plan.controls.controllerFile.baseline}`
                  : ""}
                )
              </span>
              <details>
                <summary>Content</summary>
                <pre className="code" aria-label="Generated controller file">
                  {plan.controls.controllerFile.content}
                </pre>
              </details>
            </dd>
          </div>
        )}
      </dl>
      <ol className="command-preview__arguments" aria-label="Arguments">
        {plan.arguments.map((argument, index) => (
          <li key={index} className="command-preview__argument">
            <span className="code">{argument.tokens.map(showEmpty).join(" ")}</span>
            <span className="badge">{argumentLabel(argument)}</span>
          </li>
        ))}
      </ol>
      <pre className="command-preview__line code" aria-label="Full command line">
        {[plan.executable, ...commandLine].map(quote).join(" ")}
      </pre>
      {warnings.map((warning) => (
        <p key={warning} className="message message--warning">
          {warning}
        </p>
      ))}
    </div>
  );
}

function argumentLabel(argument: PlanArgument): string {
  switch (argument.role) {
    case "machine":
      return "Machine";
    case "media":
      return `Part ${argument.part}`;
    case "option":
      return SOURCE_LABELS[argument.source];
  }
}

/** Quotes empty tokens and tokens with spaces for display only; MAME receives them unquoted. */
function quote(token: string): string {
  return token === "" || /[\s"]/.test(token) ? `"${token.replace(/"/g, '\\"')}"` : token;
}

/** Shows an empty argument (such as the value of `-ctrlr ""`) as `""`. */
function showEmpty(token: string): string {
  return token === "" ? '""' : token;
}
