import { useMemo, type ReactNode } from "react";
import { LaunchBar, LaunchControls } from "./LaunchControls";

interface MachineLaunchPanelProps {
  name: string;
  isRunnable: boolean;
  /** Short information about the machine, at the start of the launch bar. */
  summary?: ReactNode;
}

/** Launches a machine on its own, with its default configuration of media. */
export function MachineLaunchPanel({ name, isRunnable, summary }: MachineLaunchPanelProps) {
  const target = useMemo(() => ({ machine: name, software: null }), [name]);
  const request = useMemo(() => ({ machine: name }), [name]);
  if (!isRunnable) {
    return (
      <LaunchBar
        summary={summary}
        actions={
          <p className="message message--muted">
            This is a device or component: MAME cannot run it on its own.
          </p>
        }
      />
    );
  }
  return <LaunchControls target={target} request={request} summary={summary} />;
}
