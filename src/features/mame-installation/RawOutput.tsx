import type { ValidationReport } from "./mameApi";

/** The raw output of `mame -version`, collapsed. */
export function RawOutput({ report }: { report: ValidationReport }) {
  if (!report.output) {
    return null;
  }
  const { exitCode, stdout, stderr } = report.output;
  return (
    <details className="raw-output">
      <summary>Version command output</summary>
      <pre className="code">
        {`Exit code: ${exitCode === null ? "none" : String(exitCode)}\n`}
        {`stdout:\n${stdout || "(empty)"}\n`}
        {`stderr:\n${stderr || "(empty)"}`}
      </pre>
    </details>
  );
}
