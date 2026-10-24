import type { ReactNode } from "react";
import { ApplicationDataSettings } from "../features/app-data/ApplicationDataSettings";
import type { RestoreOutcome } from "../features/app-data/appDataApi";
import { HealthCheck } from "../features/health-check/HealthCheck";
import { MediaSourcesSettings } from "../features/media/MediaSourcesSettings";
import { RemoteConnectionsSettings } from "../features/remote/RemoteConnectionsSettings";
import { ControllersSettings } from "../features/controllers/ControllersSettings";
import { FavoritesSettings } from "../features/lists/FavoritesSettings";
import { SupportFilesSettings } from "../features/support-files/SupportFilesSettings";
import { TwoPaneLayout, TwoPaneNavItem } from "../shared/TwoPaneLayout";
import { useVisited } from "../shared/useVisited";
import { THEMES, type ThemeId } from "./theme/themes";

export type SettingsSection =
  | "appearance"
  | "mediaSources"
  | "remoteConnections"
  | "controllers"
  | "favorites"
  | "supportFiles"
  | "applicationData"
  | "about"
  | "diagnostics";

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "mediaSources", label: "Media Sources" },
  { id: "remoteConnections", label: "Remote Connections" },
  { id: "controllers", label: "Controllers" },
  { id: "favorites", label: "Favorites" },
  { id: "supportFiles", label: "Support Files" },
  { id: "applicationData", label: "Application Data" },
  { id: "about", label: "About" },
  { id: "diagnostics", label: "Diagnostics" },
];

interface SettingsViewProps {
  theme: ThemeId;
  onThemeChange: (theme: ThemeId) => void;
  /** The selected section; the page stays mounted, so it survives visits elsewhere. */
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  /** The support files changed: catalogs reload their classifications. */
  onSupportFilesChanged: () => void;
  /** Result of a restore applied when YMDB started, if any. */
  restoreOutcome?: RestoreOutcome | null;
}

/**
 * The global settings, one section at a time. Launch options and ROM Manager
 * settings belong to an installation: they are on the MAME page.
 */
export function SettingsView({
  theme,
  onThemeChange,
  section: selected,
  onSectionChange: setSelected,
  onSupportFilesChanged,
  restoreOutcome = null,
}: SettingsViewProps) {
  // Sections stay mounted once opened, so that unsaved forms are kept.
  const visited = useVisited(selected);
  const heading = SECTIONS.find((section) => section.id === selected)?.label ?? "";

  function content(section: SettingsSection): ReactNode {
    switch (section) {
      case "appearance":
        return <Appearance theme={theme} onThemeChange={onThemeChange} />;
      case "mediaSources":
        return <MediaSourcesSettings />;
      case "remoteConnections":
        return <RemoteConnectionsSettings />;
      case "controllers":
        return <ControllersSettings />;
      case "favorites":
        return <FavoritesSettings />;
      case "supportFiles":
        return <SupportFilesSettings onChanged={onSupportFilesChanged} />;
      case "applicationData":
        return <ApplicationDataSettings restoreOutcome={restoreOutcome} />;
      case "about":
        return <About />;
      case "diagnostics":
        return <HealthCheck />;
    }
  }

  return (
    <TwoPaneLayout
      title="Settings"
      navigationLabel="Settings sections"
      heading={heading}
      selectionKey={selected}
      navigation={
        <ul className="two-pane__list">
          {SECTIONS.map((section) => (
            <li key={section.id}>
              <TwoPaneNavItem
                selected={section.id === selected}
                onSelect={() => {
                  setSelected(section.id);
                }}
              >
                {section.label}
              </TwoPaneNavItem>
            </li>
          ))}
        </ul>
      }
    >
      {SECTIONS.filter((section) => visited.has(section.id)).map((section) => (
        <div key={section.id} hidden={section.id !== selected}>
          {content(section.id)}
        </div>
      ))}
    </TwoPaneLayout>
  );
}

function Appearance({ theme, onThemeChange }: Pick<SettingsViewProps, "theme" | "onThemeChange">) {
  return (
    <section className="panel" aria-label="Appearance">
      <fieldset className="choice-group">
        <legend>Theme</legend>
        {THEMES.map((option) => (
          <label key={option.id} className="choice">
            <input
              type="radio"
              name="theme"
              value={option.id}
              checked={theme === option.id}
              onChange={() => {
                onThemeChange(option.id);
              }}
            />
            {option.label}
          </label>
        ))}
      </fieldset>
      <p className="message message--muted">
        The theme applies immediately to the whole application. Interface preferences are the same
        for every MAME installation.
      </p>
    </section>
  );
}

function About() {
  return (
    <section className="panel" aria-label="About">
      <p className="about__name">Your MAME DB (YMDB)</p>
      <p className="message message--muted">Version {__APP_VERSION__}</p>
      <p className="message">
        A cross-platform desktop frontend for MAME: browse the catalog of each MAME installation,
        launch machines and software, and download the files they need from your own remote
        repositories.
      </p>
      <p className="message message--muted">
        Early development build. Open source under the Apache License 2.0. YMDB does not include
        ROMs, CHDs or BIOS files.
      </p>
    </section>
  );
}
