import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { describeError } from "../../shared/ipc";
import { ChevronLeftIcon, ChevronRightIcon } from "../../shared/icons";
import { useLoader } from "../../shared/useLoader";
import {
  MEDIA_CATEGORIES,
  listMedia,
  mediaUrl,
  type CategoryImages,
  type MediaCategoryId,
  type MediaTarget,
} from "./mediaApi";
import { hasSources, useMedia } from "./mediaContext";

/** With more images than this, the position is shown as text instead of dots. */
const MAX_DOTS = 12;

interface GalleryImage {
  label: string;
  src: string;
}

interface MediaGalleryProps {
  /** The record whose artwork is shown. */
  target: MediaTarget;
  /** Title of the record, used in the images' alternative text. */
  title: string;
}

/**
 * The artwork of one machine or software, from the configured media sources:
 * a carousel of the images of one category, next to the categories that have
 * at least one image of the title. The backend lists the images of the title
 * (targeted lookups, no directory scan); they are requested through the
 * `ymdb-media` protocol by identifiers and index only.
 */
export function MediaGallery({ target, title }: MediaGalleryProps) {
  const { view, error, version } = useMedia();
  const targetKey =
    target.kind === "machine" ? `machine:${target.name}` : `software:${target.list}/${target.name}`;

  if (view === null) {
    return error === null ? (
      <GalleryLoading />
    ) : (
      <GalleryMessage text="The media sources could not be read." />
    );
  }
  if (!MEDIA_CATEGORIES.some((category) => hasSources(view, category.id))) {
    return (
      <GalleryMessage text="No media directories are configured. Add them in Settings, Media Sources." />
    );
  }
  // A new record or new sources start from scratch: no image of the previous
  // record or configuration is ever shown.
  return (
    <TitleGallery
      key={`${targetKey}?${String(version)}`}
      target={target}
      title={title}
      version={version}
    />
  );
}

function GalleryMessage({ text }: { text: string }) {
  return (
    <section className="media-gallery media-gallery--empty" aria-label="Media">
      <p className="message message--muted" role="status">
        {text}
      </p>
    </section>
  );
}

/** Keeps the size of the gallery while the images are listed. */
function GalleryLoading() {
  return (
    <section className="media-gallery" aria-label="Media" aria-busy="true">
      <div className="media-gallery__carousel">
        <div className="media-gallery__viewport">
          <div className="media-gallery__stage media-gallery__stage--loading" role="status">
            Loading artwork…
          </div>
        </div>
      </div>
    </section>
  );
}

interface TitleGalleryProps {
  target: MediaTarget;
  title: string;
  version: number;
}

function TitleGallery({ target, title, version }: TitleGalleryProps) {
  // Keyed by the record: the target of the first render is the one listed.
  const [listed] = useState(target);
  const load = useCallback(() => listMedia(listed), [listed]);
  const { state, reload } = useLoader(load);

  if (state.status === "loading") return <GalleryLoading />;
  if (state.status === "error") {
    return (
      <section className="media-gallery media-gallery--empty" aria-label="Media">
        <p className="message message--error" role="alert">
          The artwork could not be listed: {describeError(state.error)}
        </p>
        <button type="button" className="button button--secondary button--small" onClick={reload}>
          Retry
        </button>
      </section>
    );
  }
  // Category order, never a category without images.
  const categories = MEDIA_CATEGORIES.flatMap((category) => {
    const found = state.value.find((entry) => entry.category === category.id);
    return found && found.count > 0 ? [{ ...found, label: category.label }] : [];
  });
  if (categories.length === 0) {
    return <GalleryMessage text="No artwork was found for this title in the media sources." />;
  }
  return <Carousel categories={categories} target={listed} title={title} version={version} />;
}

type ListedCategory = CategoryImages & { label: string };

interface CarouselProps {
  categories: ListedCategory[];
  target: MediaTarget;
  title: string;
  version: number;
}

function Carousel({ categories, target, title, version }: CarouselProps) {
  const [categoryId, setCategoryId] = useState<MediaCategoryId | null>(null);
  const [index, setIndex] = useState(0);
  const [failed, setFailed] = useState<ReadonlySet<string>>(new Set());
  const [enlarged, setEnlarged] = useState(false);
  const stageRef = useRef<HTMLButtonElement>(null);

  // The first category (in category order) is selected initially.
  const category = categories.find((entry) => entry.category === categoryId) ?? categories[0];
  if (!category) return null;
  const images: GalleryImage[] = Array.from({ length: category.count }, (_, position) => ({
    label: category.count > 1 ? `${category.label} ${String(position + 1)}` : category.label,
    src: mediaUrl(category.category, target, version, position),
  }));
  const current = Math.min(index, images.length - 1);
  const image = images[current];
  if (!image) return null;
  const several = images.length > 1;

  const markFailed = (src: string) => {
    setFailed((previous) => new Set(previous).add(src));
  };
  const show = (position: number) => {
    setIndex(Math.max(0, Math.min(images.length - 1, position)));
  };
  // An arrow button is disabled at either end: the focus moves to the stage
  // instead of being lost.
  const step = (offset: number) => {
    const position = current + offset;
    show(position);
    if (position <= 0 || position >= images.length - 1) stageRef.current?.focus();
  };
  const selectCategory = (id: MediaCategoryId) => {
    setCategoryId(id);
    setIndex(0);
  };

  function onKeyDown(event: ReactKeyboardEvent) {
    if (event.key === "ArrowLeft" && current > 0) {
      event.preventDefault();
      show(current - 1);
    } else if (event.key === "ArrowRight" && current < images.length - 1) {
      event.preventDefault();
      show(current + 1);
    } else if (event.key === "Home" && several) {
      event.preventDefault();
      show(0);
    } else if (event.key === "End" && several) {
      event.preventDefault();
      show(images.length - 1);
    }
  }

  return (
    <section className="media-gallery" aria-label="Media">
      <div
        className="media-gallery__carousel"
        role="region"
        aria-roledescription="carousel"
        aria-label={category.label}
        onKeyDown={onKeyDown}
      >
        <div className="media-gallery__viewport">
          <button
            ref={stageRef}
            type="button"
            className="media-gallery__stage"
            aria-label={`Enlarge ${image.label.toLowerCase()}`}
            onClick={() => {
              setEnlarged(true);
            }}
          >
            {failed.has(image.src) ? (
              <span className="media-gallery__unavailable">This image could not be read.</span>
            ) : (
              <img
                key={image.src}
                className="media-gallery__image"
                src={image.src}
                alt={`${title}: ${image.label}`}
                decoding="async"
                onError={() => {
                  markFailed(image.src);
                }}
              />
            )}
          </button>
          {several && (
            <>
              <button
                type="button"
                className="media-gallery__nav media-gallery__nav--previous"
                aria-label="Previous image"
                title="Previous image"
                disabled={current === 0}
                onClick={() => {
                  step(-1);
                }}
              >
                <ChevronLeftIcon size={22} />
              </button>
              <button
                type="button"
                className="media-gallery__nav media-gallery__nav--next"
                aria-label="Next image"
                title="Next image"
                disabled={current === images.length - 1}
                onClick={() => {
                  step(1);
                }}
              >
                <ChevronRightIcon size={22} />
              </button>
            </>
          )}
        </div>
        {/* Always present, so that the carousel keeps its height between categories. */}
        <div className="media-gallery__indicators">
          {several &&
            (images.length <= MAX_DOTS ? (
              <div className="media-gallery__dots" role="group" aria-label="Images">
                {images.map((entry, position) => (
                  <button
                    key={entry.src}
                    type="button"
                    className="media-gallery__dot"
                    aria-label={`Image ${String(position + 1)} of ${String(images.length)}`}
                    aria-current={position === current ? "true" : undefined}
                    onClick={() => {
                      show(position);
                    }}
                  />
                ))}
              </div>
            ) : (
              <span className="media-gallery__position">
                {current + 1} / {images.length}
              </span>
            ))}
        </div>
        <p className="visually-hidden" aria-live="polite">
          {`${image.label}, image ${String(current + 1)} of ${String(images.length)}`}
        </p>
      </div>

      <ul className="media-gallery__categories" aria-label="Image categories">
        {categories.map((entry) => {
          const thumbnail = mediaUrl(entry.category, target, version);
          return (
            <li key={entry.category}>
              <button
                type="button"
                className="media-gallery__category"
                aria-pressed={entry.category === category.category}
                aria-label={`${entry.label} (${String(entry.count)})`}
                onClick={() => {
                  selectCategory(entry.category);
                }}
              >
                <span className="media-gallery__category-frame">
                  {!failed.has(thumbnail) && (
                    <img
                      src={thumbnail}
                      alt=""
                      decoding="async"
                      data-category={entry.category}
                      onError={() => {
                        markFailed(thumbnail);
                      }}
                    />
                  )}
                </span>
                <span className="media-gallery__category-label" aria-hidden="true">
                  <span>{entry.label}</span>
                  {entry.count > 1 && (
                    <span className="media-gallery__category-count">{entry.count}</span>
                  )}
                </span>
              </button>
            </li>
          );
        })}
      </ul>

      {enlarged && (
        <Lightbox
          images={images}
          index={current}
          title={title}
          onIndex={show}
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
 * Modal view of one image of the selected category at the largest size that
 * fits the window, with the arrow keys moving between its images and Escape
 * closing it. Focus stays inside while it is open and returns to the gallery
 * when it closes.
 */
function Lightbox({ images, index, title, onIndex, onClose }: LightboxProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const image = images[index];
  const several = images.length > 1;
  const first = index === 0;
  const last = index === images.length - 1;

  useEffect(() => {
    closeRef.current?.focus();
  }, []);
  // A navigation button disabled at either end gives the focus back to Close.
  useEffect(() => {
    const active = document.activeElement;
    if (!isInside(dialogRef) || (active instanceof HTMLButtonElement && active.disabled)) {
      closeRef.current?.focus();
    }
  }, [index]);

  if (!image) return null;

  function onKeyDown(event: ReactKeyboardEvent) {
    // Handled here only: the gallery behind must not react too.
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      if (!first) onIndex(index - 1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      if (!last) onIndex(index + 1);
    } else if (event.key === "Tab") {
      // Keeps the focus on the lightbox controls.
      const controls = [
        ...(dialogRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
      ];
      const firstControl = controls[0];
      const lastControl = controls.at(-1);
      if (!firstControl || !lastControl) return;
      if (event.shiftKey && (document.activeElement === firstControl || !isInside(dialogRef))) {
        event.preventDefault();
        lastControl.focus();
      } else if (
        !event.shiftKey &&
        (document.activeElement === lastControl || !isInside(dialogRef))
      ) {
        event.preventDefault();
        firstControl.focus();
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
            disabled={first}
            onClick={() => {
              onIndex(index - 1);
            }}
          >
            <ChevronLeftIcon size={20} />
          </button>
        )}
        <img
          key={image.src}
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
            disabled={last}
            onClick={() => {
              onIndex(index + 1);
            }}
          >
            <ChevronRightIcon size={20} />
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
