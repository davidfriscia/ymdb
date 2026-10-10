import { useState, type CSSProperties } from "react";
import { MEDIA_CATEGORIES, mediaUrl, type MediaTarget } from "../../media/mediaApi";
import { hasSources, useMedia } from "../../media/mediaContext";
import { DriverStatusBadge, SupportBadge } from "../badges";
import { booleanOption, numberOption, stringOption } from "./options";
import type { ResultItem, ResultRendererProps } from "./types";

/**
 * Image-first grid: the artwork fills a fixed 4:3 frame (contained or
 * cropped, never stretched) and the text stays secondary.
 */
export function MediaGridRenderer({ items, options, onOpen }: ResultRendererProps) {
  const { view, version } = useMedia();
  const category = stringOption(options, "category");
  const configured = hasSources(view, category);
  const style = {
    "--tile-size": `${String(numberOption(options, "tileSize"))}px`,
  } as CSSProperties;
  return (
    <>
      {view !== null && !configured && (
        <p className="message message--muted media-grid__hint" role="note">
          No directory is configured for {categoryLabel(category).toLowerCase()}. Add one in
          Settings, Media Sources.
        </p>
      )}
      <ul
        className="media-grid"
        style={style}
        data-fit={stringOption(options, "fit")}
        aria-label="Results"
      >
        {items.map((item, index) => (
          <li key={item.key} className="media-grid__item">
            <MediaTile
              item={item}
              // Without a source, no request is made: the placeholder is shown directly.
              src={configured ? mediaUrl(category, targetOf(item), version) : null}
              categoryLabel={categoryLabel(category)}
              showTitle={booleanOption(options, "showTitle")}
              showName={booleanOption(options, "showName")}
              showBadges={booleanOption(options, "showBadges")}
              onOpen={() => {
                onOpen(index);
              }}
            />
          </li>
        ))}
      </ul>
    </>
  );
}

function targetOf(item: ResultItem): MediaTarget {
  return item.kind === "machine"
    ? { kind: "machine", name: item.machine.name }
    : { kind: "software", list: item.software.listName, name: item.software.name };
}

function categoryLabel(category: string): string {
  return MEDIA_CATEGORIES.find((entry) => entry.id === category)?.label ?? category;
}

interface MediaTileProps {
  item: ResultItem;
  src: string | null;
  categoryLabel: string;
  showTitle: boolean;
  showName: boolean;
  showBadges: boolean;
  onOpen: () => void;
}

function MediaTile({
  item,
  src,
  categoryLabel,
  showTitle,
  showName,
  showBadges,
  onOpen,
}: MediaTileProps) {
  const title = item.kind === "machine" ? item.machine.description : item.software.description;
  const name =
    item.kind === "machine" ? item.machine.name : `${item.software.listName}/${item.software.name}`;
  const meta =
    item.kind === "machine"
      ? [item.machine.manufacturer, item.machine.year]
      : [item.software.publisher, item.software.year];
  return (
    <button
      type="button"
      className="media-tile"
      // The accessible name and the tooltip carry the full title, even when hidden or truncated.
      aria-label={`${title} (${name})`}
      title={`${title}\n${name}${meta.some(Boolean) ? `\n${meta.filter(Boolean).join(" · ")}` : ""}`}
      onClick={onOpen}
    >
      <span className="media-tile__frame">
        <MediaImage key={src ?? "none"} src={src} categoryLabel={categoryLabel} />
        {meta.some(Boolean) && (
          <span className="media-tile__overlay" aria-hidden="true">
            {meta.filter(Boolean).join(" · ")}
          </span>
        )}
      </span>
      {(showTitle || showName || showBadges) && (
        <span className="media-tile__caption" aria-hidden="true">
          {showTitle && <span className="media-tile__title">{title}</span>}
          {showName && <span className="media-tile__name code">{name}</span>}
          {showBadges && (
            <span className="media-tile__badges">
              {item.kind === "machine" ? (
                <DriverStatusBadge status={item.machine.driverStatus} />
              ) : (
                <SupportBadge supported={item.software.supported} />
              )}
            </span>
          )}
        </span>
      )}
    </button>
  );
}

type ImageState = "loading" | "loaded" | "missing";

/** The image, or a placeholder when there is none (never another title's image). */
function MediaImage({ src, categoryLabel }: { src: string | null; categoryLabel: string }) {
  const [state, setState] = useState<ImageState>(src ? "loading" : "missing");
  return (
    <>
      {src && state !== "missing" && (
        <img
          className="media-tile__image"
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          data-state={state}
          onLoad={() => {
            setState("loaded");
          }}
          onError={() => {
            setState("missing");
          }}
        />
      )}
      {state === "missing" && (
        <span className="media-tile__placeholder" data-testid="media-placeholder">
          <svg viewBox="0 0 24 24" width="28" height="28" aria-hidden="true">
            <path
              d="M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9.5a1.5 1.5 0 1 0 0-.01"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinejoin="round"
            />
          </svg>
          <span>No {categoryLabel.toLowerCase()}</span>
        </span>
      )}
    </>
  );
}
