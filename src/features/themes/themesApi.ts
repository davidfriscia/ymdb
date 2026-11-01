import { invokeCommand } from "../../shared/ipc";

// These types mirror `src-tauri/src/custom_themes.rs` and
// `src-tauri/src/commands/custom_themes.rs`. Keep both sides in sync.

/**
 * A stored custom theme. `tokens` is kept verbatim by the backend; its format
 * is defined by `src/app/theme/themeTokens.ts` and versioned by
 * `tokensVersion`.
 */
export interface CustomTheme {
  id: string;
  name: string;
  /** The theme it was cloned from, for information only. */
  sourceThemeId?: string;
  tokensVersion: number;
  tokens: unknown;
  /** Milliseconds since the Unix epoch. */
  createdAt: number;
  updatedAt: number;
}

/** Mirrors `CustomThemeCreated`. */
export interface CustomThemeCreated {
  id: string;
  themes: CustomTheme[];
}

/** Mirrors `ThemeFile`: a chosen file, checked for JSON and size only. */
export interface ThemeFile {
  fileName: string;
  document: unknown;
}

export function getCustomThemes(): Promise<CustomTheme[]> {
  return invokeCommand("get_custom_themes");
}

export function createCustomTheme(
  name: string,
  sourceThemeId: string | null,
  tokensVersion: number,
  tokens: unknown,
): Promise<CustomThemeCreated> {
  return invokeCommand("create_custom_theme", { name, sourceThemeId, tokensVersion, tokens });
}

export function updateCustomTheme(
  id: string,
  name: string,
  tokensVersion: number,
  tokens: unknown,
): Promise<CustomTheme[]> {
  return invokeCommand("update_custom_theme", { id, name, tokensVersion, tokens });
}

export function renameCustomTheme(id: string, name: string): Promise<CustomTheme[]> {
  return invokeCommand("rename_custom_theme", { id, name });
}

export function deleteCustomTheme(id: string): Promise<CustomTheme[]> {
  return invokeCommand("delete_custom_theme", { id });
}

/** Opens the native file dialog; `null` when the user cancels. */
export function chooseThemeFile(): Promise<ThemeFile | null> {
  return invokeCommand("choose_theme_file");
}

/** Saves a theme document with the native save dialog; `null` when the user cancels. */
export function exportThemeFile(name: string, document: unknown): Promise<string | null> {
  return invokeCommand("export_theme_file", { name, document });
}
