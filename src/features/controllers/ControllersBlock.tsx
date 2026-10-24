import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  createController,
  deleteController,
  updateController,
  type AssignmentTarget,
  type Controller,
  type ControllerBinding,
  type ControllerInput,
  type ControllerKind,
  type ControllersView,
  type DetectedDevice,
  type DeviceClass,
  type JoystickProvider,
} from "./controllersApi";
import {
  BINDING_DESCRIPTIONS,
  BINDING_LABELS,
  controllerReferences,
  DEVICE_CLASS_LABELS,
  inUseReferences,
  KIND_LABELS,
  mappingProblem,
  PORT_ORDER_WARNING,
  validateName,
} from "./controlsMessages";
import { DeviceDetectionPanel } from "./DeviceDetection";
import { References } from "./References";

interface BlockProps {
  view: ControllersView;
  onChanged: () => void;
}

/** Settings → Controllers: device detection and the logical controllers. */
export function ControllersBlock({ view, onChanged }: BlockProps) {
  const [adding, setAdding] = useState(false);
  // The detected device being registered, if any.
  const [registering, setRegistering] = useState<DetectedDevice | null>(null);
  const formOpen = adding || registering !== null;
  return (
    <>
      <h3 className="panel__subtitle">Controllers</h3>
      <p className="message message--muted">
        A controller is the logical identity of one of your devices. A joystick or gamepad with a
        device ID, a provider and a binding can be assigned to players: each launch then maps it to
        its player&apos;s joystick in MAME. It is not a controller profile.
      </p>
      <h4 className="controllers-settings__heading">Detect devices</h4>
      <DeviceDetectionPanel
        view={view}
        onRegister={(device) => {
          setAdding(false);
          setRegistering(device);
        }}
      />
      <h4 className="controllers-settings__heading">Registered controllers</h4>
      {formOpen ? (
        <ControllerForm
          // A new detected device starts a new form.
          key={registering ? `${registering.provider}:${registering.deviceId}` : "new"}
          controller={null}
          providers={view.providers}
          initial={registering ? draftFromDevice(registering) : undefined}
          onSaved={() => {
            setAdding(false);
            setRegistering(null);
            onChanged();
          }}
          onCancel={() => {
            setAdding(false);
            setRegistering(null);
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
  const problem = mappingProblem(controller);

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
          providers={view.providers}
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
        <dt>Binding</dt>
        <dd>
          {controller.binding ? (
            BINDING_LABELS[controller.binding]
          ) : (
            <span className="message--muted">Not set</span>
          )}
        </dd>
      </dl>
      {controller.binding === "portOrder" && (
        <p className="message message--warning">{PORT_ORDER_WARNING}</p>
      )}
      {problem !== null && (
        <p className="message message--muted">
          {problem} Until then it cannot be mapped to a player.
        </p>
      )}
      {references.length > 0 && (
        <>
          <p className="message message--muted">Assigned to players in:</p>
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
              {controller.name} cannot be removed while saved assignments assign it to a player.
              Remove it from them first: YMDB does not change them for you.
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
  /** `""` when not set. */
  binding: ControllerBinding | "";
}

type Field = "name" | "deviceId" | "provider" | "binding";

/** The registration form of a detected device. */
function draftFromDevice(device: DetectedDevice): Draft {
  return {
    name: device.name.slice(0, 60),
    kind: "gamepad",
    deviceClass: device.deviceClass,
    deviceId: device.deviceId,
    provider: device.provider,
    binding: device.suggestedBinding ?? "",
  };
}

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
    errors.provider = "Use the MAME provider name, for example winhybrid or dinput.";
  if (draft.binding !== "") {
    if (draft.deviceClass !== "joystick")
      errors.binding = "Only joysticks and gamepads can be mapped to players.";
    else if (deviceId === "") errors.deviceId = "Enter the device ID reported by MAME.";
    else if (provider === "") errors.provider = "Choose the provider the device ID comes from.";
    else if (draft.binding === "device" && /^XInput Player \d+$/.test(deviceId))
      errors.binding =
        "An XInput slot follows the order the controllers were connected in: use the port/order binding.";
  }
  return errors;
}

interface ControllerFormProps {
  /** `null` to add a controller. */
  controller: Controller | null;
  /** The joystick providers of this operating system. */
  providers: JoystickProvider[];
  /** The values of a new controller, for example from a detected device. */
  initial?: Draft;
  onSaved: () => void;
  onCancel: () => void;
}

function ControllerForm({
  controller,
  providers,
  initial,
  onSaved,
  onCancel,
}: ControllerFormProps) {
  const [draft, setDraft] = useState<Draft>(
    () =>
      initial ?? {
        name: controller?.name ?? "",
        kind: controller?.kind ?? "gamepad",
        deviceClass: controller?.deviceClass ?? "joystick",
        deviceId: controller?.deviceId ?? "",
        provider: controller?.provider ?? "",
        binding: controller?.binding ?? "",
      },
  );
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const id = useId();
  // A stored provider of another system stays selectable.
  const providerChoices: string[] =
    draft.provider !== "" && !providers.includes(draft.provider as JoystickProvider)
      ? [...providers, draft.provider]
      : providers;

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
      binding: draft.binding === "" ? null : draft.binding,
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

  const fieldError = (field: Field) =>
    errors[field] && (
      <span id={`${id}-${field}-error`} className="message message--error">
        {errors[field]}
      </span>
    );

  return (
    <form
      className="inline-form"
      aria-label={
        controller ? `Edit ${controller.name}` : initial ? "Register device" : "New controller"
      }
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <div className="field">
        <label className="field__label" htmlFor={`${id}-name`}>
          Name
        </label>
        <input
          id={`${id}-name`}
          className="input"
          value={draft.name}
          aria-invalid={errors.name !== undefined}
          aria-describedby={errors.name ? `${id}-name-error` : undefined}
          onChange={(event) => {
            update("name", event.target.value);
          }}
        />
        <span className="message message--muted">For example Arcade Stick Player 1.</span>
        {fieldError("name")}
      </div>
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
      <div className="field">
        <label className="field__label" htmlFor={`${id}-provider`}>
          Provider
        </label>
        <select
          id={`${id}-provider`}
          className="input"
          value={draft.provider}
          aria-invalid={errors.provider !== undefined}
          aria-describedby={errors.provider ? `${id}-provider-error` : undefined}
          onChange={(event) => {
            update("provider", event.target.value);
          }}
        >
          <option value="">Not set</option>
          {providerChoices.map((provider) => (
            <option key={provider} value={provider}>
              {provider}
            </option>
          ))}
        </select>
        <span className="message message--muted">
          The MAME joystick provider the device ID was detected with: device IDs differ between
          providers.
        </span>
        {fieldError("provider")}
      </div>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-deviceId`}>
          Device ID
        </label>
        <input
          id={`${id}-deviceId`}
          className="input"
          value={draft.deviceId}
          aria-invalid={errors.deviceId !== undefined}
          aria-describedby={errors.deviceId ? `${id}-deviceId-error` : undefined}
          onChange={(event) => {
            update("deviceId", event.target.value);
          }}
        />
        <span className="message message--muted">
          The complete ID reported by MAME, best obtained with Detect devices.
        </span>
        {fieldError("deviceId")}
      </div>
      <fieldset
        className="choice-group"
        aria-describedby={errors.binding ? `${id}-binding-error` : undefined}
      >
        <legend>Binding</legend>
        {(["", "device", "portOrder"] as const).map((binding) => (
          <label key={binding} className="choice">
            <input
              type="radio"
              name={`${id}-binding`}
              checked={draft.binding === binding}
              onChange={() => {
                update("binding", binding);
              }}
            />
            {binding === "" ? "Not set: not mapped to players" : BINDING_DESCRIPTIONS[binding]}
          </label>
        ))}
        {fieldError("binding")}
      </fieldset>
      {draft.binding === "portOrder" && (
        <p className="message message--warning">{PORT_ORDER_WARNING}</p>
      )}
      {draft.binding === "device" && (
        <p className="message message--muted">
          Choose the device binding only if the device ID stays the same when the controller is
          connected to another USB port (detect again to check). Identical controllers without their
          own serial number are told apart only by their port.
        </p>
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
