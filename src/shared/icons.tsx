import type { ReactNode } from "react";

// Small inline icons drawn with `currentColor`, so they follow the theme and
// the state of the control that contains them. They are decorative: the
// control carries the accessible name.

interface IconProps {
  size?: number;
}

function Icon({ size = 16, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 16 16"
      width={size}
      height={size}
      aria-hidden="true"
      focusable="false"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

export function ChevronLeftIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10 3 5 8l5 5" />
    </Icon>
  );
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m6 3 5 5-5 5" />
    </Icon>
  );
}

export function PlayIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 2.5v11l9-5.5z" fill="currentColor" />
    </Icon>
  );
}

/** A checklist: preparing the list of files to download. */
export function PlanIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="2.5" y="2" width="11" height="12.5" rx="1.5" />
      <path d="m4.75 5.25 1 1 1.75-1.75M9 5.5h2.75M4.75 9.75l1 1 1.75-1.75M9 10h2.75" />
    </Icon>
  );
}

/** A download arrow followed by a play triangle: download, then play. */
export function DownloadPlayIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M5 1.5v8M2 6.5l3 3 3-3M1.5 13.5h7" />
      <path d="M10.5 5.5v8l5-4z" fill="currentColor" strokeWidth="1" />
    </Icon>
  );
}

export function DownloadIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M8 1.5v9M4.5 7l3.5 3.5L11.5 7M2.5 14h11" />
    </Icon>
  );
}
