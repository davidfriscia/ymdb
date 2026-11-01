import { memo, useMemo, type CSSProperties } from "react";
import { themeProperties, type ThemeTokens } from "../../app/theme/themeTokens";

/**
 * A sample of the interface drawn with a theme being edited. The generated
 * properties are set on this element only, so the rest of the application
 * keeps its theme until the changes are saved; the sample uses the real
 * classes of the application wherever they exist.
 */
export const ThemeEditorPreview = memo(function ThemeEditorPreview({
  tokens,
}: {
  tokens: ThemeTokens;
}) {
  const style = useMemo(() => themeProperties(tokens) as CSSProperties, [tokens]);
  return (
    <div
      className="theme-sample"
      style={style}
      data-testid="theme-editor-preview"
      // A visual sample, not interactive: its controls only show their states.
      aria-hidden="true"
      inert
    >
      <nav className="theme-sample__nav" aria-label="Sample navigation">
        <p className="shell__brand">YMDB</p>
        <span className="shell__nav-item" aria-current="page">
          Catalog
        </span>
        <span className="shell__nav-item">Lists</span>
        <span className="shell__nav-item">Settings</span>
      </nav>
      <div className="theme-sample__content">
        <h3 className="panel__title theme-sample__title">Typography</h3>
        <p className="theme-sample__text">
          Primary text of titles and content.{" "}
          <span className="theme-sample__secondary">Secondary labels.</span>{" "}
          <span className="theme-sample__muted">Muted hints.</span>{" "}
          <span className="link-button">A link</span>
        </p>

        <div className="detail__tabs theme-sample__tabs" role="tablist" aria-label="Sample tabs">
          <span className="detail__tab" role="tab" aria-selected="true">
            Overview
          </span>
          <span className="detail__tab" role="tab" aria-selected="false">
            Media
          </span>
          <span className="detail__tab" role="tab" aria-selected="false">
            Software
          </span>
        </div>

        <div className="theme-sample__row">
          <button type="button" className="button">
            Primary
          </button>
          <button type="button" className="button button--secondary">
            Secondary
          </button>
          <button type="button" className="button button--secondary" aria-pressed="true">
            Pressed
          </button>
          <button type="button" className="button" disabled>
            Disabled
          </button>
        </div>

        <div className="theme-sample__row">
          <input
            className="input"
            type="text"
            defaultValue="Search text"
            aria-label="Sample input"
          />
          <span className="input theme-sample__focused">Focused field</span>
          <select className="input" aria-label="Sample select" defaultValue="name">
            <option value="name">Sort by name</option>
          </select>
        </div>

        <div className="theme-sample__cards">
          <div className="theme-sample__card">
            <span className="theme-sample__card-title">Pac-Man</span>
            <span className="theme-sample__muted">Namco, 1980</span>
          </div>
          <div className="theme-sample__card theme-sample__card--selected">
            <span className="theme-sample__card-title">Galaga</span>
            <span className="theme-sample__muted">Selected card</span>
          </div>
          <div className="theme-sample__card theme-sample__card--hover">
            <span className="theme-sample__card-title">Dig Dug</span>
            <span className="theme-sample__muted">Hover</span>
          </div>
        </div>

        <div className="theme-sample__row">
          <span className="badge badge--success">Available</span>
          <span className="badge badge--warning">Incomplete</span>
          <span className="badge badge--error">Missing</span>
          <span className="badge">Clone</span>
        </div>

        <p className="message message--success">The catalog was imported.</p>
        <p className="message message--warning">Some files are missing.</p>
        <p className="message message--error">MAME could not be started.</p>

        <div className="theme-sample__backdrop">
          <div className="modal__dialog theme-sample__dialog">
            <div className="modal__header">
              <h4 className="modal__title">Dialog</h4>
            </div>
            <div className="theme-sample__dialog-body">
              <span className="theme-sample__secondary">An elevated surface with its shadow.</span>
              <div className="theme-sample__row">
                <button type="button" className="button button--small">
                  Confirm
                </button>
                <button type="button" className="button button--secondary button--small">
                  Cancel
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
});
