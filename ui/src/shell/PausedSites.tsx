import { CirclePause } from "lucide-react";
import { formatElapsed, type CrawlRun } from "../crawl/useCrawlRun";
import type { PausedHost } from "../transport";
import { Badge, Button, Popover } from "../components/ui";

/** example.com for https://example.com:443; the port only when it is unusual. */
export function siteName(hostKey: string): string {
  const m = /^(https?):\/\/(.+?)(?::(\d+))?$/.exec(hostKey);
  if (!m) return hostKey;
  const [, scheme, host, port] = m;
  const usual = !port || (scheme === "https" && port === "443") || (scheme === "http" && port === "80");
  return usual ? host : `${host}:${port}`;
}

/** Time left on a pause, or what the site is doing instead. */
function pauseState(h: PausedHost) {
  if (h.given_up) return <Badge tone="bad">Given up</Badge>;
  if (h.probing || h.seconds_left === 0) return <span className="text-fg-2">Testing…</span>;
  return <span className="text-fg-2">{formatElapsed(h.seconds_left * 1000)}</span>;
}

/**
 * Status bar control for sites the crawl has paused because they kept failing
 * (spec 004 FR-028): a count that opens the list, each with its reason, the
 * time left and "Resume now" (US4.5).
 */
export function PausedSites({ run }: { run: CrawlRun }) {
  const hosts = run.pausedHosts;
  if (hosts.length === 0) return null;
  const paused = hosts.filter((h) => !h.given_up).length;
  const gaveUp = hosts.length - paused;
  const label = [paused > 0 && `${paused} ${paused === 1 ? "site" : "sites"} paused`, gaveUp > 0 && `${gaveUp} given up`].filter(Boolean).join(" · ");
  return (
    <Popover
      title="Paused sites"
      side="top"
      trigger={
        <button type="button" data-figure="paused" className="inline-flex h-5 items-center gap-1 rounded-sm px-1 text-warn hover:bg-surface-2">
          <CirclePause aria-hidden className="size-3" />
          <span className="font-medium">{label}</span>
        </button>
      }
    >
      <ul aria-label="Paused sites" className="m-0 flex max-h-64 list-none flex-col divide-y divide-line overflow-y-auto p-0">
        {hosts.map((h) => (
          <li key={h.host} data-host={h.host} className="flex items-center gap-3 px-3 py-2">
            <div className="min-w-0 flex-1">
              <div className="truncate font-code text-sm text-fg" title={h.host}>
                {siteName(h.host)}
              </div>
              <div className="truncate text-xs text-fg-3" title={h.reason}>
                {h.reason}
              </div>
            </div>
            <span className="shrink-0 text-xs tabular" data-pause-state>
              {pauseState(h)}
            </span>
            {!h.given_up && (
              <Button size="sm" disabled={h.probing} onClick={() => run.resumeHost(h.host)} aria-label={`Resume ${siteName(h.host)} now`}>
                Resume now
              </Button>
            )}
          </li>
        ))}
      </ul>
      <p className="m-0 border-t border-line px-3 py-2 text-xs text-fg-3">Other sites keep being crawled. A paused site gets one test request when its time is up.</p>
    </Popover>
  );
}
