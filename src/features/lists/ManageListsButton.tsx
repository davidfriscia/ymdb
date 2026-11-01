import { useState } from "react";
import { ListsIcon } from "../../shared/icons";
import type { ListEntry } from "./listsApi";
import { ManageListsModal } from "./ManageListsModal";

interface ManageListsButtonProps {
  entry: ListEntry;
  /** Title of the machine, for the accessible name and the window. */
  title: string;
  /** `overlay`: corner of a result card; `inline`: next to a detail title. */
  variant: "overlay" | "inline";
}

/**
 * Opens Manage Lists for one machine: the same control and window for every
 * renderer and the detail page. Like the favorite bookmark, it is never
 * placed inside the button that opens a card, and its click goes no further.
 */
export function ManageListsButton({ entry, title, variant }: ManageListsButtonProps) {
  // The entry when the window opened: renderers build a new entry object on
  // every render, and the window must not read the memberships again for it.
  const [opened, setOpened] = useState<ListEntry | null>(null);
  return (
    <>
      <button
        type="button"
        className={`bookmark list-button list-button--${variant}`}
        aria-label={`Manage lists: ${title}`}
        aria-haspopup="dialog"
        title="Manage lists"
        onClick={(event) => {
          event.stopPropagation();
          setOpened(entry);
        }}
      >
        <ListsIcon size={variant === "overlay" ? 16 : 18} />
      </button>
      {opened && (
        <ManageListsModal
          entry={opened}
          title={title}
          onClose={() => {
            setOpened(null);
          }}
        />
      )}
    </>
  );
}
