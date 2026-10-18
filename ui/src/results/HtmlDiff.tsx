// Raw against rendered HTML (spec 014 FR-028, US3 scenario 6): the
// differences between the two normalised versions, side by side, with next
// and previous difference and a copy button for each side.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronUp, Copy, FileX } from "lucide-react";
import type { DiffHunk } from "../generated/DiffHunk";
import type { HtmlDiff } from "../generated/HtmlDiff";
import type { HtmlVersion } from "../generated/HtmlVersion";
import type { Transport } from "../transport";
import { Button, EmptyState, Notice, Sheet, SheetClose, cn } from "../components/ui";
import { formatBytes } from "./format";

/** Differences fetched at a time. */
export const HUNKS_PER_FETCH = 200;

type Load = { state: "loading" } | { state: "error"; message: string } | { state: "done"; diff: HtmlDiff; hunks: DiffHunk[] };

const kindLabel: Record<DiffHunk["kind"], string> = {
  added: "Only in the rendered HTML",
  removed: "Only in the raw HTML",
  changed: "Changed by JavaScript",
};

/** One side of a difference: its lines, or a placeholder when this side has none. */
function Side({ label, start, lines, tone }: { label: string; start: number; lines: string[]; tone: "bad" | "ok" }) {
  return (
    <div className={cn("min-w-0 flex-1 font-mono text-xs", lines.length > 0 && (tone === "bad" ? "bg-bad-soft" : "bg-ok-soft"))} aria-label={label}>
      {lines.length === 0 ? (
        <div className="px-2 py-0.5 text-fg-3 italic">nothing</div>
      ) : (
        lines.map((l, i) => (
          <div key={i} className="flex gap-2 px-2 py-0.5">
            <span className="w-8 shrink-0 text-right text-fg-3 tabular select-none">{start + i + 1}</span>
            <span className="min-w-0 break-all whitespace-pre-wrap">{l}</span>
          </div>
        ))
      )}
    </div>
  );
}

function Hunk({ hunk, index, current }: { hunk: DiffHunk; index: number; current: boolean }) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    // jsdom has no scrollIntoView.
    if (current) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [current]);
  return (
    <section ref={ref} data-hunk={index} aria-current={current || undefined} aria-label={`Difference ${index + 1}: ${kindLabel[hunk.kind]}`} className={cn("flex flex-col border-b border-line", current && "ring-2 ring-accent ring-inset")}>
      <div className="px-3 pt-1.5 text-xs text-fg-3">{kindLabel[hunk.kind]}</div>
      {hunk.context_before.length > 0 && (
        <div className="px-3 font-mono text-xs text-fg-3" aria-hidden>
          {hunk.context_before.map((l, i) => (
            <div key={i} className="truncate">
              {l}
            </div>
          ))}
        </div>
      )}
      <div className="flex gap-px pb-1.5">
        <Side label="Raw HTML" start={hunk.raw_start} lines={hunk.raw_lines} tone="bad" />
        <Side label="Rendered HTML" start={hunk.rendered_start} lines={hunk.rendered_lines} tone="ok" />
      </div>
    </section>
  );
}

export function HtmlDiffSheet({ transport, path, id, url, open, onOpenChange }: { transport: Transport; path: string; id: number; url: string; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [cur, setCur] = useState(0);
  const [copied, setCopied] = useState<string | null>(null);
  const [more, setMore] = useState(false);

  useEffect(() => {
    if (!open) return;
    let off = false;
    setLoad({ state: "loading" });
    setCur(0);
    setCopied(null);
    transport.urlHtmlDiff(path, id, 0, HUNKS_PER_FETCH).then(
      (diff) => !off && setLoad({ state: "done", diff, hunks: diff.hunks }),
      (e) => !off && setLoad({ state: "error", message: e instanceof Error ? e.message : String(e) }),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, open]);

  const hunks = load.state === "done" ? load.hunks : [];
  const total = load.state === "done" ? load.diff.total_hunks : 0;
  const step = (d: number) => hunks.length > 0 && setCur((c) => (c + d + hunks.length) % hunks.length);
  const showMore = () => {
    if (load.state !== "done") return;
    setMore(true);
    transport.urlHtmlDiff(path, id, hunks.length, HUNKS_PER_FETCH).then(
      (diff) => {
        setLoad({ state: "done", diff, hunks: [...hunks, ...diff.hunks] });
        setMore(false);
      },
      (e) => {
        setLoad({ state: "error", message: String(e) });
        setMore(false);
      },
    );
  };
  // Copies one side's HTML as stored, not the normalised lines.
  const copy = (version: HtmlVersion) =>
    transport.urlHtml(path, id, version).then(
      (page) => {
        if (page.html === null) return setCopied(`The ${version} HTML is not stored.`);
        const done = navigator.clipboard?.writeText(page.html);
        if (!done) return setCopied("Could not copy.");
        return done.then(
          () => setCopied(`Copied the ${version} HTML.`),
          () => setCopied("Could not copy."),
        );
      },
      () => setCopied("Could not copy."),
    );

  let body: ReactNode;
  if (load.state === "loading") {
    body = (
      <div className="flex flex-1 items-center justify-center text-sm text-fg-3" aria-busy>
        Loading…
      </div>
    );
  } else if (load.state === "error") {
    body = (
      <div className="flex-1 p-4">
        <Notice kind="bad" role="alert">
          {load.message}
        </Notice>
      </div>
    );
  } else if (hunks.length === 0) {
    body = (
      <EmptyState icon={<FileX className="size-6" />} title="No differences">
        After the markup is normalised (one tag per line, attributes sorted, whitespace collapsed) the raw and rendered HTML are the same.
      </EmptyState>
    );
  } else {
    body = (
      <div className="min-h-0 flex-1 overflow-y-auto" role="region" aria-label="Differences">
        <div className="sticky top-0 z-10 flex border-b border-line bg-chrome text-xs font-medium text-fg-3">
          <div className="flex-1 px-3 py-1">Raw HTML</div>
          <div className="flex-1 px-3 py-1">Rendered HTML</div>
        </div>
        {hunks.map((h, i) => (
          <Hunk key={i} hunk={h} index={i} current={i === cur} />
        ))}
        {hunks.length < total && (
          <div className="p-3">
            <Button size="sm" variant="quiet" disabled={more} onClick={showMore}>
              Show more ({(total - hunks.length).toLocaleString()} left)
            </Button>
          </div>
        )}
      </div>
    );
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Raw vs rendered HTML" className="h-180 w-280 sheet-max-w">
      <div className="flex min-h-0 flex-1 flex-col">
        <header className="flex flex-col gap-0.5 border-b border-line px-4 pt-4 pb-3">
          <h2 className="m-0 text-md font-semibold">Raw vs rendered HTML</h2>
          <p className="m-0 truncate text-sm text-fg-3" title={url} data-selectable>
            {url}
          </p>
        </header>
        <div className="flex items-center gap-2 border-b border-line px-4 py-2">
          <Button icon variant="quiet" aria-label="Previous difference" disabled={hunks.length === 0} onClick={() => step(-1)}>
            <ChevronUp className="size-4" />
          </Button>
          <Button icon variant="quiet" aria-label="Next difference" disabled={hunks.length === 0} onClick={() => step(1)}>
            <ChevronDown className="size-4" />
          </Button>
          <span className="text-sm text-fg-3 tabular" role="status" data-testid="diff-count">
            {load.state === "done" ? (total === 0 ? "No differences" : `Difference ${(cur + 1).toLocaleString()} of ${total.toLocaleString()}`) : ""}
          </span>
          <div className="ml-auto flex items-center gap-2">
            {copied && (
              <span role="status" className="text-sm text-fg-3">
                {copied}
              </span>
            )}
            <Button onClick={() => void copy("raw")}>
              <Copy className="size-3.5" /> Copy raw
            </Button>
            <Button onClick={() => void copy("rendered")}>
              <Copy className="size-3.5" /> Copy rendered
            </Button>
          </div>
        </div>
        {body}
        <footer className="flex items-center gap-2 border-t border-line px-4 py-2">
          <span className="mr-auto text-sm text-fg-3 tabular" data-testid="diff-facts">
            {load.state === "done" ? `Raw ${formatBytes(load.diff.raw_bytes)} · ${load.diff.raw_lines.toLocaleString()} lines, rendered ${formatBytes(load.diff.rendered_bytes)} · ${load.diff.rendered_lines.toLocaleString()} lines (normalised)` : ""}
          </span>
          <SheetClose render={<Button variant="primary" />}>Close</SheetClose>
        </footer>
      </div>
    </Sheet>
  );
}
