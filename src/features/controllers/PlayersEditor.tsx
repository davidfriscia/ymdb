import { useId, useState } from "react";
import type { Controller, ControllersView, Players } from "./controllersApi";
import {
  BINDING_LABELS,
  MAX_PLAYERS,
  mappingProblem,
  PORT_ORDER_WARNING,
  trimPlayers,
} from "./controlsMessages";

interface PlayersEditorProps {
  view: ControllersView;
  value: Players;
  onChange: (players: Players) => void;
  /** Names the group, for example "Players for This machine (sf2)". */
  label: string;
  disabled?: boolean;
}

/**
 * Chooses a controller for each player. Only controllers with a configured
 * device can be chosen, and each controller for one player at most; the
 * backend validates again and resolves nothing here.
 */
export function PlayersEditor({
  view,
  value,
  onChange,
  label,
  disabled = false,
}: PlayersEditorProps) {
  const [rows, setRows] = useState(() => Math.max(value.length, 2));
  const id = useId();
  const count = Math.min(Math.max(rows, value.length), MAX_PLAYERS);
  const players = Array.from({ length: count }, (_, index) => value[index] ?? null);

  function choose(index: number, controllerId: string | null) {
    const next = [...players];
    next[index] = controllerId;
    onChange(trimPlayers(next));
  }

  return (
    <fieldset className="players-editor" aria-label={label} disabled={disabled}>
      <legend className="field__label">Players</legend>
      {players.map((selected, index) => {
        const player = index + 1;
        const controller = view.controllers.find((entry) => entry.id === selected);
        return (
          <div key={player} className="players-editor__row">
            <label className="players-editor__label" htmlFor={`${id}-${String(player)}`}>
              Player {player}
            </label>
            <select
              id={`${id}-${String(player)}`}
              className="input"
              value={selected ?? ""}
              onChange={(event) => {
                choose(index, event.target.value === "" ? null : event.target.value);
              }}
            >
              <option value="">No controller</option>
              {view.controllers.map((entry) => {
                const problem = mappingProblem(entry);
                const other = players.findIndex(
                  (assigned, at) => assigned === entry.id && at !== index,
                );
                return (
                  <option
                    key={entry.id}
                    value={entry.id}
                    disabled={entry.id !== selected && (problem !== null || other !== -1)}
                  >
                    {entry.name}
                    {problem !== null
                      ? " (device not configured)"
                      : other !== -1
                        ? ` (Player ${String(other + 1)})`
                        : ""}
                  </option>
                );
              })}
              {selected !== null && !controller && (
                <option value={selected}>{selected} (missing)</option>
              )}
            </select>
            {controller && <ControllerDevice controller={controller} />}
            {selected !== null && !controller && (
              <span className="message message--error">This controller no longer exists.</span>
            )}
          </div>
        );
      })}
      {count < MAX_PLAYERS && (
        <button
          type="button"
          className="button button--secondary button--small"
          onClick={() => {
            setRows(count + 1);
          }}
        >
          Add player
        </button>
      )}
    </fieldset>
  );
}

/** The device a controller is mapped with, or why it cannot be. */
export function ControllerDevice({ controller }: { controller: Controller }) {
  const problem = mappingProblem(controller);
  if (problem !== null) {
    return <span className="message message--error">{problem}</span>;
  }
  return (
    <span className="message message--muted players-editor__device">
      {controller.binding && BINDING_LABELS[controller.binding]} ·{" "}
      <span className="code">{controller.provider}</span>
      {controller.binding === "portOrder" && (
        <span className="message message--warning"> {PORT_ORDER_WARNING}</span>
      )}
    </span>
  );
}
