import { useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { TwoPaneLayout, TwoPaneNavItem } from "../../shared/TwoPaneLayout";
import { useVisited } from "../../shared/useVisited";
import { CatalogPanel } from "../catalog/CatalogPanel";
import { LaunchSettings } from "../launcher/LaunchSettings";
import { formatVersion } from "../mame-installation/messages";
import { RomManagerSection } from "../remote/RomManagerSection";
import { AddInstallationPanel } from "./AddInstallationPanel";
import type { InstallationSummary } from "./installationsApi";
import { useInstallations } from "./installationsContext";
import { describeCatalog } from "./installationMessages";
import { InstallationDetails } from "./InstallationDetails";

type MameSection = "status" | "catalog" | "romManager" | "launchOptions";

const SECTIONS: { id: MameSection; label: string }[] = [
  { id: "status", label: "Status" },
  { id: "catalog", label: "Catalog" },
  { id: "romManager", label: "ROM Manager" },
  { id: "launchOptions", label: "Launch Options" },
];

/** What the content panel shows. Installations are named by their id, never by their version. */
type Selection =
  { kind: "add" } | { kind: "installation"; installationId: string; section: MameSection };

const ADD_KEY = "add";

function keyOf(installationId: string, section: MameSection): string {
  return `${installationId}:${section}`;
}

interface Notice {
  text: string;
  /** Files kept by a removal. */
  keptPaths?: string[];
}

interface MamePageProps {
  /** Changes when a catalog may have changed. */
  refreshKey: number;
  /** Called after an import replaced a catalog. */
  onCatalogChanged: () => void;
}

/**
 * The MAME page: a navigation grouped by installation (Status, Catalog, ROM
 * Manager, Launch Options) and the selected section. Selecting a section never
 * changes the active installation.
 */
export function MamePage({ refreshKey, onCatalogChanged }: MamePageProps) {
  const { view, error, installations, activeId, labels, reload } = useInstallations();
  const [chosen, setChosen] = useState<Selection | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  // A choice naming a removed installation falls back to the active one, so the
  // selection always names a registered installation.
  const fallback =
    installations.find((installation) => installation.id === activeId) ?? installations.at(0);
  const selection: Selection =
    chosen?.kind === "add" ||
    (chosen?.kind === "installation" &&
      installations.some((installation) => installation.id === chosen.installationId))
      ? chosen
      : fallback
        ? { kind: "installation", installationId: fallback.id, section: "status" }
        : { kind: "add" };
  const selected =
    selection.kind === "installation"
      ? installations.find((installation) => installation.id === selection.installationId)
      : undefined;
  const currentKey =
    selection.kind === "installation"
      ? keyOf(selection.installationId, selection.section)
      : ADD_KEY;
  // Sections stay mounted once opened, so that unsaved forms and running
  // imports survive a change of selection.
  const visited = useVisited(currentKey);

  // The group of the selection is always expanded when it is selected.
  const [revealed, setRevealed] = useState<string | null>(null);
  if (selected && selected.id !== revealed) {
    setRevealed(selected.id);
    setExpanded((current) => new Set(current).add(selected.id));
  }

  function navigate(next: Selection) {
    setChosen(next);
    setNotice(null);
  }

  function toggle(id: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  function content(installation: InstallationSummary, section: MameSection): ReactNode {
    switch (section) {
      case "status":
        return (
          <InstallationDetails
            installation={installation}
            onRemoved={(result, label) => {
              setChosen(null);
              setNotice({ text: `${label} was removed from YMDB.`, keptPaths: result.keptPaths });
            }}
          />
        );
      case "catalog":
        return (
          <CatalogPanel
            installation={installation}
            refreshKey={refreshKey}
            onCatalogChanged={onCatalogChanged}
          />
        );
      case "romManager":
        return <RomManagerSection installation={installation} />;
      case "launchOptions":
        return <LaunchSettings installationId={installation.id} />;
    }
  }

  const heading =
    selection.kind === "add"
      ? "Add Installation"
      : (SECTIONS.find((section) => section.id === selection.section)?.label ?? "");
  const context = selected && (
    <>
      <span>{labels.get(selected.id) ?? selected.label}</span>
      {selected.id === activeId && <span className="badge badge--success">Active</span>}
    </>
  );

  return (
    <TwoPaneLayout
      title="MAME"
      navigationLabel="MAME installations"
      heading={heading}
      context={context}
      selectionKey={currentKey}
      navigation={
        <>
          <TwoPaneNavItem
            selected={selection.kind === "add"}
            onSelect={() => {
              navigate({ kind: "add" });
            }}
          >
            <span aria-hidden="true">+</span> Add Installation
          </TwoPaneNavItem>
          <p className="two-pane__separator" aria-hidden="true">
            Installations
          </p>
          {view === null && error === null && <p className="two-pane__note">Loading…</p>}
          {view !== null && installations.length === 0 && (
            <p className="two-pane__note">No installation registered.</p>
          )}
          <ul className="two-pane__list" aria-label="Registered installations">
            {installations.map((installation) => (
              <InstallationGroup
                key={installation.id}
                installation={installation}
                label={labels.get(installation.id) ?? installation.label}
                active={installation.id === activeId}
                expanded={expanded.has(installation.id)}
                selectedSection={
                  selection.kind === "installation" && selection.installationId === installation.id
                    ? selection.section
                    : null
                }
                onToggle={() => {
                  toggle(installation.id);
                }}
                onSelect={(section) => {
                  navigate({ kind: "installation", installationId: installation.id, section });
                }}
              />
            ))}
          </ul>
        </>
      }
    >
      {error !== null && (
        <div className="panel state-message" role="alert">
          <p className="message message--error">
            The MAME installations could not be read: {describeError(error)}
          </p>
          <button type="button" className="button button--secondary" onClick={() => void reload()}>
            Retry
          </button>
        </div>
      )}
      {notice && (
        <div className="panel" role="status">
          <p className="message message--success">{notice.text}</p>
          {notice.keptPaths && notice.keptPaths.length > 0 && (
            <>
              <p className="message message--muted">These files were kept, as chosen:</p>
              <ul className="kept-paths">
                {notice.keptPaths.map((path) => (
                  <li key={path} className="code">
                    {path}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
      {/* Always mounted: detected installations stay listed after adding one. */}
      <div hidden={selection.kind !== "add"}>
        <AddInstallationPanel
          onShowInstallation={(id, message) => {
            navigate({ kind: "installation", installationId: id, section: "status" });
            setNotice({ text: message });
          }}
        />
      </div>
      {installations.flatMap((installation) =>
        SECTIONS.filter((section) => visited.has(keyOf(installation.id, section.id))).map(
          (section) => {
            const key = keyOf(installation.id, section.id);
            return (
              <div key={key} hidden={key !== currentKey}>
                {content(installation, section.id)}
              </div>
            );
          },
        ),
      )}
    </TwoPaneLayout>
  );
}

interface InstallationGroupProps {
  installation: InstallationSummary;
  label: string;
  active: boolean;
  expanded: boolean;
  /** The section selected in this group, if any. */
  selectedSection: MameSection | null;
  onToggle: () => void;
  onSelect: (section: MameSection) => void;
}

function InstallationGroup({
  installation,
  label,
  active,
  expanded,
  selectedSection,
  onToggle,
  onSelect,
}: InstallationGroupProps) {
  const sectionsId = `mame-sections-${installation.id}`;
  return (
    <li>
      <button
        type="button"
        className={`two-pane__group${selectedSection !== null ? " two-pane__group--current" : ""}`}
        aria-expanded={expanded}
        aria-controls={sectionsId}
        onClick={onToggle}
      >
        <span className="two-pane__chevron" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
        <span className="two-pane__group-text">
          {/* Spaces separate the parts of the accessible name. */}
          <span className="two-pane__group-title">
            {label} {active && <span className="badge badge--success">Active</span>}{" "}
            {installation.executableStatus === "changed" && (
              <span className="badge badge--warning">Changed</span>
            )}
            {installation.executableStatus === "missing" && (
              <span className="badge badge--error">Missing</span>
            )}
          </span>{" "}
          <span className="two-pane__group-meta">
            {installation.version ? formatVersion(installation.version) : "Version unknown"} ·{" "}
            {describeCatalog(installation)}
          </span>
        </span>
      </button>
      <ul id={sectionsId} className="two-pane__list" aria-label={label} hidden={!expanded}>
        {SECTIONS.map((section) => (
          <li key={section.id}>
            <TwoPaneNavItem
              nested
              selected={section.id === selectedSection}
              onSelect={() => {
                onSelect(section.id);
              }}
            >
              {section.label} <span className="visually-hidden">of {label}</span>
            </TwoPaneNavItem>
          </li>
        ))}
      </ul>
    </li>
  );
}
