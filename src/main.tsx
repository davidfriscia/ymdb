import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { loadPreferences } from "./app/preferences";
import { applyTheme } from "./app/theme/themes";
import "./app/theme/themes.css";
import "./app/App.css";
import "./features/browser/browser.css";
import "./features/launcher/launcher.css";
import "./features/remote/remote.css";
import "./features/roms/roms.css";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found");
}

// Apply the saved theme before the first render to avoid a flash of the default theme.
applyTheme(loadPreferences().theme);

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
