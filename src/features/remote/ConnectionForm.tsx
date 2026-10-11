import { useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  pickPrivateKeyFile,
  saveRemoteConnection,
  type ConnectionInput,
  type ConnectionSummary,
  type HostKeyVerification,
} from "./remoteApi";

interface Draft {
  name: string;
  host: string;
  port: string;
  username: string;
  privateKeyPath: string;
  hostKeyVerification: HostKeyVerification;
}

type Field = keyof Draft;

/** Field checks mirroring the backend, which validates again when saving. */
function validateDraft(draft: Draft): Partial<Record<Field, string>> {
  const errors: Partial<Record<Field, string>> = {};
  const name = draft.name.trim();
  if (name === "") errors.name = "Enter a name.";
  else if (name.length > 60) errors.name = "Use at most 60 characters.";
  const host = draft.host.trim();
  if (host === "") errors.host = "Enter a host name or IP address.";
  else if (!/^[A-Za-z0-9._:%[\]-]+$/.test(host) || host.startsWith("-"))
    errors.host = "Enter only the host name or IP address, for example nas.local.";
  const port = Number(draft.port);
  if (!/^\d+$/.test(draft.port.trim()) || port < 1 || port > 65535)
    errors.port = "Enter a port from 1 to 65535.";
  if (draft.username.trim() === "") errors.username = "Enter the user name.";
  if (draft.privateKeyPath.trim() === "") errors.privateKeyPath = "Choose the private key file.";
  return errors;
}

interface ConnectionFormProps {
  /** `null` to create a connection. */
  connection: ConnectionSummary | null;
  onSaved: () => void;
  onCancel: () => void;
}

/** Creates or edits an SFTP connection. */
export function ConnectionForm({ connection, onSaved, onCancel }: ConnectionFormProps) {
  const settings = connection?.providerConfig;
  const [draft, setDraft] = useState<Draft>({
    name: connection?.name ?? "",
    host: settings?.host ?? "",
    port: String(settings?.port ?? 22),
    username: settings?.username ?? "",
    privateKeyPath: settings?.privateKeyPath ?? "",
    hostKeyVerification: settings?.hostKeyVerification ?? "ymdb",
  });
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const formId = connection ? `connection-form-${connection.id}` : "connection-form-new";

  function update(field: Field, value: string) {
    setDraft((current) => ({ ...current, [field]: value }));
    setErrors((current) => ({ ...current, [field]: undefined }));
  }

  async function chooseKey() {
    try {
      const path = await pickPrivateKeyFile();
      if (path) update("privateKeyPath", path);
    } catch (pickError) {
      setSaveError(describeError(pickError));
    }
  }

  async function save() {
    const found = validateDraft(draft);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    const input: ConnectionInput = {
      id: connection?.id ?? null,
      name: draft.name.trim(),
      providerType: "sftp",
      providerConfig: {
        host: draft.host.trim(),
        port: Number(draft.port),
        username: draft.username.trim(),
        privateKeyPath: draft.privateKeyPath.trim(),
        hostKeyVerification: draft.hostKeyVerification,
      },
    };
    setSaving(true);
    setSaveError(null);
    try {
      await saveRemoteConnection(input);
      onSaved();
    } catch (error) {
      setSaveError(describeError(error));
    } finally {
      setSaving(false);
    }
  }

  const text = (field: Field, label: string, hint?: string, type = "text") => (
    <div className="field">
      <label className="field__label" htmlFor={`${formId}-${field}`}>
        {label}
      </label>
      <input
        id={`${formId}-${field}`}
        className="input"
        type={type}
        value={draft[field]}
        aria-invalid={errors[field] !== undefined}
        aria-describedby={errors[field] ? `${formId}-${field}-error` : undefined}
        onChange={(event) => {
          update(field, event.target.value);
        }}
      />
      {hint && <span className="message message--muted">{hint}</span>}
      {errors[field] && (
        <span id={`${formId}-${field}-error`} className="message message--error">
          {errors[field]}
        </span>
      )}
    </div>
  );

  return (
    <form
      className="inline-form"
      aria-label={connection ? `Edit ${connection.name}` : "New remote connection"}
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <p className="message message--muted">
        Protocol: SFTP over SSH, authenticated with a private key. YMDB keeps the path of the key,
        never the key or its passphrase.
      </p>
      {text("name", "Connection name")}
      {text("host", "Host", "Host name or IP address of the SSH server.")}
      {text("port", "Port", undefined, "number")}
      {text("username", "Username")}
      <div className="field">
        <label className="field__label" htmlFor={`${formId}-key`}>
          Private key file
        </label>
        <div className="remote-key-picker">
          <input
            id={`${formId}-key`}
            className="input"
            value={draft.privateKeyPath}
            aria-invalid={errors.privateKeyPath !== undefined}
            onChange={(event) => {
              update("privateKeyPath", event.target.value);
            }}
          />
          <button
            type="button"
            className="button button--secondary"
            onClick={() => void chooseKey()}
          >
            Choose…
          </button>
        </div>
        {errors.privateKeyPath && (
          <span className="message message--error">{errors.privateKeyPath}</span>
        )}
      </div>
      <fieldset className="choice-group remote-verification">
        <legend>Host key verification</legend>
        <label className="choice">
          <input
            type="radio"
            name={`${formId}-verification`}
            checked={draft.hostKeyVerification === "ymdb"}
            onChange={() => {
              update("hostKeyVerification", "ymdb");
            }}
          />
          Approve the server identity in YMDB
        </label>
        <label className="choice">
          <input
            type="radio"
            name={`${formId}-verification`}
            checked={draft.hostKeyVerification === "knownHosts"}
            onChange={() => {
              update("hostKeyVerification", "knownHosts");
            }}
          />
          Use OpenSSH known_hosts, then approval in YMDB
        </label>
      </fieldset>
      <p className="message message--muted">
        The server identity is always verified. On the first connection YMDB shows its fingerprint
        for approval; a changed identity is refused.
        {connection && " Changing the host or port forgets the approved identity."}
      </p>
      {saveError && (
        <p className="message message--error" role="alert">
          {saveError}
        </p>
      )}
      <div className="actions">
        <button type="submit" className="button" disabled={saving}>
          {saving ? "Saving…" : "Save connection"}
        </button>
        <button type="button" className="button button--secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
