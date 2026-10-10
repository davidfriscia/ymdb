import { useInstallations } from "./installationsContext";

interface InstallationSelectProps {
  /** Opens the MAME page, where installations are added and catalogs imported. */
  onOpenMame: () => void;
}

/**
 * The "MAME Version" choice: switches the active installation, whose catalog
 * the browser shows and whose executable new launches use.
 */
export function InstallationSelect({ onOpenMame }: InstallationSelectProps) {
  const { installations, active, activeId, labels, select, selectError } = useInstallations();

  if (installations.length === 0) {
    return (
      <div className="installation-select">
        <p className="message message--muted">No MAME installation is registered.</p>
        <button type="button" className="link-button" onClick={onOpenMame}>
          Add an installation
        </button>
      </div>
    );
  }
  return (
    <div className="installation-select">
      <select
        className="input"
        aria-label="MAME version"
        value={activeId ?? ""}
        onChange={(event) => {
          select(event.target.value);
        }}
      >
        {activeId === null && <option value="">Select an installation</option>}
        {installations.map((installation) => (
          <option key={installation.id} value={installation.id}>
            {labels.get(installation.id)}
            {installation.catalogImported ? "" : " — no catalog"}
          </option>
        ))}
      </select>
      {active && (
        <p className="installation-select__status" title={active.executablePath}>
          {active.catalogImported ? (
            "Catalog imported"
          ) : (
            <>
              No catalog imported.{" "}
              <button type="button" className="link-button" onClick={onOpenMame}>
                Import it
              </button>
            </>
          )}
        </p>
      )}
      {selectError && (
        <p className="message message--error" role="alert">
          The MAME version could not be changed: {selectError}
        </p>
      )}
    </div>
  );
}
