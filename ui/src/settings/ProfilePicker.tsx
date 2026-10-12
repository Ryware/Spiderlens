import { useCallback, useEffect, useState } from "react";
import { Ellipsis } from "lucide-react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { ProfileEntry } from "../generated/ProfileEntry";
import type { Transport } from "../transport";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import { Badge, Button, Input, Menu, Modal, Notice, Select, Switch } from "../components/ui";

/** Compares settings, ignoring the derived speed preset name. */
export function sameSettings(a: CrawlConfig, b: CrawlConfig): boolean {
  const strip = (c: CrawlConfig) => JSON.stringify({ ...c, speed: { ...c.speed, preset: undefined } });
  return strip(a) === strip(b);
}

type NameAction = "save" | "rename" | "duplicate";
const actionText: Record<NameAction, { title: string; button: string }> = {
  save: { title: "Save as new profile", button: "Save" },
  rename: { title: "Rename profile", button: "Rename" },
  duplicate: { title: "Duplicate profile", button: "Duplicate" },
};

/**
 * Profile switching plus every profile action in one menu (FR-012). The list
 * is a native select so the open menu is the platform's own.
 */
export function ProfilePicker({ transport, setup }: { transport: Transport; setup: CrawlSetup }) {
  const { config, loaded, startUrl } = setup;
  const [profiles, setProfiles] = useState<ProfileEntry[]>([]);
  const [message, setMessage] = useState<{ kind: "info" | "bad"; text: string } | null>(null);
  const [naming, setNaming] = useState<NameAction | null>(null);
  const [name, setName] = useState("");
  const [includeStart, setIncludeStart] = useState(false);
  const [confirmOverwrite, setConfirmOverwrite] = useState<string | null>(null);

  const refresh = useCallback(async () => setProfiles(await transport.profiles()), [transport]);
  useEffect(() => {
    refresh();
  }, [refresh]);

  if (!config) return null;
  const modified = loaded !== null && !sameSettings(loaded.config, config);

  const act = async (work: () => Promise<void>) => {
    setMessage(null);
    try {
      await work();
      await refresh();
    } catch (e) {
      setMessage({ kind: "bad", text: String(e) });
    }
  };

  const save = (target: string, overwrite: boolean) =>
    act(async () => {
      const r = await transport.saveProfile(target, config, includeStart ? startUrl || null : null, overwrite);
      if (r.result === "name_taken") {
        setConfirmOverwrite(target);
        return;
      }
      setConfirmOverwrite(null);
      setNaming(null);
      setup.setLoaded({ name: target, config });
      setMessage({ kind: "info", text: `Saved "${target}".` });
    });

  const submitName = () => {
    const n = name.trim();
    if (!n || !naming) return;
    if (naming === "save") return save(n, false);
    return act(async () => {
      if (!loaded) return;
      if (naming === "rename") {
        await transport.renameProfile(loaded.name, n);
        setup.setLoaded({ name: n, config: loaded.config });
      } else {
        await transport.duplicateProfile(loaded.name, n);
      }
      setNaming(null);
    });
  };

  const load = (n: string) =>
    act(async () => {
      if (!n) return;
      const p = await transport.loadProfile(n);
      setup.applyProfile({ name: p.name, config: p.config }, p.start_url);
    });

  const importFile = () =>
    act(async () => {
      const path = await transport.pickProfileFile();
      if (!path) return;
      const p = await transport.importProfile(path);
      setup.applyProfile({ name: p.name, config: p.config }, p.start_url);
      setMessage({
        kind: "info",
        text: p.ignored.length
          ? `Imported "${p.name}". Settings this version does not know were ignored: ${p.ignored.join(", ")}.`
          : `Imported "${p.name}".`,
      });
    });

  const exportFile = () =>
    act(async () => {
      if (!loaded) return;
      const path = await transport.pickExportPath(loaded.name);
      if (path) await transport.exportProfile(loaded.name, path);
    });

  const askName = (a: NameAction) => {
    setName(a === "save" ? "" : (loaded?.name ?? ""));
    setConfirmOverwrite(null);
    setNaming(a);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-1">
        <Select aria-label="Profile" className="min-w-0 flex-1" value={loaded?.name ?? ""} onChange={(e) => load(e.target.value)}>
          <option value="">No profile</option>
          {profiles.map((p) => (
            <option key={p.name} value={p.name} disabled={p.error !== null}>
              {p.name}
              {p.is_default ? " (default)" : ""}
              {p.error ? " (unreadable)" : ""}
            </option>
          ))}
        </Select>
        <Menu
          label="Profile actions"
          trigger={
            <Button variant="quiet" icon>
              <Ellipsis className="size-4" />
            </Button>
          }
          items={[
            { label: "Save as new profile…", onSelect: () => askName("save") },
            { label: "Rename…", onSelect: () => askName("rename"), disabled: !loaded },
            { label: "Duplicate…", onSelect: () => askName("duplicate"), disabled: !loaded },
            { label: "Make default", onSelect: () => loaded && act(() => transport.setDefaultProfile(loaded.name)), disabled: !loaded },
            { label: "Export…", onSelect: exportFile, disabled: !loaded },
            null,
            { label: "Import…", onSelect: importFile },
            { label: "Reset to defaults", onSelect: setup.resetToDefaults },
            null,
            {
              label: "Delete profile",
              destructive: true,
              disabled: !loaded,
              onSelect: () =>
                loaded &&
                act(async () => {
                  await transport.deleteProfile(loaded.name);
                  await setup.resetToDefaults();
                }),
            },
          ]}
        />
      </div>
      {modified && loaded && (
        <div className="flex items-center gap-1.5">
          <Badge tone="warn">
            <span role="status">Modified</span>
          </Badge>
          <Button size="sm" onClick={() => save(loaded.name, true)}>
            Save
          </Button>
          <Button size="sm" variant="quiet" onClick={() => setup.applyProfile(loaded, startUrl || null)}>
            Revert
          </Button>
        </div>
      )}
      {message && (
        <Notice kind={message.kind} role={message.kind === "bad" ? "alert" : "status"}>
          {message.text}
        </Notice>
      )}

      <Modal open={naming !== null} onOpenChange={(o) => !o && setNaming(null)} title={naming ? actionText[naming].title : ""}>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            submitName();
          }}
        >
          <Input aria-label="New profile name" placeholder="Profile name" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
          {naming === "save" && (
            <label className="flex items-center gap-2 text-sm text-fg-2">
              <Switch label="Include start URL" checked={includeStart} onChange={setIncludeStart} />
              Include the start URL
            </label>
          )}
          {confirmOverwrite && (
            <Notice kind="warn" role="alert">
              A profile named "{confirmOverwrite}" already exists.
            </Notice>
          )}
          <div className="flex justify-end gap-2">
            {confirmOverwrite ? (
              <>
                <Button onClick={() => setConfirmOverwrite(null)}>Choose another name</Button>
                <Button variant="primary" onClick={() => save(confirmOverwrite, true)}>
                  Overwrite
                </Button>
              </>
            ) : (
              <>
                <Button onClick={() => setNaming(null)}>Cancel</Button>
                <Button variant="primary" type="submit" disabled={!name.trim()}>
                  {naming ? actionText[naming].button : ""}
                </Button>
              </>
            )}
          </div>
        </form>
      </Modal>
    </div>
  );
}
