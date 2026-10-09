import type { ReactNode } from "react";
import type { SupportStatus } from "../search/searchApi";
import { supportLabel } from "./filterDefinitions";

type Tone = "success" | "warning" | "error" | "neutral";

function statusTone(status: string | null): Tone {
  switch (status) {
    case "good":
    case "yes":
      return "success";
    case "imperfect":
    case "partial":
      return "warning";
    case "preliminary":
    case "no":
      return "error";
    default:
      return "neutral";
  }
}

export function Badge({ tone, children }: { tone: Tone; children: ReactNode }) {
  return <span className={tone === "neutral" ? "badge" : `badge badge--${tone}`}>{children}</span>;
}

/** Overall driver status of a machine; nothing when not declared. */
export function DriverStatusBadge({ status }: { status: string | null }) {
  if (status === null) return null;
  return <Badge tone={statusTone(status)}>Driver: {status}</Badge>;
}

export function StatusBadge({ label, status }: { label: string; status: string | null }) {
  if (status === null) return null;
  return (
    <Badge tone={statusTone(status)}>
      {label}: {status}
    </Badge>
  );
}

export function SupportBadge({ supported }: { supported: SupportStatus }) {
  return <Badge tone={statusTone(supported)}>{supportLabel(supported)}</Badge>;
}
