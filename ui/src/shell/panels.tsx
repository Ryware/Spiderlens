import { useEffect, useState } from "react";
import { Copy, MousePointerClick } from "lucide-react";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { BuildInfo, Transport } from "../transport";
import { Badge, Button, EmptyState, Kbd, Modal, Notice } from "../components/ui";
import { IssueList } from "../settings/fields";

/** Detail panel (FR-001): explains the selected URL once results exist (spec 007). */
export function DetailPanel() {
  return (
    <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
      <EmptyState icon={<MousePointerClick className="size-6" />} title="Nothing selected">
        Select a URL in the results to see its response, links, headings and issues here.
      </EmptyState>
    </aside>
  );
}

const robotsLabel = { respect: "robots.txt respected", ignore_report: "robots.txt ignored, reported", ignore: "robots.txt ignored" } as const;

/** Live crawl figures (FR-006). They read zero until crawling arrives with spec 003. */
export function StatusBar({ setup }: { setup: CrawlSetup }) {
  const figures: [string, string][] = [
    ["0", "crawled"],
    ["0", "queued"],
    ["0", "errors"],
    ["–", "URLs/s"],
    ["00:00", "elapsed"],
  ];
  const speed = setup.speedPreset === "polite" ? "Polite" : setup.speedPreset === "fast" ? "Fast" : "Custom speed";
  return (
    <footer aria-label="Crawl status" className="flex h-statusbar shrink-0 items-center gap-4 overflow-hidden border-t border-line bg-chrome px-3 text-xs whitespace-nowrap text-fg-3 tabular">
      <span className="flex items-center gap-1.5 text-fg-2">
        <span aria-hidden className="size-1.5 rounded-full bg-fg-3" />
        {setup.ready ? "Ready to start" : "Idle"}
      </span>
      {figures.map(([n, label]) => (
        <span key={label}>
          <span className="font-medium text-fg-2">{n}</span> {label}
        </span>
      ))}
      <span className="ml-auto">
        {setup.loaded ? `${setup.loaded.name} · ` : ""}
        {speed}
        {setup.config ? ` · ${robotsLabel[setup.config.robots.mode]}` : ""}
      </span>
    </footer>
  );
}

/** Main area before any crawl (FR-008): what to do next, and anything to be careful about. */
export function StartState({ setup, onFocusUrl, onOpenSettings }: { setup: CrawlSetup; onFocusUrl: () => void; onOpenSettings: () => void }) {
  const mode = setup.config?.mode ?? "spider";
  const urlIssues = setup.issues.filter((i) => i.path === "input.start_url");
  return (
    <section aria-label="Start" className="flex h-full flex-col items-center justify-center gap-5 overflow-y-auto p-8">
      <div className="flex max-w-md flex-col items-center gap-2 text-center">
        <h1 className="m-0 text-xl font-semibold tracking-tight text-balance">{mode === "spider" ? "Crawl a website" : "Crawl a list of URLs"}</h1>
        <p className="m-0 text-base text-fg-2">
          {mode === "spider"
            ? "Enter a start URL above and press Start. SpiderLens follows links from there and lists every page it finds, with its issues."
            : "Add your URLs, then press Start. Only the URLs in the list are crawled."}
        </p>
      </div>

      {setup.ready && (
        <p role="status" className="m-0 rounded-md bg-ok-soft px-3 py-1.5 font-medium text-ok">
          {setup.ready}
        </p>
      )}
      {mode === "spider" && setup.startUrl !== "" && <IssueList issues={urlIssues} />}

      {setup.warnings.length > 0 && (
        <div aria-label="Less respectful settings" className="flex w-full max-w-md flex-col gap-2 rounded-lg border border-line bg-surface px-4 py-3">
          <div className="flex items-center gap-2">
            <Badge tone="warn">Less respectful settings</Badge>
            <Button size="sm" variant="quiet" className="ml-auto" onClick={onOpenSettings}>
              Review
            </Button>
          </div>
          <IssueList issues={setup.warnings} />
        </div>
      )}

      <div className="flex flex-wrap items-center justify-center gap-x-4 gap-y-1 text-sm text-fg-3">
        <button type="button" className="rounded-sm hover:text-fg-2" onClick={onFocusUrl}>
          Start URL <Kbd keys="Mod+L" />
        </button>
        <span>
          Settings <Kbd keys="Mod+," />
        </span>
        <span>
          Commands <Kbd keys="Mod+K" />
        </span>
      </div>
    </section>
  );
}

/** About (FR-007): version, engine and build, copyable. */
export function AboutDialog({ open, onOpenChange, transport }: { open: boolean; onOpenChange: (o: boolean) => void; transport: Transport }) {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    transport.buildInfo().then(setInfo, (e) => setError(String(e)));
  }, [transport]);
  const text = info ? `SpiderLens ${info.product_version} (engine ${info.engine_version}, build ${info.build_id})` : "";
  return (
    <Modal open={open} onOpenChange={onOpenChange} title="SpiderLens" description="SEO crawler for Windows, macOS and Linux.">
      {error && (
        <Notice kind="bad" role="alert">
          Could not read build information: {error}
        </Notice>
      )}
      {info && (
        <dl className="m-0 grid grid-cols-2 gap-x-4 gap-y-1 text-sm" data-selectable>
          <dt className="text-fg-3">Version</dt>
          <dd className="m-0 tabular" data-testid="product-version">
            {info.product_version}
          </dd>
          <dt className="text-fg-3">Engine</dt>
          <dd className="m-0 tabular" data-testid="engine-version">
            {info.engine_version}
          </dd>
          <dt className="text-fg-3">Build</dt>
          <dd className="m-0 font-code" data-testid="build-id">
            {info.build_id}
          </dd>
        </dl>
      )}
      <div className="flex justify-end gap-2">
        {info && (
          <Button
            onClick={() =>
              navigator.clipboard?.writeText(text).then(
                () => setCopied(true),
                () => setCopied(false),
              )
            }
          >
            <Copy className="size-3.5" /> {copied ? "Copied" : "Copy"}
          </Button>
        )}
        <Button variant="primary" onClick={() => onOpenChange(false)}>
          Close
        </Button>
      </div>
    </Modal>
  );
}
