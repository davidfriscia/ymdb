import type { AssignmentTarget } from "./controllersApi";
import { describeTarget } from "./controlsMessages";

/** The saved assignments that use a controller profile or a controller. */
export function References({ label, targets }: { label: string; targets: AssignmentTarget[] }) {
  return (
    <ul className="controllers-references" aria-label={label}>
      {targets.map((target) => (
        <li key={`${target.level}:${target.key ?? ""}`} className="code">
          {describeTarget(target)}
        </li>
      ))}
    </ul>
  );
}
