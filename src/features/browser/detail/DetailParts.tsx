import type { ReactNode } from "react";
import { describeError } from "../../../shared/ipc";
import type { LoadState } from "../../../shared/useLoader";
import { MediaGallery } from "../../media/MediaGallery";
import type { MediaTarget } from "../../media/mediaApi";
import type { RecordLink } from "../detailApi";

export interface DetailTab {
  id: string;
  label: string;
  /** Number of entries, shown next to the label. */
  count?: number;
}

/** The navigation controls of the page, in the header above the media. */
export interface DetailNavigation {
  /** Back to the results (or to the previous detail). */
  start: ReactNode;
  /** Previous / position / Next in the results, when opened from them. */
  end: ReactNode;
}

interface DetailFrameProps {
  title: string;
  subtitle: ReactNode;
  badges?: ReactNode;
  navigation: DetailNavigation;
  /** The record whose artwork is shown in the gallery below the header. */
  media?: MediaTarget;
  /**
   * The launch bar (and what goes with it), given the short information about
   * the record to show at its start. Without it, the information is shown alone.
   */
  launch?: (summary: ReactNode) => ReactNode;
  tabs: DetailTab[];
  activeTab: string;
  onTab: (tab: string) => void;
  children: ReactNode;
}

/**
 * The layout shared by the detail pages, from top to bottom: the header
 * (navigation only), the media gallery, the information and launch bar with
 * the title, the status of the last launch, the Advanced launch options, and
 * the technical sections with their content.
 */
export function DetailFrame({
  title,
  subtitle,
  badges,
  navigation,
  media,
  launch,
  tabs,
  activeTab,
  onTab,
  children,
}: DetailFrameProps) {
  const active = tabs.some((tab) => tab.id === activeTab) ? activeTab : "overview";
  const summary = (
    <div className="detail-summary">
      <h2 id="detail-title" className="detail-summary__title" title={title}>
        {title}
      </h2>
      <p className="detail-summary__line">{subtitle}</p>
      {badges && <p className="detail-summary__badges">{badges}</p>}
    </div>
  );
  return (
    <article className="detail" aria-labelledby="detail-title">
      <DetailHeader navigation={navigation} />
      {media && <MediaGallery target={media} title={title} />}
      <div className="detail__launch">
        {launch ? launch(summary) : <div className="launch-bar">{summary}</div>}
      </div>
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

/**
 * Back on the left, the result navigation on the right. The title is shown in
 * the information bar below the media, never a second time here.
 */
function DetailHeader({ navigation }: { navigation: DetailNavigation }) {
  return (
    <header className="detail-header">
      <nav className="detail-header__start" aria-label="Detail navigation">
        {navigation.start}
      </nav>
      <nav className="detail-header__end" aria-label="Result navigation">
        {navigation.end}
      </nav>
    </header>
  );
}

/** Loading, error and "not found" states shared by the detail views. */
export function DetailStatus({
  state,
  what,
  navigation,
  onRetry,
}: {
  state: LoadState<unknown>;
  what: string;
  navigation: DetailNavigation;
  onRetry: () => void;
}) {
  return (
    <div className="detail">
      <DetailHeader navigation={navigation} />
      {state.status === "loading" ? (
        <p className="message message--muted" role="status">
          Loading {what}…
        </p>
      ) : state.status === "error" ? (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The {what} could not be loaded: {describeError(state.error)}
          </p>
          <button type="button" className="button button--secondary" onClick={onRetry}>
            Retry
          </button>
        </div>
      ) : (
        <p className="message message--warning" role="alert">
          This {what} is not in the catalog. The catalog may have been imported again.
        </p>
      )}
    </div>
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
