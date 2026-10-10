import { useState } from "react";
import { describeError } from "../../shared/ipc";
import {
  DEFAULT_PROFILE_ID,
  profileId,
  type LaunchConfig,
  type OptionDefinition,
  type OptionSet,
  type Profile,
} from "./launcherApi";
import { useLauncher } from "./launcherContext";
import { OptionSetEditor } from "./OptionSetEditor";
import { describeValue, omit } from "./optionValues";

/** Settings page section: global options, profiles and overrides. */
export function LaunchSettings() {
  const { view, error, reloadConfig, configVersion } = useLauncher();
  // Kept here: the sections below are remounted after each save.
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  return (
    <section className="panel" aria-labelledby="launch-settings-title">
      <h2 id="launch-settings-title" className="panel__title">
        MAME launch options
      </h2>
      <p className="message message--muted">
        YMDB passes only the options set here; everything else follows MAME&apos;s own configuration
        (mame.ini), which YMDB never modifies. Precedence, from lowest to highest: MAME
        configuration, global options, launch profile, machine options, software options.
      </p>
      {error !== null && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The launch options could not be loaded: {describeError(error)}
          </p>
          <button type="button" className="button button--secondary" onClick={reloadConfig}>
            Retry
          </button>
        </div>
      )}
      {view?.loadError && (
        <p className="message message--warning" role="alert">
          {view.loadError}. The defaults are shown; saving replaces the file.
        </p>
      )}
      {view === null && error === null && <p className="message message--muted">Loading…</p>}
      {view && (
        // Remounted after each save so that drafts start from the saved configuration.
        <div key={configVersion}>
          <GlobalOptions config={view.config} definitions={view.definitions} />
          <Profiles
            config={view.config}
            definitions={view.definitions}
            selectedId={selectedProfileId ?? view.config.defaultProfileId}
            onSelect={setSelectedProfileId}
          />
          <Overrides config={view.config} />
        </div>
      )}
    </section>
  );
}

interface SectionProps {
  config: LaunchConfig;
  definitions: OptionDefinition[];
}

function useSave() {
  const { saveConfig } = useLauncher();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function save(config: LaunchConfig): Promise<boolean> {
    setSaving(true);
    setError(null);
    try {
      await saveConfig(config);
      return true;
    } catch (saveError) {
      setError(describeError(saveError));
      return false;
    } finally {
      setSaving(false);
    }
  }
  return { save, saving, error };
}

function SaveError({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <p className="message message--error" role="alert">
      {error}
    </p>
  );
}

function GlobalOptions({ config, definitions }: SectionProps) {
  const [draft, setDraft] = useState<OptionSet>(config.globalOptions);
  const [valid, setValid] = useState(true);
  const { save, saving, error } = useSave();
  return (
    <>
      <h3 className="panel__subtitle">Global options</h3>
      <OptionSetEditor
        label="Global options"
        definitions={definitions}
        options={draft}
        onChange={setDraft}
        onValidityChange={setValid}
      />
      <SaveError error={error} />
      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={!valid || saving}
          onClick={() => void save({ ...config, globalOptions: draft })}
        >
          Save global options
        </button>
      </div>
    </>
  );
}

function Profiles({
  config,
  definitions,
  selectedId,
  onSelect,
}: SectionProps & { selectedId: string; onSelect: (id: string) => void }) {
  const [newName, setNewName] = useState("");
  const { save, saving, error } = useSave();
  const selected =
    config.profiles.find((profile) => profile.id === selectedId) ?? config.profiles[0];

  function create() {
    const name = newName.trim();
    if (!name) return;
    const id = profileId(name, config.profiles);
    void save({ ...config, profiles: [...config.profiles, { id, name, options: {} }] }).then(
      (saved) => {
        if (saved) onSelect(id);
      },
    );
  }

  return (
    <>
      <h3 className="panel__subtitle">Launch profiles</h3>
      <p className="message message--muted">
        A profile is a named set of options chosen when launching. The default profile is used when
        no other profile is selected.
      </p>
      <div className="launch-panel__row">
        <label className="launch-panel__field">
          <span>Profile</span>
          <select
            className="input"
            value={selected?.id ?? ""}
            onChange={(event) => {
              onSelect(event.target.value);
            }}
          >
            {config.profiles.map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
                {profile.id === config.defaultProfileId ? " (default)" : ""}
              </option>
            ))}
          </select>
        </label>
        <form
          className="launch-panel__row"
          onSubmit={(event) => {
            event.preventDefault();
            create();
          }}
        >
          <input
            className="input"
            aria-label="New profile name"
            placeholder="New profile name"
            maxLength={60}
            value={newName}
            onChange={(event) => {
              setNewName(event.target.value);
            }}
          />
          <button
            type="submit"
            className="button button--secondary"
            disabled={saving || newName.trim() === ""}
          >
            Create profile
          </button>
        </form>
      </div>
      <SaveError error={error} />
      {selected && (
        <ProfileEditor
          key={selected.id}
          config={config}
          definitions={definitions}
          profile={selected}
        />
      )}
    </>
  );
}

function ProfileEditor({ config, definitions, profile }: SectionProps & { profile: Profile }) {
  const [name, setName] = useState(profile.name);
  const [options, setOptions] = useState<OptionSet>(profile.options);
  const [valid, setValid] = useState(true);
  const { save, saving, error } = useSave();
  const isDefault = profile.id === config.defaultProfileId;
  const removable = profile.id !== DEFAULT_PROFILE_ID;

  const replace = (next: Profile) =>
    config.profiles.map((entry) => (entry.id === profile.id ? next : entry));

  return (
    <div className="launch-settings__profile" role="group" aria-label={`Profile ${profile.name}`}>
      <label className="launch-panel__field">
        <span>Name</span>
        <input
          className="input"
          maxLength={60}
          value={name}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
      </label>
      <OptionSetEditor
        label={`Options of ${profile.name}`}
        definitions={definitions}
        options={options}
        onChange={setOptions}
        onValidityChange={setValid}
      />
      <SaveError error={error} />
      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={!valid || saving || name.trim() === ""}
          onClick={() => void save({ ...config, profiles: replace({ ...profile, name, options }) })}
        >
          Save profile
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={isDefault || saving}
          onClick={() => void save({ ...config, defaultProfileId: profile.id })}
        >
          {isDefault ? "Default profile" : "Make default"}
        </button>
        {removable && (
          <button
            type="button"
            className="button button--secondary"
            disabled={saving}
            onClick={() =>
              void save({
                ...config,
                profiles: config.profiles.filter((entry) => entry.id !== profile.id),
                defaultProfileId: isDefault ? DEFAULT_PROFILE_ID : config.defaultProfileId,
              })
            }
          >
            Delete profile
          </button>
        )}
      </div>
    </div>
  );
}

function Overrides({ config }: { config: LaunchConfig }) {
  const { save, saving, error } = useSave();
  const entries = [
    ...Object.entries(config.machineOverrides).map(([key, options]) => ({
      kind: "machine" as const,
      key,
      options: options ?? {},
    })),
    ...Object.entries(config.softwareOverrides).map(([key, options]) => ({
      kind: "software" as const,
      key,
      options: options ?? {},
    })),
  ];

  function remove(kind: "machine" | "software", key: string) {
    const field = kind === "machine" ? "machineOverrides" : "softwareOverrides";
    void save({ ...config, [field]: omit(config[field], key) });
  }

  return (
    <>
      <h3 className="panel__subtitle">Machine and software options</h3>
      <p className="message message--muted">
        Set them from the detail page of a machine or software, with &quot;Options for this
        machine&quot; or &quot;Options for this software&quot;.
      </p>
      {entries.length === 0 ? (
        <p className="message message--muted">No machine or software has its own options.</p>
      ) : (
        <ul className="launch-settings__overrides" aria-label="Machine and software options">
          {entries.map((entry) => (
            <li key={`${entry.kind}:${entry.key}`}>
              <span className="badge">{entry.kind === "machine" ? "Machine" : "Software"}</span>{" "}
              <span className="code">{entry.key}</span>
              <span className="message--muted">
                {" · "}
                {Object.entries(entry.options)
                  .map(([name, value]) =>
                    value === undefined ? name : `${name}: ${describeValue(value)}`,
                  )
                  .join(", ")}
              </span>{" "}
              <button
                type="button"
                className="button button--secondary button--small"
                aria-label={`Remove options of ${entry.key}`}
                disabled={saving}
                onClick={() => {
                  remove(entry.kind, entry.key);
                }}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <SaveError error={error} />
    </>
  );
}
