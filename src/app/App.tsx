import { HealthCheck } from "../features/health-check/HealthCheck";

export function App() {
  return (
    <main className="app">
      <header className="app__header">
        <p className="app__acronym">YMDB</p>
        <h1 className="app__title">Your MAME DB</h1>
        <p className="app__version">Version {__APP_VERSION__}</p>
        <p className="app__notice">
          Early development build. Catalog, search and launch features are not available yet.
        </p>
      </header>

      <HealthCheck />
    </main>
  );
}
