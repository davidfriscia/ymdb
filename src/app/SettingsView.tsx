import { HealthCheck } from "../features/health-check/HealthCheck";
import { LaunchSettings } from "../features/launcher/LaunchSettings";
import { MediaSourcesSettings } from "../features/media/MediaSourcesSettings";
import { RemoteConnectionsSettings } from "../features/remote/RemoteConnectionsSettings";
import { THEMES, type ThemeId } from "./theme/themes";

interface SettingsViewProps {
  theme: ThemeId;
  onThemeChange: (theme: ThemeId) => void;
}

export function SettingsView({ theme, onThemeChange }: SettingsViewProps) {
  return (
    <div className="page">
      <h1 className="page__title">Settings</h1>

      <section className="panel" aria-labelledby="appearance-title">
        <h2 id="appearance-title" className="panel__title">
          Appearance
        </h2>
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
      </section>

      <section className="panel" aria-labelledby="about-title">
        <h2 id="about-title" className="panel__title">
          About
        </h2>
        <p className="about__name">Your MAME DB (YMDB)</p>
        <p className="message message--muted">Version {__APP_VERSION__}</p>
        <p className="message message--muted">
          Early development build. Remote repositories can be configured, but ROMs are not
          downloaded yet.
        </p>
      </section>

      <MediaSourcesSettings />

      <RemoteConnectionsSettings />

      <LaunchSettings />

      <HealthCheck />
    </div>
  );
}
