import type { ReactNode } from "react";
import { describeError } from "../../../shared/ipc";
import type { LoadState } from "../../../shared/useLoader";
import type { RecordLink } from "../detailApi";

export interface DetailTab {
  id: string;
  label: string;
  /** Number of entries, shown next to the label. */
  count?: number;
}

interface DetailFrameProps {
  title: string;
  subtitle: ReactNode;
  badges?: ReactNode;
  /** Shown below the header, for example the launch controls. */
  actions?: ReactNode;
  tabs: DetailTab[];
  activeTab: string;
  onTab: (tab: string) => void;
  children: ReactNode;
}

/** Title, tabs and content of a detail view. */
export function DetailFrame({
  title,
  subtitle,
  badges,
  actions,
  tabs,
  activeTab,
  onTab,
  children,
}: DetailFrameProps) {
  const active = tabs.some((tab) => tab.id === activeTab) ? activeTab : "overview";
  return (
    <article className="detail" aria-labelledby="detail-title">
      <header className="detail__header">
        <h2 id="detail-title" className="detail__title">
          {title}
        </h2>
        <p className="detail__subtitle">{subtitle}</p>
        {badges && <p className="detail__badges">{badges}</p>}
      </header>
      {actions}
      <div className="detail__body">
        <div className="detail__tabs" role="tablist" aria-label="Sections">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              id={`detail-tab-${tab.id}`}
              aria-selected={tab.id === active}
              aria-controls="detail-panel"
              className="detail__tab"
              onClick={() => {
                onTab(tab.id);
              }}
            >
              {tab.label}
              {tab.count !== undefined && <span className="detail__tab-count">{tab.count}</span>}
            </button>
          ))}
        </div>
        <div
          id="detail-panel"
          className="detail__panel"
          role="tabpanel"
          aria-labelledby={`detail-tab-${active}`}
        >
          {children}
        </div>
      </div>
    </article>
  );
}

/** Loading, error and "not found" states shared by the detail views. */
export function DetailStatus({
  state,
  what,
  onRetry,
}: {
  state: LoadState<unknown>;
  what: string;
  onRetry: () => void;
}) {
  if (state.status === "loading") {
    return (
      <p className="message message--muted" role="status">
        Loading {what}…
      </p>
    );
  }
  if (state.status === "error") {
    return (
      <div className="state-message" role="alert">
        <p className="message message--error">
          The {what} could not be loaded: {describeError(state.error)}
        </p>
        <button type="button" className="button button--secondary" onClick={onRetry}>
          Retry
        </button>
      </div>
    );
  }
  return (
    <p className="message message--warning" role="alert">
      This {what} is not in the catalog. The catalog may have been imported again.
    </p>
  );
}

export type Field = [label: string, value: ReactNode];

/** A definition list; fields without a value are omitted. */
export function Fields({ fields, label }: { fields: Field[]; label?: string }) {
  const shown = fields.filter(([, value]) => value !== null && value !== undefined && value !== "");
  if (shown.length === 0) return null;
  return (
    <dl className="details-list" aria-label={label}>
      {shown.map(([name, value]) => (
        <div key={name} className="details-list__row">
          <dt>{name}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="detail__section">
      <h3 className="detail__section-title">{title}</h3>
      {children}
    </section>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="message message--muted">{children}</p>;
}

/** A link to another record; unresolved references are shown as plain text. */
export function RecordLinkButton({
  link,
  onOpen,
}: {
  link: RecordLink;
  onOpen: (name: string) => void;
}) {
  if (link.description === null) {
    return (
      <span>
        <span className="code">{link.name}</span>{" "}
        <span className="message--muted">(not in the catalog)</span>
      </span>
    );
  }
  return (
    <button
      type="button"
      className="link-button"
      onClick={() => {
        onOpen(link.name);
      }}
    >
      {link.description} <span className="code">({link.name})</span>
    </button>
  );
}

export function DataTable({
  label,
  columns,
  rows,
}: {
  label: string;
  columns: string[];
  rows: { key: string; cells: ReactNode[] }[];
}) {
  return (
    <div className="data-table">
      <table aria-label={label}>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column} scope="col">
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key}>
              {row.cells.map((cell, index) => (
                <td key={columns[index] ?? index}>{cell ?? "—"}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
