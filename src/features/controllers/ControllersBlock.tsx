import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  createController,
  deleteController,
  updateController,
  type AssignmentTarget,
  type Controller,
  type ControllerInput,
  type ControllerKind,
  type ControllersView,
  type DeviceClass,
} from "./controllersApi";
import {
  controllerReferences,
  DEVICE_CLASS_LABELS,
  inUseReferences,
  KIND_LABELS,
  validateName,
} from "./controlsMessages";
import { References } from "./References";

interface BlockProps {
  view: ControllersView;
  onChanged: () => void;
}

/** Settings → Controllers: the logical controllers. */
export function ControllersBlock({ view, onChanged }: BlockProps) {
  const [adding, setAdding] = useState(false);
  return (
    <>
      <h3 className="panel__subtitle">Controllers</h3>
      <p className="message message--muted">
        A controller is the logical identity of one of your devices (name, kind and, optionally, the
        device ID shown by MAME&apos;s Input Devices menu). It is not a controller profile.
      </p>
      <p className="message message--warning" role="note">
        Controller registration is available, but physical device assignments are not yet applied to
        MAME. Device mapping will be supported in a future update.
      </p>
      {adding ? (
        <ControllerForm
          controller={null}
          onSaved={() => {
            setAdding(false);
            onChanged();
          }}
          onCancel={() => {
            setAdding(false);
          }}
        />
      ) : (
        <div className="actions">
          <button
            type="button"
            className="button button--secondary"
            onClick={() => {
              setAdding(true);
            }}
          >
            Add controller…
          </button>
        </div>
      )}
      {view.controllers.length === 0 ? (
        <p className="message message--muted">
          No controller registered. MAME uses the keyboard and the connected devices in the order
          the system reports them.
        </p>
      ) : (
        <ul className="controllers-list" aria-label="Controllers">
          {view.controllers.map((controller) => (
            <ControllerItem
              key={controller.id}
              view={view}
              controller={controller}
              onChanged={onChanged}
            />
          ))}
        </ul>
      )}
    </>
  );
}

interface ControllerItemProps {
  view: ControllersView;
  controller: Controller;
  onChanged: () => void;
}

function ControllerItem({ view, controller, onChanged }: ControllerItemProps) {
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [blockers, setBlockers] = useState<AssignmentTarget[] | null>(null);
  const titleId = useId();
  const references = controllerReferences(view, controller.id);

  async function remove() {
    setRemoving(true);
    setError(null);
    setBlockers(null);
    try {
      await deleteController(controller.id);
      setConfirming(false);
      onChanged();
    } catch (failure) {
      setError(describeError(failure));
      setBlockers(inUseReferences(failure));
    } finally {
      setRemoving(false);
    }
  }

  if (editing) {
    return (
      <li className="controllers-list__item">
        <ControllerForm
          controller={controller}
          onSaved={() => {
            setEditing(false);
            onChanged();
          }}
          onCancel={() => {
            setEditing(false);
          }}
        />
      </li>
    );
  }

  return (
    <li className="controllers-list__item" aria-labelledby={titleId} aria-busy={removing}>
      <p id={titleId} className="installation-list__title">
        {controller.name} <span className="badge">{KIND_LABELS[controller.kind]}</span>
      </p>
      <dl className="details-list">
        <dt>Device class</dt>
        <dd>{DEVICE_CLASS_LABELS[controller.deviceClass]}</dd>
        <dt>Device ID</dt>
        <dd>
          {controller.deviceId ? (
            <span className="code wrap">{controller.deviceId}</span>
          ) : (
            <span className="message--muted">Not set</span>
          )}
        </dd>
        <dt>Provider</dt>
        <dd>
          {controller.provider ? (
            <span className="code">{controller.provider}</span>
          ) : (
            <span className="message--muted">Not set</span>
          )}
        </dd>
      </dl>
      {references.length > 0 && (
        <>
          <p className="message message--muted">
            Stored as a player preference (not applied to MAME yet) in:
          </p>
          <References label={`Assignments using ${controller.name}`} targets={references} />
        </>
      )}
      <div className="actions">
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={removing}
          onClick={() => {
            setEditing(true);
            setConfirming(false);
          }}
        >
          Edit…
        </button>
        <button
          type="button"
          className="button button--secondary button--small"
          aria-expanded={confirming}
          disabled={removing}
          onClick={() => {
            setConfirming(!confirming);
          }}
        >
          Remove…
        </button>
      </div>
      {error && (
        <div role="alert">
          <p className="message message--error">{error}</p>
          {blockers && blockers.length > 0 && (
            <References label="Assignments preventing the removal" targets={blockers} />
          )}
        </div>
      )}
      {confirming &&
        (references.length > 0 ? (
          <div className="confirm-box" role="group" aria-label="Remove controller">
            <p className="message message--warning">
              {controller.name} cannot be removed while saved assignments store it as a player
              preference. Remove it from them first: YMDB does not change them for you.
            </p>
            <References label={`Assignments using ${controller.name}`} targets={references} />
            <div className="actions">
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  setConfirming(false);
                }}
              >
                Close
              </button>
            </div>
          </div>
        ) : (
          <div className="confirm-box" role="group" aria-label="Remove controller">
            <p className="message message--warning">
              Remove the controller {controller.name}? MAME configurations are not modified.
            </p>
            <div className="actions">
              <button
                type="button"
                className="button"
                disabled={removing}
                onClick={() => void remove()}
              >
                {removing ? "Removing…" : "Remove controller"}
              </button>
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  setConfirming(false);
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        ))}
    </li>
  );
}

interface Draft {
  name: string;
  kind: ControllerKind;
  deviceClass: DeviceClass;
  deviceId: string;
  provider: string;
}

type Field = "name" | "deviceId" | "provider";

/** Field checks mirroring the backend (`controllers/model.rs`), which validates again. */
function validateDraft(draft: Draft): Partial<Record<Field, string>> {
  const errors: Partial<Record<Field, string>> = {};
  const name = validateName(draft.name);
  if (name) errors.name = name;
  const deviceId = draft.deviceId.trim();
  if (Array.from(deviceId).length > 512) errors.deviceId = "Use at most 512 characters.";
  // eslint-disable-next-line no-control-regex
  else if (/[\u0000-\u001f\u007f]/.test(deviceId))
    errors.deviceId = "Remove the control characters.";
  const provider = draft.provider.trim();
  if (provider !== "" && !/^[a-z0-9_]{1,32}$/.test(provider))
    errors.provider = "Use the MAME provider name, for example dinput or sdlgame.";
  return errors;
}

interface ControllerFormProps {
  /** `null` to add a controller. */
  controller: Controller | null;
  onSaved: () => void;
  onCancel: () => void;
}

function ControllerForm({ controller, onSaved, onCancel }: ControllerFormProps) {
  const [draft, setDraft] = useState<Draft>({
    name: controller?.name ?? "",
    kind: controller?.kind ?? "gamepad",
    deviceClass: controller?.deviceClass ?? "joystick",
    deviceId: controller?.deviceId ?? "",
    provider: controller?.provider ?? "",
  });
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const id = useId();

  function update<K extends keyof Draft>(field: K, value: Draft[K]) {
    setDraft((current) => ({ ...current, [field]: value }));
    setErrors((current) => ({ ...current, [field]: undefined }));
  }

  async function save() {
    const found = validateDraft(draft);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    const input: ControllerInput = {
      name: draft.name.trim(),
      kind: draft.kind,
      deviceClass: draft.deviceClass,
      deviceId: draft.deviceId.trim() || null,
      provider: draft.provider.trim() || null,
    };
    setSaving(true);
    setSaveError(null);
    try {
      await (controller ? updateController(controller.id, input) : createController(input));
      onSaved();
    } catch (error) {
      setSaveError(describeError(error));
    } finally {
      setSaving(false);
    }
  }

  const text = (field: Field, label: string, hint: string) => (
    <div className="field">
      <label className="field__label" htmlFor={`${id}-${field}`}>
        {label}
      </label>
      <input
        id={`${id}-${field}`}
        className="input"
        value={draft[field]}
        aria-invalid={errors[field] !== undefined}
        aria-describedby={errors[field] ? `${id}-${field}-error` : undefined}
        onChange={(event) => {
          update(field, event.target.value);
        }}
      />
      <span className="message message--muted">{hint}</span>
      {errors[field] && (
        <span id={`${id}-${field}-error`} className="message message--error">
          {errors[field]}
        </span>
      )}
    </div>
  );

  return (
    <form
      className="inline-form"
      aria-label={controller ? `Edit ${controller.name}` : "New controller"}
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      {text("name", "Name", "For example Arcade Stick Player 1.")}
      <div className="field">
        <label className="field__label" htmlFor={`${id}-kind`}>
          Kind
        </label>
        <select
          id={`${id}-kind`}
          className="input"
          value={draft.kind}
          onChange={(event) => {
            update("kind", event.target.value as ControllerKind);
          }}
        >
          {Object.entries(KIND_LABELS).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </div>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-class`}>
          Device class
        </label>
        <select
          id={`${id}-class`}
          className="input"
          value={draft.deviceClass}
          onChange={(event) => {
            update("deviceClass", event.target.value as DeviceClass);
          }}
        >
          {Object.entries(DEVICE_CLASS_LABELS).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <span className="message message--muted">How MAME numbers the device&apos;s inputs.</span>
      </div>
      {text(
        "deviceId",
        "Device ID (optional)",
        "As copied from MAME: Input Settings → Input Devices → Copy Device ID.",
      )}
      {text(
        "provider",
        "Provider (optional)",
        "The MAME input provider the device ID was obtained with, for example dinput.",
      )}
      {saveError && (
        <p className="message message--error" role="alert">
          {saveError}
        </p>
      )}
      <div className="actions">
        <button type="submit" className="button" disabled={saving}>
          {saving ? "Saving…" : "Save controller"}
        </button>
        <button type="button" className="button button--secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
