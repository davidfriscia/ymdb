import { FavoriteBookmark } from "../../lists/FavoriteBookmark";
import type { ListEntry } from "../../lists/listsApi";
import { ManageListsButton } from "../../lists/ManageListsButton";
import type { ResultItem } from "./types";

function entryOf(item: ResultItem): ListEntry {
  return item.kind === "machine"
    ? { kind: "machine", name: item.machine.name }
    : { kind: "software", listName: item.software.listName, name: item.software.name };
}

/**
 * The favorite bookmark of a result, in the top right corner of its card,
 * and for machines the Manage Lists button next to it. Renderers place them
 * next to (never inside) the button that opens the item.
 */
export function ResultBookmark({ item }: { item: ResultItem }) {
  const title = item.kind === "machine" ? item.machine.description : item.software.description;
  const entry = entryOf(item);
  return (
    <>
      {/* Custom lists hold machines only in this version. */}
      {entry.kind === "machine" && (
        <ManageListsButton entry={entry} title={title} variant="overlay" />
      )}
      <FavoriteBookmark entry={entry} title={title} variant="overlay" />
    </>
  );
}
