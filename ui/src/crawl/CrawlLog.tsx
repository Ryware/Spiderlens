import { useEffect, useMemo, useState } from "react";
import { Chip, cn } from "../components/ui";
import type { CrawlLogEntry, Transport } from "../transport";

/** Words for the engine's log kinds (spec 004 FR-035). */
const kinds: Record<string, string> = {
  retry: "Retry",
  retry_after_capped: "Wait shortened",
  failure: "Failed",
  host_paused: "Site paused",
  host_resumed: "Site resumed",
  host_given_up: "Site given up",
  local_problem: "Crawl paused",
  manual_retry: "Retry failed URLs",
};

const groups: { id: string; label: string; kinds: string[] | null }[] = [
  { id: "all", label: "All", kinds: null },
  { id: "retries", label: "Retries", kinds: ["retry", "retry_after_capped", "manual_retry"] },
  { id: "failures", label: "Failures", kinds: ["failure"] },
  { id: "sites", label: "Sites", kinds: ["host_paused", "host_resumed", "host_given_up"] },
  { id: "crawl", label: "Crawl", kinds: ["local_problem"] },
];

/** Entries shown; the newest first. */
const LIMIT = 1000;
/** How often the log is read again while the crawl runs. */
const REFRESH_MS = 2000;

const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

/**
 * What happened during the crawl that the results table does not show:
 * retries, failures, site pauses and resumes, and the crawl pausing itself
 * (spec 004 FR-035). Read from the crawl file, so it covers earlier runs too.
 */
export function CrawlLog({ transport, path, live }: { transport: Transport; path: string; live: boolean }) {
  const [entries, setEntries] = useState<CrawlLogEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [group, setGroup] = useState("all");

  useEffect(() => {
    let gone = false;
    const load = () =>
      transport.crawlLog(path, LIMIT).then(
        (e) => {
          if (!gone) {
            setEntries(e);
            setError(null);
          }
        },
        (e: unknown) => !gone && setError(String(e)),
      );
    void load();
    const timer = live ? setInterval(() => void load(), REFRESH_MS) : undefined;
    return () => {
      gone = true;
      clearInterval(timer);
    };
  }, [transport, path, live]);

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const g of groups) c[g.id] = (entries ?? []).filter((e) => !g.kinds || g.kinds.includes(e.kind)).length;
    return c;
  }, [entries]);
  const shown = useMemo(() => {
    const g = groups.find((x) => x.id === group)!;
    return (entries ?? []).filter((e) => !g.kinds || g.kinds.includes(e.kind));
  }, [entries, group]);

  return (
    <section aria-label="Crawl log" className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5">
        <h1 className="m-0 text-base font-semibold">Crawl log</h1>
        <div role="toolbar" aria-label="Log filters" className="flex flex-wrap items-center gap-0.5">
          {groups.map((g) => (
            <Chip key={g.id} label={g.label} count={entries ? counts[g.id] : null} active={g.id === group} onClick={() => setGroup(g.id)} />
          ))}
        </div>
        {entries && entries.length >= LIMIT && <span className="ml-auto text-xs text-fg-3">Newest {LIMIT.toLocaleString()} entries</span>}
      </div>
      {error ? (
        <p className="m-0 p-4 text-sm text-bad">The log could not be read: {error}</p>
      ) : entries === null ? (
        <p className="m-0 p-4 text-sm text-fg-3" aria-busy>
          Reading the log…
        </p>
      ) : shown.length === 0 ? (
        <p className="m-0 p-4 text-sm text-fg-3">
          {entries.length === 0 ? "Nothing to report: no retries, failures or site pauses in this crawl." : "No entries of this kind."}
        </p>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full table-fixed border-collapse text-sm">
            <thead className="sticky top-0 bg-surface text-left text-xs text-fg-3">
              <tr>
                <th className="w-24 px-3 py-1.5 font-medium">Time</th>
                <th className="w-36 px-3 py-1.5 font-medium">Event</th>
                <th className="w-48 px-3 py-1.5 font-medium">Site or URL</th>
                <th className="px-3 py-1.5 font-medium">Details</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {shown.map((e, i) => (
                <tr key={`${e.at_ms}-${i}`}>
                  <td className="px-3 py-1 tabular text-fg-2">{time(e.at_ms)}</td>
                  <td className={cn("px-3 py-1", (e.kind === "failure" || e.kind === "host_given_up" || e.kind === "local_problem") && "text-bad")}>{kinds[e.kind] ?? e.kind}</td>
                  <td className="max-w-0 truncate px-3 py-1 font-code" title={e.url ?? e.host ?? undefined}>
                    {e.url ?? e.host ?? "–"}
                  </td>
                  <td className="px-3 py-1 text-fg-2">{e.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
