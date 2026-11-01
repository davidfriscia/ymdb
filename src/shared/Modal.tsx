import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";

const FOCUSABLE =
  'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  title: ReactNode;
  /** Called by Escape, the backdrop and the close button of the header. */
  onClose: () => void;
  /** Accessible name of the close button of the header. */
  closeLabel?: string;
  /** `narrow` for small dialogs, such as a selection list; `wide` for editors. */
  size?: "default" | "narrow" | "wide";
  children: ReactNode;
}

/**
 * A modal dialog over the page. The focus moves into it when it opens, stays
 * inside while it is open, and returns to the element that had it (usually
 * the control that opened it) when it closes. Only the content scrolls, so
 * the header and its close button stay reachable in small windows.
 */
export function Modal({
  title,
  onClose,
  closeLabel = "Close window",
  size = "default",
  children,
}: ModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  // The latest handler, for the document listener installed once.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    // The first control of the content, else the dialog itself.
    const first = dialog?.querySelector(".modal__body")?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? dialog)?.focus();

    // On the document: the content may change while the window is open, and
    // a control that disappears or becomes disabled leaves the focus outside.
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onCloseRef.current();
      } else if (event.key === "Tab") {
        const controls = [...(dialog?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])];
        const firstControl = controls[0];
        const lastControl = controls.at(-1);
        if (!firstControl || !lastControl) {
          event.preventDefault();
          return;
        }
        const active = document.activeElement;
        const inside = dialog?.contains(active) ?? false;
        if (event.shiftKey && (active === firstControl || !inside)) {
          event.preventDefault();
          lastControl.focus();
        } else if (!event.shiftKey && (active === lastControl || !inside)) {
          event.preventDefault();
          firstControl.focus();
        }
      }
    }
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  return createPortal(
    <div
      className="modal"
      onMouseDown={(event) => {
        // A press on the backdrop, outside the dialog, closes.
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        className={size === "default" ? "modal__dialog" : `modal__dialog modal__dialog--${size}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <header className="modal__header">
          <h2 id={titleId} className="modal__title">
            {title}
          </h2>
          <button
            type="button"
            className="modal__close"
            aria-label={closeLabel}
            title={closeLabel}
            onClick={onClose}
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" focusable="false">
              <path
                d="m3.5 3.5 9 9m0-9-9 9"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
              />
            </svg>
          </button>
        </header>
        <div className="modal__body">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
