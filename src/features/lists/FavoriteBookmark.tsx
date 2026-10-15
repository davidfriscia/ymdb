import { BookmarkIcon } from "../../shared/icons";
import { useFavorites } from "./favoritesContext";
import type { ListEntry } from "./listsApi";

interface FavoriteBookmarkProps {
  entry: ListEntry;
  /** Title of the machine or software, for the accessible name. */
  title: string;
  /** `overlay`: corner of a result card; `inline`: next to a detail title. */
  variant: "overlay" | "inline";
}

/**
 * Adds a machine or software to Favorites or removes it. The same control,
 * state and persistence for every renderer and the detail pages. It is never
 * placed inside another button: a click on it does not open the card.
 */
export function FavoriteBookmark({ entry, title, variant }: FavoriteBookmarkProps) {
  const favorites = useFavorites();
  const favorite = favorites.isFavorite(entry);
  const pending = favorites.isPending(entry);
  const unavailable = !favorites.ready;
  const tooltip = unavailable
    ? favorites.loadError
      ? "Favorites are unavailable"
      : "Loading favorites…"
    : favorite
      ? "Remove from favorites"
      : "Add to favorites";
  return (
    <button
      type="button"
      className={`bookmark bookmark--${variant}`}
      aria-pressed={favorite}
      aria-label={`Favorite: ${title}`}
      // While saving, clicks are ignored but the focus stays on the control.
      aria-busy={pending || undefined}
      aria-disabled={pending || undefined}
      title={tooltip}
      disabled={unavailable}
      onClick={(event) => {
        event.stopPropagation();
        favorites.toggle(entry);
      }}
    >
      <BookmarkIcon filled={favorite} size={variant === "overlay" ? 16 : 18} />
    </button>
  );
}
