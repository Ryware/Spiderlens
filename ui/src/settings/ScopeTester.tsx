import { useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { ScopeVerdict, Transport } from "../transport";
import { Badge, Button, Notice, Switch, Textarea } from "../components/ui";

/**
 * What the crawler would do with each pasted URL under every scope setting
 * at once (spec 003 FR-016): patterns, subdomains, folder, query parameters,
 * URL length and, when asked, robots.txt. Requests nothing else.
 */
export function ScopeTester({ transport, config, startUrl }: { transport: Transport; config: CrawlConfig; startUrl: string | null }) {
  const [samples, setSamples] = useState("");
  const [robots, setRobots] = useState(false);
  const [rows, setRows] = useState<ScopeVerdict[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    if (!startUrl) return;
    setBusy(true);
    setError(null);
    try {
      setRows(await transport.scopeTest(config, startUrl, samples.split("\n"), robots));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!startUrl) return <p className="m-0 text-sm text-fg-3">Enter a start URL first: whether a URL is in scope depends on where the crawl starts.</p>;

  return (
    <div className="flex w-full flex-col gap-2">
      <Textarea aria-label="URLs to test" rows={3} placeholder="Paste URLs or paths, one per line" value={samples} onChange={(e) => setSamples(e.target.value)} />
      <div className="flex items-center gap-3">
        <Button size="sm" onClick={run} disabled={busy || samples.trim() === ""}>
          {busy ? "Testing…" : "Test URLs"}
        </Button>
        <label className="flex items-center gap-2 text-sm text-fg-2">
          <Switch label="Check robots.txt" checked={robots} onChange={setRobots} />
          Check robots.txt (one request per site)
        </label>
      </div>
      {error && <Notice kind="bad">{error}</Notice>}
      {rows.length > 0 && (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left text-xs text-fg-3">
              <th className="py-1 pr-2 font-medium">URL</th>
              <th className="py-1 font-medium">Would be</th>
              {rows.some((r) => r.segment) && <th className="py-1 pl-2 font-medium">Segment</th>}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} className="border-t border-line" data-verdict={r.reason ?? "crawl"}>
                <td className="max-w-60 truncate py-1 pr-2 font-code" title={r.url ?? r.input} data-selectable>
                  {r.url ?? r.input}
                </td>
                <td className="py-1">
                  {r.would_crawl ? (
                    <Badge tone="ok">{r.robots_checked ? "Crawled" : "Crawled (robots.txt not checked)"}</Badge>
                  ) : (
                    <span className="flex flex-wrap items-center gap-1.5">
                      <Badge tone={r.reason === "external" ? "neutral" : "warn"}>{r.label}</Badge>
                      {r.detail && <span className="font-code text-fg-2">{r.detail}</span>}
                    </span>
                  )}
                </td>
                {rows.some((r) => r.segment) && (
                  <td className="py-1 pl-2" data-testid="scope-segment">
                    {r.segment ?? <span className="text-fg-3">No segment</span>}
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
