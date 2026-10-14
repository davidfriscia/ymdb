import type { ReactNode } from "react";

interface DisclosureProps {
  expanded: boolean;
  /** Id of the element that the button shows and hides. */
  controls: string;
  onToggle: () => void;
  children: ReactNode;
}

/**
 * The button that opens and closes a collapsible launch section (the launch
 * status, Advanced), with a chevron turning when it is open.
 */
export function Disclosure({ expanded, controls, onToggle, children }: DisclosureProps) {
  return (
    <button
      type="button"
      className="launch-section__disclosure"
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={onToggle}
    >
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
        <path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" strokeWidth="1.75" />
      </svg>
      {children}
    </button>
  );
}
