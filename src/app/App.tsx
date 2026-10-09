import { useState } from "react";
import { CatalogPanel } from "../features/catalog/CatalogPanel";
import { HealthCheck } from "../features/health-check/HealthCheck";
import { MameInstallation } from "../features/mame-installation/MameInstallation";

export function App() {
  // Incremented when the MAME configuration changes, so the catalog panel
  // re-checks whether its catalog matches the configured executable.
  const [mameConfigurationVersion, setMameConfigurationVersion] = useState(0);

  return (
    <main className="app">
      <header className="app__header">
        <p className="app__acronym">YMDB</p>
        <h1 className="app__title">Your MAME DB</h1>
        <p className="app__version">Version {__APP_VERSION__}</p>
        <p className="app__notice">
          Early development build. Browsing, search and launch features are not available yet.
        </p>
      </header>

      <MameInstallation
        onConfigurationChanged={() => {
          setMameConfigurationVersion((version) => version + 1);
        }}
      />
      <CatalogPanel refreshKey={mameConfigurationVersion} />
      <HealthCheck />
    </main>
  );
}
