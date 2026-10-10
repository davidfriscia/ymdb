import { useCallback } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
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
};

interface CommandPreviewProps {
  /** Must be memoized: a new object loads a new preview. */
  request: LaunchRequest;
  /** Incremented when the launch configuration is saved. */
  configVersion: number;
}

/** The exact command line a launch runs, built by the backend. */
export function CommandPreview({ request, configVersion }: CommandPreviewProps) {
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
      </dl>
      <ol className="command-preview__arguments" aria-label="Arguments">
        {plan.arguments.map((argument, index) => (
          <li key={index} className="command-preview__argument">
            <span className="code">{argument.tokens.join(" ")}</span>
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

/** Quotes tokens with spaces for display only; MAME receives them unquoted. */
function quote(token: string): string {
  return /[\s"]/.test(token) ? `"${token.replace(/"/g, '\\"')}"` : token;
}
