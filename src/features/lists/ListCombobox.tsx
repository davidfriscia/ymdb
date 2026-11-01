import { useId, useMemo, useState, type KeyboardEvent } from "react";
import type { ListSummary } from "./listsApi";

interface ListComboboxProps {
  /** The selected list id; `null` is "All items". */
  value: string | null;
  /** The custom lists; `null` while they load. */
  lists: readonly ListSummary[] | null;
  onChange: (listId: string | null) => void;
  /** Accessible name of the field. */
  label: string;
  disabled?: boolean;
}

/** Options rendered at once; typing finds the others. */
const SHOWN_OPTIONS = 200;
export const ALL_ITEMS_LABEL = "All items";
export const DELETED_LIST_LABEL = "Deleted list";

interface Option {
  id: string | null;
  label: string;
  count?: number;
}

/**
 * A searchable list picker (ARIA combobox): typing filters the lists by
 * name, the arrow keys move through the options, Enter chooses and Escape
 * closes. "All items" is always the first option. Hundreds of lists stay
 * usable: only the first matches are rendered and the list scrolls.
 */
export function ListCombobox({ value, lists, onChange, label, disabled }: ListComboboxProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const sorted = useMemo(
    () =>
      [...(lists ?? [])].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
      ),
    [lists],
  );
  const selected = value === null ? null : sorted.find((list) => list.id === value);
  const selectedLabel =
    value === null
      ? ALL_ITEMS_LABEL
      : selected
        ? selected.name
        : lists === null
          ? "Loading…"
          : DELETED_LIST_LABEL;

  const text = query.trim().toLowerCase();
  const matching =
    text === "" ? sorted : sorted.filter((list) => list.name.toLowerCase().includes(text));
  const options: Option[] = [
    { id: null, label: ALL_ITEMS_LABEL },
    ...matching
      .slice(0, SHOWN_OPTIONS)
      .map((list) => ({ id: list.id, label: list.name, count: list.machines })),
  ];
  const hidden = matching.length - (options.length - 1);
  const activeIndex = Math.min(active, options.length - 1);

  const choose = (option: Option | undefined) => {
    if (!option) return;
    setOpen(false);
    setQuery("");
    if (option.id !== value) onChange(option.id);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        setOpen(true);
        return;
      }
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive(Math.max(0, Math.min(options.length - 1, activeIndex + step)));
    } else if (event.key === "Enter" && open) {
      event.preventDefault();
      choose(options[activeIndex]);
    } else if (event.key === "Escape" && open) {
      // Closes the options only, not the panel or a window around it.
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      setQuery("");
    }
  };

  return (
    <div className="list-combobox">
      <input
        id={`${id}-input`}
        className="input list-combobox__input"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-controls={`${id}-options`}
        aria-autocomplete="list"
        aria-activedescendant={open ? `${id}-option-${String(activeIndex)}` : undefined}
        disabled={disabled}
        value={open ? query : selectedLabel}
        placeholder={open ? selectedLabel : undefined}
        onFocus={() => {
          setOpen(true);
          setQuery("");
          setActive(0);
        }}
        // Clicking the focused field after a choice opens the options again.
        onClick={() => {
          setOpen(true);
        }}
        onBlur={() => {
          setOpen(false);
          setQuery("");
        }}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      {open && (
        <ul
          id={`${id}-options`}
          className="list-combobox__options"
          role="listbox"
          aria-label={label}
        >
          {options.map((option, index) => (
            <li
              key={option.id ?? ""}
              id={`${id}-option-${String(index)}`}
              role="option"
              aria-selected={option.id === value}
              className={`list-combobox__option${index === activeIndex ? " list-combobox__option--active" : ""}`}
              // Before the input loses the focus.
              onMouseDown={(event) => {
                event.preventDefault();
                choose(option);
              }}
              onMouseEnter={() => {
                setActive(index);
              }}
            >
              {option.label}
              {option.count !== undefined && (
                <span className="list-combobox__count">{option.count}</span>
              )}
            </li>
          ))}
          {matching.length === 0 && text !== "" && (
            <li className="list-combobox__option message message--muted" role="presentation">
              No list matches “{query.trim()}”.
            </li>
          )}
          {hidden > 0 && (
            <li className="list-combobox__option message message--muted" role="presentation">
              {hidden} more: type to narrow the list.
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
