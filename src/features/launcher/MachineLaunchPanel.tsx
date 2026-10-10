import { useMemo } from "react";
import { LaunchControls } from "./LaunchControls";

interface MachineLaunchPanelProps {
  name: string;
  isRunnable: boolean;
}

/** Launches a machine on its own, with its default configuration of media. */
export function MachineLaunchPanel({ name, isRunnable }: MachineLaunchPanelProps) {
  const target = useMemo(() => ({ machine: name, software: null }), [name]);
  const request = useMemo(() => ({ machine: name }), [name]);
  if (!isRunnable) {
    return (
      <p className="message message--muted launch-panel">
        This is a device or component: MAME cannot run it on its own.
      </p>
    );
  }
  return <LaunchControls target={target} request={request} />;
}
