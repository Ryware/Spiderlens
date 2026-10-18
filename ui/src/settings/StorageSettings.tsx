import { useEffect, useState } from "react";
import type { Issue } from "../generated/Issue";
import { Button, NumberInput, Select, Switch } from "../components/ui";
import { formatDateTime, formatSize, fileStem } from "../files/format";
import type { CleanupReason, CleanupRecord, StoragePrefs, Transport } from "../transport";
import { Group, IssueList, issuesAt, Row } from "./fields";

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const DAY_CHOICES = [7, 14, 30, 90];

const reasons: Record<CleanupReason, string> = {
  manual_delete: "Moved to the Trash",
  manual_delete_permanent: "Deleted permanently",
  auto_age: "Unsaved, not used for a while: moved to the Trash",
  leftover: "Leftover file from a crash",
  html_purge: "Stored HTML removed",
  schedule_retention: "Scheduled run beyond its keep-last limit: moved to the Trash",
};

/** Reserve in GB as the user types it; whole MB underneath. */
const toGb = (bytes: number) => Math.round((bytes / GB) * 100) / 100;

/**
 * Settings › Storage (spec 019 FR-025): automatic cleanup of unsaved crawls,
 * the free-space safety reserve, where unsaved crawls are kept, and what
 * cleanup removed. Saved on this computer, not in the crawl profile.
 */
export function StorageSettings({ transport }: { transport: Transport }) {
  const [prefs, setPrefs] = useState<StoragePrefs | null>(null);
  const [reserveGb, setReserveGb] = useState<number | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [history, setHistory] = useState<CleanupRecord[] | null>(null);
  const [folder, setFolder] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void transport.storagePrefs().then((p) => {
      if (!live) return;
      setPrefs(p);
      setReserveGb(toGb(p.reserve_bytes));
    });
    void transport.storageStatus().then((s) => live && setFolder(s.unsaved_dir));
    return () => {
      live = false;
    };
  }, [transport]);

  const save = async (next: StoragePrefs) => {
    try {
      const saved = await transport.setStoragePrefs(next);
      setPrefs(saved);
      setIssues([]);
    } catch (e) {
      try {
        setIssues(JSON.parse(String(e)) as Issue[]);
      } catch {
        setIssues([{ path: "storage", code: "write", message: String(e), severity: "error" }]);
      }
    }
  };

  if (!prefs) return <p className="m-0 text-sm text-fg-3">Reading storage settings…</p>;
  const on = prefs.cleanup_days !== null;

  return (
    <>
      <IssueList issues={issues.filter((i) => i.path === "storage")} />
      <Group title="Unsaved crawls">
        <Row
          label="Remove unused unsaved crawls"
          help="Unsaved crawls you have not opened for this long go to the Trash when SpiderLens starts. Saved crawls are never removed."
          issues={issuesAt(issues, "storage.cleanup_days")}
        >
          {(_, l) => <Switch labelledBy={l} checked={on} onChange={(v) => void save({ ...prefs, cleanup_days: v ? 30 : null })} />}
        </Row>
        {on && (
          <Row label="Remove after">
            {(id) => (
              <Select id={id} className="w-36" value={String(prefs.cleanup_days)} onChange={(e) => void save({ ...prefs, cleanup_days: Number(e.target.value) })}>
                {DAY_CHOICES.map((d) => (
                  <option key={d} value={d}>
                    {d} days
                  </option>
                ))}
              </Select>
            )}
          </Row>
        )}
        {folder && (
          <Row label="Kept in">
            {() => (
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate font-code text-sm text-fg-2" title={folder}>
                  {folder}
                </span>
                <Button size="sm" onClick={() => void transport.revealPath(folder)}>
                  Show in folder
                </Button>
              </span>
            )}
          </Row>
        )}
      </Group>

      <Group title="Disk space">
        <Row
          label="Keep free (GB)"
          help="A crawl pauses before the disk gets this full, so it can be resumed after you free some space. Between 0.25 and 50 GB."
          issues={issuesAt(issues, "storage.reserve_bytes")}
        >
          {(id) => (
            <NumberInput
              id={id}
              className="w-28"
              step={0.25}
              value={reserveGb}
              onChange={(v) => setReserveGb(v)}
              onBlur={() =>
                reserveGb !== null && Number.isFinite(reserveGb)
                  ? void save({ ...prefs, reserve_bytes: Math.round(reserveGb * 1024) * MB })
                  : setIssues([{ path: "storage.reserve_bytes", code: "reserve_range", message: "Enter how many GB to keep free, between 0.25 and 50.", severity: "error" }])
              }
            />
          )}
        </Row>
      </Group>

      <Group title="Cleanup history">
        <Row label="What was removed" help="Deletes, removed HTML and automatic cleanup, newest first." stacked>
          {() =>
            history === null ? (
              <Button size="sm" className="self-start" onClick={() => void transport.cleanupHistory().then(setHistory)}>
                Show history
              </Button>
            ) : history.length === 0 ? (
              <p className="m-0 text-sm text-fg-3">Nothing removed yet.</p>
            ) : (
              <table className="w-full border-collapse text-sm" aria-label="Cleanup history">
                <tbody>
                  {history.map((r, i) => (
                    <tr key={i} className="border-t border-line first:border-t-0">
                      <td className="py-1 pr-2 whitespace-nowrap text-fg-3">{formatDateTime(r.at)}</td>
                      <td className="max-w-60 truncate py-1 pr-2" title={r.path}>
                        {fileStem(r.path)}
                      </td>
                      <td className="py-1 pr-2 text-fg-2">{reasons[r.reason]}</td>
                      <td className="py-1 text-right tabular">{formatSize(r.bytes)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </Row>
      </Group>
    </>
  );
}
