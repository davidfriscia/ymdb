import { useEffect, useState } from "react";
import { describeError } from "../../shared/ipc";
import { formatCount } from "../catalog/catalogMessages";
import { searchMachines, searchSoftware } from "./searchApi";

/** Temporary panel to try the search engine until the catalog browser exists. */

type Scope = "machines" | "software";

const PREVIEW_SIZE = 20;

interface PreviewRow {
  key: string;
  title: string;
  detail: string;
}

interface PreviewResult {
  total: number;
  elapsedMs: number;
  rows: PreviewRow[];
}

async function runSearch(scope: Scope, text: string): Promise<PreviewResult> {
  if (scope === "machines") {
    const response = await searchMachines({ text, pageSize: PREVIEW_SIZE });
    return {
      total: response.total,
      elapsedMs: response.elapsedMs,
      rows: response.items.map((machine) => ({
        key: machine.name,
        title: machine.description,
        detail: [machine.year, machine.manufacturer].filter(Boolean).join(" · "),
      })),
    };
  }
  const response = await searchSoftware({ text, pageSize: PREVIEW_SIZE });
  return {
    total: response.total,
    elapsedMs: response.elapsedMs,
    rows: response.items.map((software) => ({
      key: `${software.listName}/${software.name}`,
      title: software.description,
      detail: [software.year, software.publisher].filter(Boolean).join(" · "),
    })),
  };
}

interface SearchPreviewProps {
  /** Delay after the last keystroke before searching. */
  debounceMs?: number;
}

export function SearchPreview({ debounceMs = 300 }: SearchPreviewProps) {
  const [text, setText] = useState("");
  const [scope, setScope] = useState<Scope>("machines");
  const [result, setResult] = useState<PreviewResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(() => {
      setSearching(true);
      runSearch(scope, text)
        .then(
          (found) => {
            if (!active) return;
            setResult(found);
            setError(null);
          },
          (searchError: unknown) => {
            if (!active) return;
            setResult(null);
            setError(describeError(searchError));
          },
        )
        .finally(() => {
          if (active) setSearching(false);
        });
    }, debounceMs);
    return () => {
      // Responses of superseded searches are ignored.
      active = false;
      window.clearTimeout(timer);
    };
  }, [text, scope, debounceMs]);

  return (
    <section className="panel" aria-labelledby="search-preview-title" aria-busy={searching}>
      <h2 id="search-preview-title" className="panel__title">
        Catalog search (preview)
      </h2>
      <p className="message message--muted">
        A temporary view to try the search engine. The catalog browser will replace it.
      </p>

      <div className="search-preview__controls">
        <input
          type="search"
          className="search-preview__input"
          aria-label="Search text"
          placeholder="Name, description, manufacturer, year…"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
        />
        <select
          className="search-preview__scope"
          aria-label="Search scope"
          value={scope}
          onChange={(event) => {
            setScope(event.target.value as Scope);
          }}
        >
          <option value="machines">Machines</option>
          <option value="software">Software</option>
        </select>
      </div>

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      {result && (
        <>
          <p className="message message--muted" role="status">
            {formatCount(result.total)} results in {result.elapsedMs.toFixed(1)} ms
            {result.total > result.rows.length &&
              ` (showing the first ${String(result.rows.length)})`}
          </p>
          <ol className="search-preview__results" aria-label="Search results">
            {result.rows.map((row) => (
              <li key={row.key} className="search-preview__result">
                <span className="code">{row.key}</span> {row.title}
                {row.detail && <span className="message--muted"> · {row.detail}</span>}
              </li>
            ))}
          </ol>
        </>
      )}
    </section>
  );
}
