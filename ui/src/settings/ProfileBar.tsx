import { useCallback, useEffect, useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { ProfileEntry } from "../generated/ProfileEntry";
import type { Transport } from "../transport";

export interface Loaded {
  name: string;
  config: CrawlConfig;
}

/** Compares settings, ignoring the derived speed preset name. */
export function sameSettings(a: CrawlConfig, b: CrawlConfig): boolean {
  const strip = (c: CrawlConfig) => JSON.stringify({ ...c, speed: { ...c.speed, preset: undefined } });
  return strip(a) === strip(b);
}

export function ProfileBar({
  transport,
  config,
  startUrl,
  loaded,
  onLoad,
  onSaved,
  onReset,
}: {
  transport: Transport;
  config: CrawlConfig;
  startUrl: string | null;
  loaded: Loaded | null;
  onLoad: (l: Loaded, startUrl: string | null) => void;
  onSaved: (l: Loaded) => void;
  onReset: () => void;
}) {
  const [profiles, setProfiles] = useState<ProfileEntry[]>([]);
  const [name, setName] = useState("");
  const [includeStart, setIncludeStart] = useState(false);
  const [confirmOverwrite, setConfirmOverwrite] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => setProfiles(await transport.profiles()), [transport]);
  useEffect(() => {
    refresh();
  }, [refresh]);

  const modified = loaded !== null && !sameSettings(loaded.config, config);

  const act = async (work: () => Promise<void>) => {
    setMessage(null);
    try {
      await work();
      await refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const save = (target: string, overwrite: boolean) =>
    act(async () => {
      const r = await transport.saveProfile(target, config, includeStart ? startUrl : null, overwrite);
      if (r.result === "name_taken") {
        setConfirmOverwrite(target);
        return;
      }
      setConfirmOverwrite(null);
      onSaved({ name: target, config });
      setMessage(`Saved "${target}".`);
    });

  const load = (n: string) =>
    act(async () => {
      if (!n) return;
      const p = await transport.loadProfile(n);
      onLoad({ name: p.name, config: p.config }, p.start_url);
    });

  const importFile = () =>
    act(async () => {
      const path = await transport.pickProfileFile();
      if (!path) return;
      const p = await transport.importProfile(path);
      onLoad({ name: p.name, config: p.config }, p.start_url);
      setMessage(
        p.ignored.length
          ? `Imported "${p.name}". Settings this version does not know were ignored: ${p.ignored.join(", ")}.`
          : `Imported "${p.name}".`,
      );
    });

  const exportFile = () =>
    act(async () => {
      if (!loaded) return;
      const path = await transport.pickExportPath(loaded.name);
      if (path) await transport.exportProfile(loaded.name, path);
    });

  return (
    <div className="profile-bar">
      <label className="field inline">
        <span className="label">Profile</span>
        <select aria-label="Profile" value={loaded?.name ?? ""} onChange={(e) => load(e.target.value)}>
          <option value="">(none)</option>
          {profiles.map((p) => (
            <option key={p.name} value={p.name} disabled={p.error !== null}>
              {p.name}
              {p.is_default ? " (default)" : ""}
              {p.error ? " (unreadable)" : ""}
            </option>
          ))}
        </select>
      </label>
      {modified && (
        <span className="badge" role="status">
          Modified
        </span>
      )}
      {loaded && modified && (
        <>
          <button type="button" onClick={() => save(loaded.name, true)}>
            Save
          </button>
          <button type="button" onClick={() => onLoad(loaded, startUrl)}>
            Revert
          </button>
        </>
      )}
      <input aria-label="New profile name" placeholder="New profile name" value={name} onChange={(e) => setName(e.target.value)} />
      <button type="button" disabled={!name.trim()} onClick={() => save(name.trim(), false)}>
        Save as new
      </button>
      <label className="toggle">
        <input type="checkbox" checked={includeStart} onChange={(e) => setIncludeStart(e.target.checked)} /> Include
        start URL
      </label>
      {loaded && (
        <>
          <button type="button" onClick={() => act(() => transport.setDefaultProfile(loaded.name))}>
            Make default
          </button>
          <button
            type="button"
            disabled={!name.trim()}
            onClick={() =>
              act(async () => {
                await transport.renameProfile(loaded.name, name.trim());
                onSaved({ name: name.trim(), config: loaded.config });
              })
            }
          >
            Rename to new name
          </button>
          <button
            type="button"
            disabled={!name.trim()}
            onClick={() => act(() => transport.duplicateProfile(loaded.name, name.trim()))}
          >
            Duplicate as new name
          </button>
          <button type="button" onClick={exportFile}>
            Export…
          </button>
          <button
            type="button"
            onClick={() =>
              act(async () => {
                await transport.deleteProfile(loaded.name);
                onReset();
              })
            }
          >
            Delete
          </button>
        </>
      )}
      <button type="button" onClick={importFile}>
        Import…
      </button>
      <button type="button" onClick={onReset}>
        Reset to defaults
      </button>
      {confirmOverwrite && (
        <div role="alert" className="confirm">
          A profile named "{confirmOverwrite}" already exists.{" "}
          <button type="button" onClick={() => save(confirmOverwrite, true)}>
            Overwrite
          </button>{" "}
          <button type="button" onClick={() => setConfirmOverwrite(null)}>
            Choose another name
          </button>
        </div>
      )}
      {message && <p className="notice">{message}</p>}
    </div>
  );
}
