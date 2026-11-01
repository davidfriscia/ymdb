/** Mirrors `MAX_NAME_LENGTH` in `src-tauri/src/custom_themes.rs`. */
export const MAX_THEME_NAME_LENGTH = 60;

/** Names are unique ignoring case and surrounding spaces (as in the backend). */
export function themeNameKey(name: string): string {
  return name.trim().toLowerCase();
}

/**
 * The problem with a theme name, or `null` when it can be saved. Built-in
 * names are taken too, so that every theme of the list is distinguishable.
 */
export function themeNameError(name: string, takenNames: readonly string[]): string | null {
  const trimmed = name.trim();
  const length = Array.from(trimmed).length;
  if (length === 0) return "Enter a name.";
  if (length > MAX_THEME_NAME_LENGTH) {
    return `A theme name can have at most ${String(MAX_THEME_NAME_LENGTH)} characters.`;
  }
  if (/\p{Cc}/u.test(trimmed)) return "A theme name cannot contain control characters.";
  const key = themeNameKey(trimmed);
  if (takenNames.some((taken) => themeNameKey(taken) === key)) {
    return `A theme named “${trimmed}” already exists. Choose another name.`;
  }
  return null;
}

/** `base`, or `base 2`, `base 3`… : the first name not taken. */
export function uniqueThemeName(base: string, takenNames: readonly string[]): string {
  const trimmed =
    Array.from(base.trim())
      .slice(0, MAX_THEME_NAME_LENGTH - 3)
      .join("")
      .trim() || "Theme";
  const taken = new Set(takenNames.map(themeNameKey));
  if (!taken.has(themeNameKey(trimmed))) return trimmed;
  for (let index = 2; ; index += 1) {
    const candidate = `${trimmed} ${String(index)}`;
    if (!taken.has(themeNameKey(candidate))) return candidate;
  }
}
