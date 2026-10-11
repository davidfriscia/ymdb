import { useCallback, useState } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import type { InstallationSummary } from "../installations/installationsApi";
import {
  getRemoteSources,
  pickDestinationDirectory,
  saveRemoteSources,
  type CategoryInfo,
  type CategorySource,
  type DirectoriesOrigin,
  type RemoteSourcesConfig,
  type RemoteSourcesView,
  type SearchPathDirectories,
  type SourceCategory,
} from "./remoteApi";
import { ORGANIZATION_LABELS, PROVIDER_LABELS } from "./remoteMessages";

const EMPTY: CategorySource = {
  enabled: false,
  connectionId: null,
  remoteBasePath: null,
  organization: null,
  destination: null,
};

const SEARCH_PATH_OPTIONS = { rom: "rompath", sample: "samplepath" } as const;

const STATUS_LABELS = {
  available: "available",
  missing: "not found",
  notADirectory: "not a directory",
  unreadable: "not readable",
} as const;

/** Empty text fields mean "not set"; incomplete destinations are dropped. */
function cleaned(source: CategorySource): CategorySource {
  const path = source.remoteBasePath?.trim() ?? "";
  const destination =
    source.destination && source.destination.directory.trim() !== "" ? source.destination : null;
  return {
    ...source,
    connectionId: source.connectionId === "" ? null : source.connectionId,
    remoteBasePath: path === "" ? null : path,
    destination,
  };
}

/** What an enabled category still needs; the backend checks again. */
function missingFields(source: CategorySource): string[] {
  if (!source.enabled) return [];
  const missing: string[] = [];
  if (source.connectionId === null) missing.push("a remote connection");
  if (source.remoteBasePath === null) missing.push("a remote base path");
  else if (!source.remoteBasePath.startsWith("/"))
    missing.push("an absolute remote base path (starting with /)");
  if (source.organization === null) missing.push("the collection organization");
  if (source.destination === null) missing.push("a local destination");
  return missing;
}

function describeOrigin(origin: DirectoriesOrigin, iniFile: string | null): string {
  switch (origin) {
    case "launchOptions":
      return "from the global launch options of this installation";
    case "mameIni":
      return `from ${iniFile ?? "mame.ini"}`;
    case "mameDefault":
      return "MAME's default (no mame.ini found)";
  }
}

interface RomManagerSectionProps {
  installation: InstallationSummary;
}

/**
 * MAME page section: the remote sources of one installation, per category.
 * Downloads are not available yet; this only records where files will come
 * from and where they will go.
 */
export function RomManagerSection({ installation }: RomManagerSectionProps) {
  const load = useCallback(() => getRemoteSources(installation.id), [installation.id]);
  const { state, latest, reload } = useLoader(load);
  const [saved, setSaved] = useState<RemoteSourcesView | null>(null);
  const [draft, setDraft] = useState<RemoteSourcesConfig | null>(null);
  const [problems, setProblems] = useState<Partial<Record<SourceCategory, string>>>({});
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ tone: "error" | "success"; text: string } | null>(null);

  const view = saved ?? latest;
  const current = view?.installationId === installation.id ? view : null;
  const config = draft ?? current?.config ?? null;

  function change(category: SourceCategory, update: Partial<CategorySource>) {
    if (!config) return;
    setMessage(null);
    setProblems((previous) => ({ ...previous, [category]: undefined }));
    const source = config.categories[category] ?? EMPTY;
    setDraft({
      ...config,
      categories: { ...config.categories, [category]: { ...source, ...update } },
    });
  }

  async function save() {
    if (!config) return;
    const categories: Partial<Record<SourceCategory, CategorySource>> = {};
    const found: Partial<Record<SourceCategory, string>> = {};
    for (const [category, source] of Object.entries(config.categories) as [
      SourceCategory,
      CategorySource,
    ][]) {
      const clean = cleaned(source);
      categories[category] = clean;
      const missing = missingFields(clean);
      if (missing.length > 0) found[category] = `To enable it, set ${missing.join(", ")}.`;
    }
    setProblems(found);
    if (Object.keys(found).length > 0) {
      setMessage({ tone: "error", text: "Some enabled categories are incomplete." });
      return;
    }
    setSaving(true);
    setMessage(null);
    try {
      const result = await saveRemoteSources(installation.id, { ...config, categories });
      setSaved(result);
      setDraft(null);
      setMessage({ tone: "success", text: "ROM Manager settings saved." });
    } catch (error) {
      setMessage({ tone: "error", text: describeError(error) });
    } finally {
      setSaving(false);
    }
  }

  async function chooseDirectory(category: SourceCategory) {
    try {
      const directory = await pickDestinationDirectory();
      if (directory) change(category, { destination: { mode: "custom", directory } });
    } catch (error) {
      setMessage({ tone: "error", text: describeError(error) });
    }
  }

  return (
    <section className="panel" aria-labelledby="rom-manager-title">
      <h2 id="rom-manager-title" className="panel__title">
        ROM Manager
      </h2>
      <p className="message message--muted">
        Where YMDB will retrieve the files of {installation.label} from, and where it will store
        them. Each category is independent and optional; categories that are not enabled keep using
        only local files. Downloading is not available yet.
      </p>
      {state.status === "error" && current === null && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The ROM Manager settings could not be loaded: {describeError(state.error)}
          </p>
          <button type="button" className="button button--secondary" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {current === null && state.status === "loading" && (
        <p className="message message--muted">Loading…</p>
      )}
      {current && config && (
        <>
          {current.loadError && (
            <p className="message message--warning" role="alert">
              {current.loadError}. Saving replaces it.
            </p>
          )}
          {current.connections.length === 0 && (
            <p className="message message--muted">
              No remote connection is configured yet: add one in Settings → Remote Connections.
            </p>
          )}
          {current.categories.map((info) => (
            <CategoryEditor
              key={info.id}
              info={info}
              source={config.categories[info.id] ?? EMPTY}
              view={current}
              problem={problems[info.id]}
              onChange={(update) => {
                change(info.id, update);
              }}
              onChooseDirectory={() => void chooseDirectory(info.id)}
            />
          ))}
          {message && (
            <p
              className={`message message--${message.tone}`}
              role={message.tone === "error" ? "alert" : "status"}
            >
              {message.text}
            </p>
          )}
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={saving || draft === null}
              onClick={() => void save()}
            >
              {saving ? "Saving…" : "Save ROM Manager settings"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              disabled={saving || draft === null}
              onClick={() => {
                setDraft(null);
                setProblems({});
                setMessage(null);
              }}
            >
              Discard changes
            </button>
          </div>
        </>
      )}
    </section>
  );
}

interface CategoryEditorProps {
  info: CategoryInfo;
  source: CategorySource;
  view: RemoteSourcesView;
  problem: string | undefined;
  onChange: (update: Partial<CategorySource>) => void;
  onChooseDirectory: () => void;
}

function CategoryEditor({
  info,
  source,
  view,
  problem,
  onChange,
  onChooseDirectory,
}: CategoryEditorProps) {
  const id = `rom-manager-${view.installationId}-${info.id}`;
  const directories: SearchPathDirectories | undefined = view.mameDirectories.find(
    (entry) => entry.searchPath === info.searchPath,
  );
  const option = SEARCH_PATH_OPTIONS[info.searchPath];
  const destination = source.destination;
  const warnings = view.warnings.filter((warning) => warning.category === info.id);
  const knownConnection =
    source.connectionId === null ||
    view.connections.some((connection) => connection.id === source.connectionId);

  return (
    <fieldset className="rom-manager__category">
      <legend className="rom-manager__legend">{info.label}</legend>
      <label className="choice">
        <input
          type="checkbox"
          checked={source.enabled}
          onChange={(event) => {
            onChange({ enabled: event.target.checked });
          }}
        />
        Enabled
      </label>
      <div className="rom-manager__fields">
        <label className="field">
          <span className="field__label">Remote connection</span>
          <select
            className="input"
            value={source.connectionId ?? ""}
            onChange={(event) => {
              onChange({ connectionId: event.target.value === "" ? null : event.target.value });
            }}
          >
            <option value="">Not set</option>
            {!knownConnection && source.connectionId !== null && (
              <option value={source.connectionId}>Removed connection</option>
            )}
            {view.connections.map((connection) => (
              <option key={connection.id} value={connection.id}>
                {connection.name} ({PROVIDER_LABELS[connection.providerType]})
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span className="field__label">Remote base path</span>
          <input
            className="input"
            placeholder={
              info.searchPath === "sample" ? "/volume1/mame/samples" : "/volume1/mame/roms"
            }
            value={source.remoteBasePath ?? ""}
            onChange={(event) => {
              onChange({ remoteBasePath: event.target.value });
            }}
          />
        </label>
        <label className="field">
          <span className="field__label">Collection organization</span>
          <select
            className="input"
            value={source.organization ?? ""}
            onChange={(event) => {
              const value = info.organizations.find(
                (organization) => organization === event.target.value,
              );
              onChange({ organization: value ?? null });
            }}
          >
            <option value="">Not set</option>
            {info.organizations.map((organization) => (
              <option key={organization} value={organization}>
                {ORGANIZATION_LABELS[organization]}
              </option>
            ))}
          </select>
        </label>
      </div>
      {(info.id === "machineChds" || info.id === "softwareListChds") && (
        <p className="message message--muted">
          For CHDs, the set type says where the disks of clones are stored (with the parent, in each
          clone&apos;s folder, or both). CHD files are always downloaded whole.
        </p>
      )}
      <fieldset className="choice-group rom-manager__destination">
        <legend>Local destination</legend>
        <label className="choice">
          <input
            type="radio"
            name={`${id}-destination`}
            checked={destination?.mode !== "custom"}
            onChange={() => {
              onChange({ destination: null });
            }}
          />
          A MAME directory ({option})
        </label>
        <label className="choice">
          <input
            type="radio"
            name={`${id}-destination`}
            checked={destination?.mode === "custom"}
            onChange={() => {
              onChange({ destination: { mode: "custom", directory: "" } });
            }}
          />
          A custom directory
        </label>
      </fieldset>
      {destination?.mode === "custom" ? (
        <div className="field">
          <span className="field__label">Custom directory</span>
          <div className="remote-key-picker">
            <span className="code wrap">
              {destination.directory === "" ? "No directory chosen" : destination.directory}
            </span>
            <button
              type="button"
              className="button button--secondary button--small"
              onClick={onChooseDirectory}
            >
              Choose…
            </button>
          </div>
          <span className="message message--muted">
            When launching MAME, YMDB adds this directory to -{option} after the directories MAME
            already uses. mame.ini is not modified.
          </span>
        </div>
      ) : (
        <label className="field">
          <span className="field__label">
            MAME directory{" "}
            {directories && (
              <span className="message--muted">
                ({describeOrigin(directories.origin, view.iniFile)})
              </span>
            )}
          </span>
          <select
            className="input"
            value={destination?.mode === "mameDirectory" ? destination.directory : ""}
            onChange={(event) => {
              onChange({
                destination:
                  event.target.value === ""
                    ? null
                    : { mode: "mameDirectory", directory: event.target.value },
              });
            }}
          >
            <option value="">Choose a directory</option>
            {destination?.mode === "mameDirectory" &&
              !directories?.directories.some((entry) => entry.entry === destination.directory) && (
                <option value={destination.directory}>
                  {destination.directory} (no longer in {option})
                </option>
              )}
            {directories?.directories.map((entry) => (
              <option key={entry.entry} value={entry.entry}>
                {entry.entry === entry.path ? entry.entry : `${entry.entry} — ${entry.path}`} (
                {STATUS_LABELS[entry.status]})
              </option>
            ))}
          </select>
        </label>
      )}
      {warnings.map((warning) => (
        <p key={warning.message} className="message message--warning">
          {warning.message}
        </p>
      ))}
      {problem && (
        <p className="message message--error" role="alert">
          {problem}
        </p>
      )}
    </fieldset>
  );
}
