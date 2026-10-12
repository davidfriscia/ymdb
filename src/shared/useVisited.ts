import { useState } from "react";

/**
 * The keys of the panels shown so far, including `current`. Pages keep these
 * panels mounted (hidden when not selected), so that drafts and other state
 * survive a change of selection.
 */
export function useVisited(current: string): ReadonlySet<string> {
  const [visited, setVisited] = useState<ReadonlySet<string>>(() => new Set([current]));
  if (visited.has(current)) return visited;
  const next = new Set(visited).add(current);
  setVisited(next);
  return next;
}
