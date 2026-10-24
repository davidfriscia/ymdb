import { useContext, useId, useRef, useState } from "react";
import { describeError } from "../../shared/ipc";
import { InstallationsContext } from "../installations/installationsContext";
import {
  detectControllerDevices,
  type ControllersView,
  type DetectedDevice,
  type DeviceDetection,
  type JoystickProvider,
} from "./controllersApi";
import { BINDING_LABELS, PORT_ORDER_WARNING, registeredFor } from "./controlsMessages";

interface DeviceDetectionPanelProps {
  view: ControllersView;
  /** Opens the registration form of a detected device. */
  onRegister: (device: DetectedDevice) => void;
}

/**
 * Settings → Controllers: detects the joysticks connected now, on explicit
 * request only, with the MAME of an installation. Nothing is registered
 * automatically: each device is registered by the user, and devices that
 * MAME cannot tell apart cannot be registered at all.
 */
export function DeviceDetectionPanel({ view, onRegister }: DeviceDetectionPanelProps) {
  const installations = useContext(InstallationsContext);
  const [installationId, setInstallationId] = useState<string | null>(null);
  const [provider, setProvider] = useState<JoystickProvider | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [result, setResult] = useState<DeviceDetection | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the answer of the latest request is shown.
  const request = useRef(0);
  const id = useId();
  const chosenInstallation = installationId ?? installations?.activeId ?? null;
  const chosenProvider = provider ?? view.providers[0] ?? null;

  async function detect() {
    if (chosenInstallation === null) return;
    const current = ++request.current;
    setDetecting(true);
    setError(null);
    try {
      const detection = await detectControllerDevices(chosenInstallation, chosenProvider);
      if (current !== request.current) return;
      setResult(detection);
    } catch (failure) {
      if (current !== request.current) return;
      // A failed detection never shows an empty list.
      setResult(null);
      setError(describeError(failure));
    } finally {
      if (current === request.current) setDetecting(false);
    }
  }

  return (
    <div className="device-detection" role="group" aria-label="Device detection">
      <p className="message message--muted">
        YMDB asks MAME which joysticks are connected now, only when you click Detect devices. MAME
        starts without a window for a few seconds and changes nothing.
      </p>
      <div className="launch-panel__row">
        {installations && installations.installations.length > 1 && (
          <label className="field">
            <span className="field__label">MAME installation</span>
            <select
              className="input"
              value={chosenInstallation ?? ""}
              disabled={detecting}
              onChange={(event) => {
                setInstallationId(event.target.value);
                setResult(null);
              }}
            >
              {installations.installations.map((installation) => (
                <option key={installation.id} value={installation.id}>
                  {installations.labels.get(installation.id) ?? installation.id}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="field">
          <span className="field__label" id={`${id}-provider`}>
            Joystick provider
          </span>
          <select
            className="input"
            aria-labelledby={`${id}-provider`}
            value={chosenProvider ?? ""}
            disabled={detecting}
            onChange={(event) => {
              setProvider(event.target.value as JoystickProvider);
              setResult(null);
            }}
          >
            {view.providers.map((entry, index) => (
              <option key={entry} value={entry}>
                {entry}
                {index === 0 ? " (MAME's default)" : ""}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="button button--secondary"
          disabled={detecting || chosenInstallation === null || chosenProvider === null}
          aria-busy={detecting || undefined}
          onClick={() => void detect()}
        >
          {detecting ? "Detecting…" : "Detect devices"}
        </button>
      </div>
      {chosenInstallation === null && (
        <p className="message message--muted">Register a MAME installation to detect devices.</p>
      )}
      {detecting && (
        <p className="message message--muted" role="status">
          MAME is listing the connected devices…
        </p>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      {result && (
        <>
          {result.warnings.map((warning) => (
            <p key={warning} className="message message--warning">
              {warning}
            </p>
          ))}
          {result.devices.length > 0 && (
            <ul className="controllers-list" aria-label="Detected devices">
              {result.devices.map((device) => (
                <DetectedDeviceItem
                  key={`${device.provider}:${String(device.index)}`}
                  view={view}
                  device={device}
                  onRegister={onRegister}
                />
              ))}
            </ul>
          )}
          <p className="message message--muted">
            Device IDs depend on the provider: a controller registered with one provider is mapped
            with that provider. Detected devices are not shown as connected afterwards: detect again
            to see the current devices.
          </p>
        </>
      )}
    </div>
  );
}

function DetectedDeviceItem({
  view,
  device,
  onRegister,
}: {
  view: ControllersView;
  device: DetectedDevice;
  onRegister: (device: DetectedDevice) => void;
}) {
  const titleId = useId();
  const registered = registeredFor(view, device);
  return (
    <li className="controllers-list__item" aria-labelledby={titleId}>
      <p id={titleId} className="installation-list__title">
        {device.name} <span className="badge">Joystick #{device.index}</span>{" "}
        {!device.distinguishable && <span className="badge badge--error">Not distinguishable</span>}
      </p>
      <dl className="details-list">
        <dt>Provider</dt>
        <dd className="code">{device.provider}</dd>
        <dt>Device ID</dt>
        <dd className="code wrap">{device.deviceId}</dd>
        {device.vendorId !== null && device.productId !== null && (
          <>
            <dt>USB ids</dt>
            <dd className="code">
              VID {device.vendorId} · PID {device.productId}
            </dd>
          </>
        )}
        {device.serial !== null && (
          <>
            <dt>Serial number</dt>
            <dd className="code">{device.serial}</dd>
          </>
        )}
        <dt>Supported binding</dt>
        <dd>{device.allowedBindings.map((binding) => BINDING_LABELS[binding]).join(", ")}</dd>
      </dl>
      {device.notes.map((note) => (
        <p key={note} className="message message--muted">
          {note}
        </p>
      ))}
      {device.distinguishable && device.allowedBindings.length === 1 && (
        <p className="message message--warning">{PORT_ORDER_WARNING}</p>
      )}
      {registered.length > 0 ? (
        <p className="message message--success">
          Registered as {registered.map((controller) => controller.name).join(", ")}.
        </p>
      ) : (
        <div className="actions">
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={!device.distinguishable}
            title={
              device.distinguishable
                ? undefined
                : "MAME cannot tell this device apart from another one with this provider"
            }
            onClick={() => {
              onRegister(device);
            }}
          >
            Register…
          </button>
        </div>
      )}
    </li>
  );
}
