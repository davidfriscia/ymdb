import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { loadPreferences } from "./app/preferences";
import { applyStartupTheme } from "./features/themes/startupTheme";
import "./app/theme/themes.css";
import "./app/App.css";
import "./features/browser/browser.css";
import "./features/media/media.css";
import "./features/launcher/launcher.css";
import "./features/remote/remote.css";
import "./features/roms/roms.css";
import "./features/lists/lists.css";
import "./features/support-files/supportFiles.css";
import "./features/controllers/controllers.css";
import "./features/app-data/appData.css";
import "./features/themes/themeEditor.css";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found");
}

// Apply the saved theme before the first render to avoid a flash of the default theme.
void applyStartupTheme(loadPreferences().theme).finally(() => {
  createRoot(rootElement).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
});
