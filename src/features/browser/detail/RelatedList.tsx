import { useCallback, useState } from "react";
import { describeError } from "../../../shared/ipc";
import { useLoader } from "../../../shared/useLoader";
import { formatCount } from "../../catalog/catalogMessages";
import { searchMachines, searchSoftware } from "../../search/searchApi";
import type { DetailTarget } from "../browserState";

const RELATED_PAGE_SIZE = 20;

/** Records related to a detail, listed through the paginated search. */
export type RelatedKind = "machineClones" | "listMachines" | "softwareClones" | "listSoftware";

interface RelatedEntry {
  key: string;
  title: string;
  detail: string;
  target: DetailTarget;
}

interface RelatedPage {
  entries: RelatedEntry[];
  total: number;
  pageCount: number;
}

async function loadRelated(
  kind: RelatedKind,
  name: string,
  list: string,
  page: number,
): Promise<RelatedPage> {
  const common = { page, pageSize: RELATED_PAGE_SIZE, sort: "description" } as const;
  if (kind === "machineClones" || kind === "listMachines") {
    const response = await searchMachines({
      ...common,
      filters: kind === "machineClones" ? { cloneOf: name } : { softwareLists: [list] },
    });
    return {
      total: response.total,
      pageCount: response.pageCount,
      entries: response.items.map((machine) => ({
        key: machine.name,
        title: machine.description,
        detail: [machine.name, machine.manufacturer, machine.year].filter(Boolean).join(" · "),
        target: { kind: "machine", name: machine.name },
      })),
    };
  }
  const response = await searchSoftware({
    ...common,
    filters: kind === "softwareClones" ? { lists: [list], cloneOf: name } : { lists: [list] },
  });
  return {
    total: response.total,
    pageCount: response.pageCount,
    entries: response.items.map((software) => ({
      key: `${software.listName}/${software.name}`,
      title: software.description,
      detail: [software.name, software.publisher, software.year].filter(Boolean).join(" · "),
      target: { kind: "software", list: software.listName, name: software.name },
    })),
  };
}

interface RelatedListProps {
  kind: RelatedKind;
  /** Parent record name, for clone lists. */
  name?: string;
  /** Software list name. */
  list?: string;
  label: string;
  refreshKey: number;
  onOpen: (target: DetailTarget) => void;
}

export function RelatedList({
  kind,
  name = "",
  list = "",
  label,
  refreshKey,
  onOpen,
}: RelatedListProps) {
  const [page, setPage] = useState(0);
  const load = useCallback(() => loadRelated(kind, name, list, page), [kind, name, list, page]);
  const { state, reload } = useLoader(load, refreshKey);

  if (state.status === "loading") {
    return (
      <p className="message message--muted" role="status">
        Loading {label.toLowerCase()}…
      </p>
    );
  }
  if (state.status === "error") {
    return (
      <div className="state-message" role="alert">
        <p className="message message--error">{describeError(state.error)}</p>
        <button type="button" className="button button--secondary" onClick={reload}>
          Retry
        </button>
      </div>
    );
  }
  const { entries, total, pageCount } = state.value;
  if (total === 0) {
    return <p className="message message--muted">None.</p>;
  }
  return (
    <div className="related">
      <ul className="related__list" aria-label={label}>
        {entries.map((entry) => (
          <li key={entry.key}>
            <button
              type="button"
              className="link-button"
              onClick={() => {
                onOpen(entry.target);
              }}
            >
              {entry.title}
            </button>{" "}
            <span className="message--muted">{entry.detail}</span>
          </li>
        ))}
      </ul>
      {pageCount > 1 && (
        <div className="related__pages">
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={page === 0}
            onClick={() => {
              setPage(page - 1);
            }}
          >
            Previous
          </button>
          <span className="message--muted">
            Page {page + 1} of {formatCount(pageCount)} · {formatCount(total)} total
          </span>
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={page >= pageCount - 1}
            onClick={() => {
              setPage(page + 1);
            }}
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
