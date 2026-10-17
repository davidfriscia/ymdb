import { useState } from "react";
import { formatCount } from "../../catalog/catalogMessages";
import { nodeMatches as matches, type CategoryNode } from "./facetValues";

interface CategoryTreeProps {
  label: string;
  roots: CategoryNode[];
  selected: (string | null)[];
  onToggle: (path: string) => void;
  /** Shows the nodes whose path contains the text, with their ancestors. */
  search: string;
  scroll?: boolean;
}

export function CategoryTree({
  label,
  roots,
  selected,
  onToggle,
  search,
  scroll = true,
}: CategoryTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const needle = search.trim().toLowerCase();
  const toggleExpanded = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };
  const visible = roots.filter((node) => needle === "" || matches(node, needle));

  function renderNodes(nodes: CategoryNode[], includedBy: string | null) {
    return nodes
      .filter((node) => needle === "" || matches(node, needle))
      .map((node) => {
        const checked = selected.includes(node.path);
        const included = includedBy !== null;
        // While searching, every matching branch is open.
        const open = node.children.length > 0 && (needle !== "" || expanded.has(node.path));
        return (
          <li key={node.path}>
            <div className="facet-tree__row">
              {node.children.length > 0 ? (
                <button
                  type="button"
                  className="facet-tree__toggle"
                  aria-expanded={open}
                  aria-label={`${open ? "Collapse" : "Expand"} ${node.path}`}
                  onClick={() => {
                    toggleExpanded(node.path);
                  }}
                >
                  <span aria-hidden="true">{open ? "▾" : "▸"}</span>
                </button>
              ) : (
                <span className="facet-tree__spacer" aria-hidden="true" />
              )}
              <label
                className="facet__value"
                title={includedBy !== null ? `Included in ${includedBy}` : node.path}
              >
                <input
                  type="checkbox"
                  checked={checked || included}
                  disabled={included}
                  aria-label={node.path}
                  onChange={() => {
                    onToggle(node.path);
                  }}
                />
                <span className="facet__label">{node.label}</span>
                <span className="facet__count">
                  {node.count === null ? "—" : formatCount(node.count)}
                </span>
              </label>
            </div>
            {open && (
              <ul className="facet-tree__children">
                {renderNodes(node.children, includedBy ?? (checked ? node.path : null))}
              </ul>
            )}
          </li>
        );
      });
  }

  if (visible.length === 0) {
    return <p className="facet__empty">{needle === "" ? "No values" : "No matching values"}</p>;
  }
  return (
    <ul
      className={scroll ? "facet-tree facet__values--scroll" : "facet-tree"}
      aria-label={`${label} values`}
    >
      {renderNodes(visible, null)}
    </ul>
  );
}
