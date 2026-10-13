import { useCallback, useMemo, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import {
  getSoftwareLaunchInfo,
  type MachineCandidate,
  type MediaAssignment,
  type SoftwareLaunchInfo,
} from "./launcherApi";
import { LaunchBar, LaunchControls } from "./LaunchControls";
import { useInstallationScope } from "../installations/installationsContext";

interface SoftwareLaunchPanelProps {
  list: string;
  name: string;
  refreshKey: number;
  /** Short information about the software, at the start of the launch bar. */
  summary?: ReactNode;
}

/** Launches a software on a machine chosen by the user. */
export function SoftwareLaunchPanel({ list, name, refreshKey, summary }: SoftwareLaunchPanelProps) {
  const installationId = useInstallationScope();
  const load = useCallback(
    () => getSoftwareLaunchInfo(installationId, list, name),
    [installationId, list, name],
  );
  const { state, reload } = useLoader(load, refreshKey);

  if (state.status === "loading") {
    return (
      <LaunchBar
        summary={summary}
        actions={
          <p className="message message--muted" role="status">
            Looking for machines that can run this software…
          </p>
        }
      />
    );
  }
  if (state.status === "error") {
    return (
      <LaunchBar
        summary={summary}
        actions={
          <div className="launch-bar__error" role="alert">
            <p className="message message--error">{describeError(state.error)}</p>
            <button type="button" className="button button--secondary" onClick={reload}>
              Retry
            </button>
          </div>
        }
      />
    );
  }
  if (state.value === null) return null;
  return <SoftwareLauncher key={`${list}:${name}`} info={state.value} summary={summary} />;
}

function SoftwareLauncher({ info, summary }: { info: SoftwareLaunchInfo; summary?: ReactNode }) {
  const [machineName, setMachineName] = useState<string | null>(info.recommended);
  // Part assignments per machine, kept when the user switches machines.
  const [assignments, setAssignments] = useState<Record<string, MediaAssignment[]>>({});
  const candidate = info.candidates.find((entry) => entry.name === machineName) ?? null;
  const media = useMemo(
    () => (candidate ? (assignments[candidate.name] ?? candidate.defaultMedia) : []),
    [candidate, assignments],
  );

  const target = useMemo(
    () => ({
      machine: candidate?.name ?? "",
      software: { list: info.listName, name: info.name },
    }),
    [candidate, info.listName, info.name],
  );
  const request = useMemo(
    () =>
      candidate && media.length > 0
        ? {
            machine: candidate.name,
            software: { list: info.listName, name: info.name, media },
          }
        : null,
    [candidate, media, info.listName, info.name],
  );

  if (info.candidates.length === 0) {
    return (
      <LaunchBar
        summary={summary}
        actions={
          <p className="message message--muted">
            No machine in the catalog declares the software list {info.listName}: this software
            cannot be launched.
          </p>
        }
      />
    );
  }

  function assign(device: string, part: string) {
    if (!candidate) return;
    // A part is mounted once: choosing it for a device frees it elsewhere.
    const others = media.filter((entry) => entry.device !== device && entry.part !== part);
    const next = part === "" ? others : [...others, { device, part }];
    const order = candidate.devices.map((entry) => entry.briefName);
    next.sort((a, b) => order.indexOf(a.device) - order.indexOf(b.device));
    setAssignments({ ...assignments, [candidate.name]: next });
  }

  return (
    <LaunchControls target={target} request={request} summary={summary}>
      <div className="launch-setup">
        <label className="launch-panel__field launch-panel__machine">
          <span>Machine</span>
          <select
            className="input"
            value={machineName ?? ""}
            onChange={(event) => {
              setMachineName(event.target.value === "" ? null : event.target.value);
            }}
          >
            {!info.recommended && <option value="">Choose a machine…</option>}
            {info.candidates.map((entry) => (
              <option key={entry.name} value={entry.name} disabled={!entry.launchable}>
                {candidateLabel(entry)}
              </option>
            ))}
          </select>
        </label>
        {candidate && (
          <fieldset className="launch-panel__media">
            <legend>Media</legend>
            {candidate.devices.map((device) => {
              const current = media.find((entry) => entry.device === device.briefName)?.part ?? "";
              const parts = info.parts.filter((part) => device.interfaces.includes(part.interface));
              return (
                <label key={device.briefName} className="launch-panel__field">
                  <span>
                    {device.instanceName ?? device.briefName}{" "}
                    <span className="code message--muted">-{device.briefName}</span>
                  </span>
                  <select
                    className="input"
                    value={current}
                    onChange={(event) => {
                      assign(device.briefName, event.target.value);
                    }}
                  >
                    <option value="">(empty)</option>
                    {parts.map((part) => (
                      <option key={part.name} value={part.name}>
                        {part.name}
                        {part.label ? ` – ${part.label}` : ""}
                      </option>
                    ))}
                  </select>
                </label>
              );
            })}
          </fieldset>
        )}
      </div>
      {candidate && media.length === 0 && (
        <p className="message message--warning">Mount at least one part to launch.</p>
      )}
      <div className="launch-setup__notes">
        {candidate && <CandidateDetails candidate={candidate} />}
        <p className="message message--muted">
          {info.recommended
            ? "Preselected: the only machine with this list as its own whose filter matches the software."
            : "Choose a machine: the catalog does not identify a single suitable one."}{" "}
          Declared lists and filters come from MAME; they cannot guarantee that the software works
          on the chosen machine (slot cards, RAM or other requirements are not checked).
        </p>
      </div>
    </LaunchControls>
  );
}

function candidateLabel(candidate: MachineCandidate): string {
  const notes = [candidate.relation === "original" ? "original list" : "compatible list"];
  if (!candidate.filterMatch) notes.push("filter mismatch");
  if (!candidate.launchable) notes.push("no media device for this software");
  return `${candidate.description} (${candidate.name}) — ${notes.join(", ")}`;
}

function CandidateDetails({ candidate }: { candidate: MachineCandidate }) {
  return (
    <p className="launch-panel__badges">
      <span className={candidate.relation === "original" ? "badge badge--success" : "badge"}>
        {candidate.relation === "original" ? "Original list" : "Compatible list"}
      </span>
      {candidate.filters.length > 0 && (
        <span className={candidate.filterMatch ? "badge badge--success" : "badge badge--warning"}>
          Filter {candidate.filters.join(", ")}:{" "}
          {candidate.filterMatch ? "matches" : "does not match"}
        </span>
      )}
      {candidate.cloneOf && <span className="badge">Clone of {candidate.cloneOf}</span>}
    </p>
  );
}
