import { useState } from "react";
import { checkHealth, type HealthReport } from "./healthApi";

type CheckState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "success"; report: HealthReport }
  | { kind: "error"; message: string };

export function HealthCheck() {
  const [state, setState] = useState<CheckState>({ kind: "idle" });

  async function runCheck() {
    setState({ kind: "loading" });
    try {
      const report = await checkHealth();
      setState({ kind: "success", report });
    } catch (error) {
      setState({ kind: "error", message: String(error) });
    }
  }

  return (
    <section className="panel" aria-labelledby="health-check-title">
      <h3 id="health-check-title" className="panel__title">
        Backend connection
      </h3>
      <button
        type="button"
        className="button"
        onClick={() => void runCheck()}
        disabled={state.kind === "loading"}
      >
        {state.kind === "loading" ? "Checking…" : "Check backend connection"}
      </button>

      {state.kind === "success" && (
        <dl className="details-list" aria-label="Backend status">
          <dt>Status</dt>
          <dd>{state.report.status}</dd>
          <dt>Operating system</dt>
          <dd>{state.report.os}</dd>
          <dt>Architecture</dt>
          <dd>{state.report.arch}</dd>
          <dt>Version</dt>
          <dd>{state.report.version}</dd>
        </dl>
      )}

      {state.kind === "error" && (
        <p className="message message--error" role="alert">
          Backend check failed: {state.message}
        </p>
      )}
    </section>
  );
}
