import { useState } from "react";
import { CatalogPanel } from "../catalog/CatalogPanel";
import { RomManagerSection } from "../remote/RomManagerSection";
import type { RemovalResult } from "./installationsApi";
import { useInstallations } from "./installationsContext";
import { InstallationDetails } from "./InstallationDetails";
import { InstallationsPanel } from "./InstallationsPanel";

interface MamePageProps {
  /** Changes when a catalog may have changed. */
  refreshKey: number;
  /** Called after an import replaced a catalog. */
  onCatalogChanged: () => void;
}

/**
 * The MAME page: the registered installations, then the installation selected
 * here and its catalog. Every action names the installation it applies to.
 */
export function MamePage({ refreshKey, onCatalogChanged }: MamePageProps) {
  const { installations, activeId } = useInstallations();
  const [chosenId, setChosenId] = useState<string | null>(null);
  const [removal, setRemoval] = useState<{ result: RemovalResult; label: string } | null>(null);
  // The installation chosen on this page, or the active one.
  const selected =
    installations.find((installation) => installation.id === chosenId) ??
    installations.find((installation) => installation.id === activeId) ??
    installations.at(0) ??
    null;

  return (
    <div className="page">
      <h1 className="page__title">MAME</h1>
      <InstallationsPanel
        selectedId={selected?.id ?? null}
        onSelect={(id) => {
          setChosenId(id);
          setRemoval(null);
        }}
      />
      {removal && (
        <div className="panel" role="status">
          <p className="message message--success">{removal.label} was removed from YMDB.</p>
          {removal.result.keptPaths.length > 0 && (
            <>
              <p className="message message--muted">These files were kept, as chosen:</p>
              <ul className="kept-paths">
                {removal.result.keptPaths.map((path) => (
                  <li key={path} className="code">
                    {path}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
      {selected && (
        <>
          <InstallationDetails
            key={selected.id}
            installation={selected}
            onRemoved={(result, label) => {
              setChosenId(null);
              setRemoval({ result, label });
            }}
          />
          <CatalogPanel
            key={`catalog:${selected.id}`}
            installation={selected}
            refreshKey={refreshKey}
            onCatalogChanged={onCatalogChanged}
          />
          <RomManagerSection key={`rom-manager:${selected.id}`} installation={selected} />
        </>
      )}
    </div>
  );
}
