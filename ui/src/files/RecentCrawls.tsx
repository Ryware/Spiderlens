import { useState } from "react";
import { FileQuestion } from "lucide-react";
import { Badge, Button, Modal, Notice, Tooltip, type Tone } from "../components/ui";
import type { CrawlStatus, RecentEntry, StorageStatus } from "../transport";
import { formatSize, formatWhen } from "./format";
import type { CrawlLibrary } from "./useCrawlLibrary";

export const statusInfo: Record<CrawlStatus, { label: string; tone: Tone }> = {
  running: { label: "Running", tone: "info" },
  paused: { label: "Paused", tone: "warn" },
  stopped: { label: "Stopped", tone: "neutral" },
  completed: { label: "Completed", tone: "ok" },
};

const title = (e: RecentEntry) => e.name || e.start_url || e.path;

/** The badges an entry carries: its status, then Recovered and Unsaved. */
export function EntryBadges({ entry }: { entry: Pick<RecentEntry, "status" | "recovered" | "unsaved"> }) {
  const s = statusInfo[entry.status];
  return (
    <span className="flex shrink-0 items-center gap-1">
      <Badge tone={s.tone}>{s.label}</Badge>
      {entry.recovered && <Badge tone="info">Recovered</Badge>}
      {entry.unsaved && <Badge>Unsaved</Badge>}
    </span>
  );
}

/** Recent crawls on the start screen (spec 006 US4): one click opens one. */
export function RecentCrawls({ library, onOpen }: { library: CrawlLibrary; onOpen: (path: string) => void }) {
  const [error, setError] = useState<string | null>(null);
  if (library.recent.length === 0) return null;
  const act = (f: () => Promise<void>) => () => {
    setError(null);
    f().catch((e) => setError(String(e)));
  };
  return (
    <section aria-labelledby="recent-crawls" className="flex w-full max-w-xl flex-col gap-1.5">
      <h2 id="recent-crawls" className="m-0 px-1 text-xs font-semibold text-fg-3">
        Recent crawls
      </h2>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      <ul aria-label="Recent crawls" className="m-0 flex list-none flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0">
        {library.recent.map((e) => (
          <li key={e.path} data-path={e.path} data-available={e.available}>
            {e.available ? (
              <button
                type="button"
                onClick={() => onOpen(e.path)}
                title={e.path}
                className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-surface-2 focus-visible:bg-surface-2 focus-visible:outline-none"
              >
                <EntryText entry={e} />
                <EntryBadges entry={e} />
              </button>
            ) : (
              <div className="flex items-center gap-3 px-3 py-2">
                <div className="flex min-w-0 flex-1 items-center gap-3 opacity-55">
                  <EntryText entry={e} />
                </div>
                <span className="flex shrink-0 items-center gap-1 text-xs text-fg-3">
                  <FileQuestion aria-hidden className="size-3.5" /> File not found
                </span>
                <Button size="sm" onClick={act(() => library.locate(e.path))}>
                  Locate…
                </Button>
                <Tooltip content="Removes it from this list. The file itself is not deleted.">
                  <Button size="sm" variant="quiet" aria-description="Removes it from this list. The file itself is not deleted." onClick={act(() => library.remove(e.path))}>
                    Remove
                  </Button>
                </Tooltip>
              </div>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

function EntryText({ entry: e }: { entry: RecentEntry }) {
  return (
    <>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-base font-medium text-fg">{title(e)}</span>
        {e.start_url && e.start_url !== title(e) && <span className="truncate font-code text-xs text-fg-3">{e.start_url}</span>}
      </span>
      <span className="flex shrink-0 flex-col items-end text-xs text-fg-3 tabular">
        <span>{formatWhen(e.last_used)}</span>
        <span>{e.urls.toLocaleString()} URLs</span>
      </span>
    </>
  );
}

/** Crawls kept in the app's own folder until saved somewhere, by size, so space can be freed (spec 006 edge cases). */
export function UnsavedCrawls({ library, openPath = null, onManage }: { library: CrawlLibrary; openPath?: string | null; onManage?: () => void }) {
  const [confirm, setConfirm] = useState<RecentEntry | null>(null);
  const [error, setError] = useState<string | null>(null);
  const u = library.unsaved;
  if (!u || u.entries.length === 0) return null;
  const remove = async (e: RecentEntry) => {
    setConfirm(null);
    try {
      await library.deleteUnsaved(e.path);
    } catch (err) {
      setError(String(err));
    }
  };
  return (
    <section aria-labelledby="unsaved-crawls" className="flex w-full max-w-xl flex-col gap-1.5">
      <h2 id="unsaved-crawls" className="m-0 flex items-baseline gap-2 px-1 text-xs font-semibold text-fg-3">
        Unsaved crawls
        <span className="ml-auto font-normal tabular" data-testid="unsaved-total">
          Unsaved crawls use {formatSize(u.total_bytes)}
        </span>
        {onManage && (
          <Button size="sm" variant="quiet" onClick={onManage}>
            Manage crawls…
          </Button>
        )}
      </h2>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      <ul aria-label="Unsaved crawls" className="m-0 flex list-none flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0">
        {u.entries.map((e) => (
          <li key={e.path} data-path={e.path} className="flex items-center gap-3 px-3 py-1.5 text-sm">
            <span className="min-w-0 flex-1 truncate" title={e.path}>
              {title(e)}
            </span>
            <span className="shrink-0 text-fg-3 tabular">{formatSize(u.size_bytes[e.path] ?? 0)}</span>
            <Button size="sm" variant="destructive" disabled={e.path === openPath} onClick={() => setConfirm(e)}>
              Delete…
            </Button>
          </li>
        ))}
      </ul>
      {confirm && (
        <Modal open onOpenChange={(o) => !o && setConfirm(null)} title={`Move ${title(confirm)} to the Trash?`} description="You can restore it from the Trash.">
          <div className="flex justify-end gap-2">
            <Button onClick={() => setConfirm(null)}>Cancel</Button>
            <Button variant="destructive" onClick={() => void remove(confirm)}>
              Move to Trash
            </Button>
          </div>
        </Modal>
      )}
    </section>
  );
}

/** Little space left where unsaved crawls are kept. */
export function StorageWarning({ storage, onManage }: { storage: StorageStatus | null; onManage?: () => void }) {
  if (!storage?.low) return null;
  return (
    <Notice kind="warn" role="alert" className="w-full max-w-xl rounded-lg border border-line bg-surface px-3 py-2">
      {storage.free_bytes !== null
        ? `Only ${formatSize(storage.free_bytes)} free on the drive for unsaved crawls (keep at least ${formatSize(storage.reserve_bytes)}).`
        : `Little space is free on the drive for unsaved crawls (keep at least ${formatSize(storage.reserve_bytes)}).`}{" "}
      Save crawls elsewhere or delete ones you no longer need.
      {onManage && (
        <Button size="sm" className="ml-2" onClick={onManage}>
          Manage crawls…
        </Button>
      )}
    </Notice>
  );
}
