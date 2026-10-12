import { useState } from "react";
import { describeError } from "../../shared/ipc";
import { pickMediaDirectory, type MediaConfig, type SourceStatus } from "./mediaApi";
import { useMedia } from "./mediaContext";

const STATUS_LABELS: Record<SourceStatus, { label: string; tone: string }> = {
  available: { label: "Available", tone: "badge badge--success" },
  missing: { label: "Not found", tone: "badge badge--error" },
  notADirectory: { label: "Not a directory", tone: "badge badge--error" },
  unreadable: { label: "Not readable", tone: "badge badge--warning" },
};

/** Settings section: image directories per category, in order of precedence. */
export function MediaSourcesSettings() {
  const { view, error, reload, save } = useMedia();
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  async function apply(change: (config: MediaConfig) => MediaConfig | null) {
    if (!view) return;
    setBusy(true);
    setActionError(null);
    try {
      const next = change(view.config);
      if (next) await save(next);
    } catch (saveError) {
      setActionError(describeError(saveError));
    } finally {
      setBusy(false);
    }
  }

  function withSources(config: MediaConfig, category: string, paths: string[]): MediaConfig {
    return { ...config, sources: { ...config.sources, [category]: paths } };
  }

  async function add(category: string) {
    if (!view) return;
    setActionError(null);
    let path: string | null;
    try {
      path = await pickMediaDirectory();
    } catch (pickError) {
      setActionError(describeError(pickError));
      return;
    }
    if (!path) return;
    const selected = path;
    await apply((config) => {
      const current = config.sources[category] ?? [];
      if (current.includes(selected)) {
        setActionError("This directory is already listed for this category.");
        return null;
      }
      return withSources(config, category, [...current, selected]);
    });
  }

  function move(category: string, index: number, offset: number) {
    void apply((config) => {
      const paths = [...(config.sources[category] ?? [])];
      const [moved] = paths.splice(index, 1);
      if (moved === undefined) return null;
      paths.splice(index + offset, 0, moved);
      return withSources(config, category, paths);
    });
  }

  function remove(category: string, index: number) {
    void apply((config) =>
      withSources(
        config,
        category,
        (config.sources[category] ?? []).filter((_, position) => position !== index),
      ),
    );
  }

  return (
    <section className="panel" aria-label="Media Sources">
      <p className="message message--muted">
        Directories with artwork and screenshots, for example from an artwork collection. Images are
        named after the MAME short name (snap/pacman.png) or, for software, the list and the
        software (snap/nes/smb.png). YMDB only reads these directories: it never copies or modifies
        the images. Directories are searched from top to bottom.
      </p>
      {error !== null && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The media sources could not be loaded: {describeError(error)}
          </p>
          <button type="button" className="button button--secondary" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {view?.loadError && (
        <p className="message message--warning" role="alert">
          {view.loadError}. No directory is used until the sources are saved again.
        </p>
      )}
      {actionError && (
        <p className="message message--error" role="alert">
          {actionError}
        </p>
      )}
      {view === null && error === null && <p className="message message--muted">Loading…</p>}
      {view?.categories.map((category) => {
        const sources = view.sources.find((entry) => entry.category === category.id)?.sources ?? [];
        return (
          <div
            key={category.id}
            className="media-sources__category"
            role="group"
            aria-label={category.label}
          >
            <h3 className="panel__subtitle">
              {category.label} <span className="code message--muted">{category.id}</span>
            </h3>
            {sources.length === 0 ? (
              <p className="message message--muted">No directory.</p>
            ) : (
              <ol className="media-sources__list">
                {sources.map((source, index) => (
                  <li key={source.path} className="media-sources__item">
                    <span className="code media-sources__path">{source.path}</span>
                    <span className={STATUS_LABELS[source.status].tone}>
                      {STATUS_LABELS[source.status].label}
                    </span>
                    <span className="media-sources__actions">
                      <button
                        type="button"
                        className="button button--secondary button--small"
                        aria-label={`Move up ${source.path}`}
                        disabled={busy || index === 0}
                        onClick={() => {
                          move(category.id, index, -1);
                        }}
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        className="button button--secondary button--small"
                        aria-label={`Move down ${source.path}`}
                        disabled={busy || index === sources.length - 1}
                        onClick={() => {
                          move(category.id, index, 1);
                        }}
                      >
                        ↓
                      </button>
                      <button
                        type="button"
                        className="button button--secondary button--small"
                        aria-label={`Remove ${source.path}`}
                        disabled={busy}
                        onClick={() => {
                          remove(category.id, index);
                        }}
                      >
                        Remove
                      </button>
                    </span>
                  </li>
                ))}
              </ol>
            )}
            <button
              type="button"
              className="button button--secondary button--small"
              disabled={busy}
              onClick={() => void add(category.id)}
            >
              Add directory…
            </button>
          </div>
        );
      })}
    </section>
  );
}
