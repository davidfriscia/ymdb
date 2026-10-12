import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

interface TwoPaneLayoutProps {
  /** The page title, shown above the navigation. */
  title: string;
  /** Accessible name of the secondary navigation. */
  navigationLabel: string;
  /** Items of the secondary navigation (`TwoPaneNavItem` and groups). */
  navigation: ReactNode;
  /** Heading of the content panel: the selected item. */
  heading: string;
  /** Shown above the heading, for example the installation of the item. */
  context?: ReactNode;
  /** Changes when another item is selected, to scroll the content back to the top. */
  selectionKey: string;
  children: ReactNode;
}

/**
 * A page with a secondary navigation on the left and the content of the
 * selected item on the right. Both columns scroll independently; on narrow
 * windows the navigation collapses behind a toggle.
 */
export function TwoPaneLayout({
  title,
  navigationLabel,
  navigation,
  heading,
  context,
  selectionKey,
  children,
}: TwoPaneLayoutProps) {
  const id = useId();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const content = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (content.current) content.current.scrollTop = 0;
  }, [selectionKey]);

  return (
    <div className={`two-pane${navigationOpen ? " two-pane--navigation-open" : ""}`}>
      <div className="two-pane__sidebar">
        <h1 className="two-pane__title">{title}</h1>
        <button
          type="button"
          className="button button--secondary two-pane__toggle"
          aria-expanded={navigationOpen}
          aria-controls={`${id}-navigation`}
          aria-label={`Sections, current: ${heading}`}
          onClick={() => {
            setNavigationOpen(!navigationOpen);
          }}
        >
          {heading}
          <span aria-hidden="true">{navigationOpen ? " ▴" : " ▾"}</span>
        </button>
        <nav
          id={`${id}-navigation`}
          className="two-pane__navigation"
          aria-label={navigationLabel}
          onKeyDown={moveFocus}
          onClick={(event) => {
            // Choosing an item closes the navigation on narrow windows.
            if (event.target instanceof Element && event.target.closest("[data-two-pane-item]")) {
              setNavigationOpen(false);
            }
          }}
        >
          {navigation}
        </nav>
      </div>
      <div
        ref={content}
        className="two-pane__content"
        role="region"
        aria-labelledby={`${id}-heading`}
      >
        <div className="two-pane__body">
          <header className="two-pane__header">
            {context !== undefined && <div className="two-pane__context">{context}</div>}
            <h2 id={`${id}-heading`} className="two-pane__heading">
              {heading}
            </h2>
          </header>
          {children}
        </div>
      </div>
    </div>
  );
}

/** Arrow keys, Home and End move the focus between the visible navigation buttons. */
function moveFocus(event: KeyboardEvent<HTMLElement>) {
  const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
  if (!keys.includes(event.key)) return;
  const buttons = Array.from(
    event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
  ).filter((button) => button.closest("[hidden]") === null);
  const index = buttons.findIndex((button) => button === document.activeElement);
  if (index === -1 || buttons.length === 0) return;
  event.preventDefault();
  const last = buttons.length - 1;
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? last
        : event.key === "ArrowDown"
          ? Math.min(index + 1, last)
          : Math.max(index - 1, 0);
  buttons[next]?.focus();
}

interface TwoPaneNavItemProps {
  selected: boolean;
  onSelect: () => void;
  /** Items inside a group are indented. */
  nested?: boolean;
  children: ReactNode;
}

/** One item of the secondary navigation. */
export function TwoPaneNavItem({
  selected,
  onSelect,
  nested = false,
  children,
}: TwoPaneNavItemProps) {
  return (
    <button
      type="button"
      className={`two-pane__item${nested ? " two-pane__item--nested" : ""}`}
      aria-current={selected ? "page" : undefined}
      data-two-pane-item=""
      onClick={onSelect}
    >
      {children}
    </button>
  );
}
