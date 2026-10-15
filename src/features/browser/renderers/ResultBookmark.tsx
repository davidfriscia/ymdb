import { FavoriteBookmark } from "../../lists/FavoriteBookmark";
import type { ListEntry } from "../../lists/listsApi";
import type { ResultItem } from "./types";

function entryOf(item: ResultItem): ListEntry {
  return item.kind === "machine"
    ? { kind: "machine", name: item.machine.name }
    : { kind: "software", listName: item.software.listName, name: item.software.name };
}

/**
 * The favorite bookmark of a result, in the top right corner of its card.
 * Renderers place it next to (never inside) the button that opens the item.
 */
export function ResultBookmark({ item }: { item: ResultItem }) {
  const title = item.kind === "machine" ? item.machine.description : item.software.description;
  return <FavoriteBookmark entry={entryOf(item)} title={title} variant="overlay" />;
}
