import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { MEDIA_CATEGORIES, mediaUrl, type MediaCategoryId, type MediaTarget } from "./mediaApi";
import { hasSources, useMedia } from "./mediaContext";

type ImageStatus = "loading" | "loaded" | "missing";

interface GalleryImage {
  category: MediaCategoryId;
  label: string;
  src: string;
}

interface MediaGalleryProps {
  /** The record whose artwork is shown. */
  target: MediaTarget;
  /** Title of the record, used in the images' alternative text. */
  title: string;
  /**
   * Whether the gallery has (or may still find) an image to show. Lets the
   * page give the space of an empty gallery to the content next to it.
   */
  onAvailabilityChange?: (available: boolean) => void;
}

/**
 * The artwork of one machine or software, from the configured media sources.
 * Images are requested through the `ymdb-media` protocol by identifiers only
 * (at most one request per configured category); only the images that load
 * are shown, the first one in category order (snapshots, title screens,
 * flyers, cabinets, marquees) as main image.
 */
export function MediaGallery({ target, title, onAvailabilityChange }: MediaGalleryProps) {
  const { view, error, version } = useMedia();
  const targetKey =
    target.kind === "machine" ? `machine:${target.name}` : `software:${target.list}/${target.name}`;

  if (view === null) {
    return (
      <GalleryMessage
        text={error === null ? "Loading artwork…" : "The media sources could not be read."}
        onAvailabilityChange={onAvailabilityChange}
        available={error === null}
      />
    );
  }
  const images = MEDIA_CATEGORIES.filter((category) => hasSources(view, category.id)).map(
    (category): GalleryImage => ({
      category: category.id,
      label: category.label,
      src: mediaUrl(category.id, target, version),
    }),
  );
  if (images.length === 0) {
    return (
      <GalleryMessage
        text="No media directories are configured. Add them in Settings, Media Sources."
        onAvailabilityChange={onAvailabilityChange}
        available={false}
      />
    );
  }
  // A new record or new sources start from scratch: no image of the previous
  // record or configuration is ever shown.
  return (
    <Gallery
      key={`${targetKey}?${String(version)}`}
      images={images}
      title={title}
      onAvailabilityChange={onAvailabilityChange}
    />
  );
}

function GalleryMessage({
  text,
  available,
  onAvailabilityChange,
}: {
  text: string;
  available: boolean;
  onAvailabilityChange?: (available: boolean) => void;
}) {
  useLayoutEffect(() => {
    onAvailabilityChange?.(available);
  }, [available, onAvailabilityChange]);
  return (
    <section className="media-gallery media-gallery--empty" aria-label="Media">
      <p className="message message--muted" role="status">
        {text}
      </p>
    </section>
  );
}

interface GalleryProps {
  images: GalleryImage[];
  title: string;
  onAvailabilityChange?: (available: boolean) => void;
}

function Gallery({ images, title, onAvailabilityChange }: GalleryProps) {
  const [statuses, setStatuses] = useState<Partial<Record<MediaCategoryId, ImageStatus>>>({});
  const [chosen, setChosen] = useState<MediaCategoryId | null>(null);
  const [enlarged, setEnlarged] = useState(false);
  const stageRef = useRef<HTMLButtonElement>(null);

  const statusOf = (category: MediaCategoryId) => statuses[category] ?? "loading";
  const setStatus = (category: MediaCategoryId, status: ImageStatus) => {
    setStatuses((current) => ({ ...current, [category]: status }));
  };
  const available = images.filter((image) => statusOf(image.category) === "loaded");
  const settled = images.every((image) => statusOf(image.category) !== "loading");

  // The initial image follows the category order: it is chosen once every
  // preferred category is known, so a later answer never replaces it.
  let preferred: GalleryImage | null = null;
  for (const image of images) {
    const status = statusOf(image.category);
    if (status === "loading") break;
    if (status === "loaded") {
      preferred = image;
      break;
    }
  }
  const selected = available.find((image) => image.category === chosen) ?? preferred;
  const empty = settled && available.length === 0;

  useLayoutEffect(() => {
    onAvailabilityChange?.(!empty);
  }, [empty, onAvailabilityChange]);

  const showIndex = (index: number) => {
    const image = available[(index + available.length) % available.length];
    if (image) setChosen(image.category);
  };

  return (
    <section className="media-gallery" aria-label="Media">
      {selected ? (
        <figure className="media-gallery__main">
          <button
            ref={stageRef}
            type="button"
            className="media-gallery__stage"
            aria-label={`Enlarge ${selected.label.toLowerCase()}`}
            onClick={() => {
              setEnlarged(true);
            }}
          >
            <img
              className="media-gallery__image"
              src={selected.src}
              alt={`${title}: ${selected.label}`}
              decoding="async"
              onError={() => {
                setStatus(selected.category, "missing");
              }}
            />
          </button>
          <figcaption className="media-gallery__caption">{selected.label}</figcaption>
        </figure>
      ) : (
        !settled && (
          <div className="media-gallery__stage media-gallery__stage--loading" role="status">
            Loading artwork…
          </div>
        )
      )}
      {empty && (
        <p className="message message--muted" role="status">
          No artwork was found for this title in the media sources.
        </p>
      )}

      {/*
        Every configured category is requested once, here; an image that does
        not load (missing or unreadable) is hidden. With a single image there
        is nothing to choose, so the list is hidden too.
      */}
      <ul className="media-gallery__thumbs" aria-label="Images" hidden={available.length < 2}>
        {images.map((image) => {
          const status = statusOf(image.category);
          return (
            <li key={image.category} hidden={status !== "loaded"}>
              <button
                type="button"
                className="media-gallery__thumb"
                aria-pressed={selected?.category === image.category}
                aria-label={image.label}
                title={image.label}
                onClick={() => {
                  setChosen(image.category);
                }}
              >
                <span className="media-gallery__thumb-frame">
                  {status !== "missing" && (
                    <img
                      src={image.src}
                      alt=""
                      decoding="async"
                      data-category={image.category}
                      onLoad={() => {
                        setStatus(image.category, "loaded");
                      }}
                      onError={() => {
                        setStatus(image.category, "missing");
                      }}
                    />
                  )}
                </span>
                <span className="media-gallery__thumb-label" aria-hidden="true">
                  {image.label}
                </span>
              </button>
            </li>
          );
        })}
      </ul>

      {enlarged && selected && (
        <Lightbox
          images={available}
          index={available.indexOf(selected)}
          title={title}
          onIndex={showIndex}
          onClose={() => {
            setEnlarged(false);
            stageRef.current?.focus();
          }}
        />
      )}
    </section>
  );
}

interface LightboxProps {
  images: GalleryImage[];
  index: number;
  title: string;
  onIndex: (index: number) => void;
  onClose: () => void;
}

/**
 * Modal view of one image at the largest size that fits the window, with
 * the arrow keys moving between images and Escape closing it. Focus stays
 * inside while it is open and returns to the gallery when it closes.
 */
function Lightbox({ images, index, title, onIndex, onClose }: LightboxProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const image = images[index];
  const several = images.length > 1;

  useEffect(() => {
    closeRef.current?.focus();
  }, []);

  if (!image) return null;

  function onKeyDown(event: ReactKeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    } else if (event.key === "ArrowLeft" && several) {
      event.preventDefault();
      onIndex(index - 1);
    } else if (event.key === "ArrowRight" && several) {
      event.preventDefault();
      onIndex(index + 1);
    } else if (event.key === "Tab") {
      // Keeps the focus on the lightbox controls.
      const controls = [...(dialogRef.current?.querySelectorAll("button") ?? [])];
      const first = controls[0];
      const last = controls.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && (document.activeElement === first || !isInside(dialogRef))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !isInside(dialogRef))) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  return createPortal(
    <div
      ref={dialogRef}
      className="lightbox"
      role="dialog"
      aria-modal="true"
      aria-label={`${title}: ${image.label}`}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      onClick={(event) => {
        // A click on the backdrop, outside the image and the controls, closes.
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="lightbox__bar">
        <p className="lightbox__caption">
          <span className="lightbox__title">{title}</span>
          <span>
            {image.label}
            {several && (
              <span className="lightbox__position">
                {" "}
                · {index + 1} / {images.length}
              </span>
            )}
          </span>
        </p>
        <button
          ref={closeRef}
          type="button"
          className="button button--secondary button--small"
          onClick={onClose}
        >
          Close
        </button>
      </div>
      <div
        className="lightbox__stage"
        onClick={(event) => {
          if (event.target === event.currentTarget) onClose();
        }}
      >
        {several && (
          <button
            type="button"
            className="button button--secondary lightbox__nav"
            aria-label="Previous image"
            onClick={() => {
              onIndex(index - 1);
            }}
          >
            ‹
          </button>
        )}
        <img
          className="lightbox__image"
          src={image.src}
          alt={`${title}: ${image.label}`}
          decoding="async"
        />
        {several && (
          <button
            type="button"
            className="button button--secondary lightbox__nav"
            aria-label="Next image"
            onClick={() => {
              onIndex(index + 1);
            }}
          >
            ›
          </button>
        )}
      </div>
    </div>,
    document.body,
  );
}

function isInside(ref: { current: HTMLElement | null }): boolean {
  return ref.current?.contains(document.activeElement) ?? false;
}
