import type { CSSProperties } from "react";
import type { MachineSummary, SoftwareSummary } from "../../search/searchApi";
import { DriverStatusBadge, SupportBadge } from "../badges";
import type { ResultRendererProps } from "./types";

/** Displays results as a grid of cards whose minimum width follows `itemSize`. */
export function CardRenderer({ items, itemSize, onOpen }: ResultRendererProps) {
  const style = { "--card-size": `${String(itemSize)}px` } as CSSProperties;
  return (
    <ul className="card-grid" style={style} aria-label="Results">
      {items.map((item, index) => (
        <li key={item.key} className="card-grid__item">
          <button
            type="button"
            className="card"
            onClick={() => {
              onOpen(index);
            }}
          >
            {item.kind === "machine" ? (
              <MachineCard machine={item.machine} />
            ) : (
              <SoftwareCard software={item.software} />
            )}
          </button>
        </li>
      ))}
    </ul>
  );
}

function MachineCard({ machine }: { machine: MachineSummary }) {
  const meta = [machine.manufacturer, machine.year].filter(Boolean).join(" · ");
  return (
    <>
      <span className="card__title">{machine.description}</span>
      <span className="card__name code">{machine.name}</span>
      {meta && <span className="card__meta">{meta}</span>}
      <span className="card__badges">
        <DriverStatusBadge status={machine.driverStatus} />
        {machine.cloneOf && <span className="badge">Clone of {machine.cloneOf}</span>}
        {machine.isBios && <span className="badge">BIOS</span>}
        {machine.isDevice && <span className="badge">Device</span>}
        {machine.isMechanical && <span className="badge">Mechanical</span>}
        {!machine.isRunnable && <span className="badge">Not runnable</span>}
      </span>
    </>
  );
}

function SoftwareCard({ software }: { software: SoftwareSummary }) {
  const meta = [software.publisher, software.year].filter(Boolean).join(" · ");
  return (
    <>
      <span className="card__title">{software.description}</span>
      <span className="card__name code">
        {software.listName}/{software.name}
      </span>
      {meta && <span className="card__meta">{meta}</span>}
      <span className="card__meta">{software.listDescription ?? software.listName}</span>
      <span className="card__badges">
        <SupportBadge supported={software.supported} />
        {software.cloneOf && <span className="badge">Clone of {software.cloneOf}</span>}
      </span>
    </>
  );
}
