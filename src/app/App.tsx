import { useCallback, useEffect, useState, type ReactNode } from "react";
import { CatalogBrowser } from "../features/browser/CatalogBrowser";
import type { BrowserPreferences } from "../features/browser/browserPreferences";
import { CatalogPanel } from "../features/catalog/CatalogPanel";
import { LauncherProvider } from "../features/launcher/LauncherProvider";
import { MediaProvider } from "../features/media/MediaProvider";
import { MameInstallation } from "../features/mame-installation/MameInstallation";
import { loadPreferences, savePreferences, type UiPreferences } from "./preferences";
import { SettingsView } from "./SettingsView";
import { applyTheme } from "./theme/themes";

type View = "catalog" | "mame" | "settings";

const NAVIGATION: { view: View; label: string; icon: ReactNode }[] = [
  {
    view: "catalog",
    label: "Catalog",
    icon: <path d="M3 4h18v4H3zM3 10h18v4H3zM3 16h18v4H3z" />,
  },
  {
    view: "mame",
    label: "MAME",
    icon: <path d="M4 6h16v10H4zM8 20h8M12 16v4" />,
  },
  {
    view: "settings",
    label: "Settings",
    icon: (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1" />
      </>
    ),
  },
];

export function App() {
  const [view, setView] = useState<View>("catalog");
  const [preferences, setPreferences] = useState<UiPreferences>(() => loadPreferences());
  // Incremented when the MAME configuration changes, so the catalog views
  // re-check whether their catalog matches the configured executable.
  const [mameConfigurationVersion, setMameConfigurationVersion] = useState(0);
  // Incremented when an import replaces the catalog.
  const [catalogVersion, setCatalogVersion] = useState(0);

  useEffect(() => {
    applyTheme(preferences.theme);
    savePreferences(preferences);
  }, [preferences]);

  const updatePreferences = useCallback((change: Partial<UiPreferences>) => {
    setPreferences((current) => ({ ...current, ...change }));
  }, []);
  const onCatalogChanged = useCallback(() => {
    setCatalogVersion((version) => version + 1);
  }, []);
  const onConfigurationChanged = useCallback(() => {
    setMameConfigurationVersion((version) => version + 1);
  }, []);
  const onShowCommandPreviewChange = useCallback(
    (showCommandPreview: boolean) => {
      updatePreferences({ showCommandPreview });
    },
    [updatePreferences],
  );

  const browserPreferences: BrowserPreferences = {
    filtersVisible: preferences.filtersVisible,
    viewOptionsVisible: preferences.viewOptionsVisible,
    renderer: preferences.renderer,
    rendererOptions: preferences.rendererOptions,
  };

  return (
    <MediaProvider>
      <LauncherProvider
        showCommandPreview={preferences.showCommandPreview}
        onShowCommandPreviewChange={onShowCommandPreviewChange}
      >
        <div className="shell">
          <nav className="shell__nav" aria-label="Main">
            <p className="shell__brand" title="Your MAME DB">
              YMDB
            </p>
            <ul className="shell__nav-list">
              {NAVIGATION.map((item) => (
                <li key={item.view}>
                  <button
                    type="button"
                    className="shell__nav-item"
                    aria-current={view === item.view ? "page" : undefined}
                    onClick={() => {
                      setView(item.view);
                    }}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      width="20"
                      height="20"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.75"
                      strokeLinecap="round"
                      aria-hidden="true"
                    >
                      {item.icon}
                    </svg>
                    <span>{item.label}</span>
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          {/* Views stay mounted so that their state (search, filters, scroll) survives navigation. */}
          <main className="shell__main shell__main--catalog" hidden={view !== "catalog"}>
            <CatalogBrowser
              refreshKey={catalogVersion + mameConfigurationVersion}
              preferences={browserPreferences}
              onPreferencesChange={updatePreferences}
              onOpenMame={() => {
                setView("mame");
              }}
            />
          </main>

          <main className="shell__main" hidden={view !== "mame"}>
            <div className="page">
              <h1 className="page__title">MAME</h1>
              <MameInstallation onConfigurationChanged={onConfigurationChanged} />
              <CatalogPanel
                refreshKey={mameConfigurationVersion}
                onCatalogChanged={onCatalogChanged}
              />
            </div>
          </main>

          <main className="shell__main" hidden={view !== "settings"}>
            <SettingsView
              theme={preferences.theme}
              onThemeChange={(theme) => {
                updatePreferences({ theme });
              }}
            />
          </main>
        </div>
      </LauncherProvider>
    </MediaProvider>
  );
}
