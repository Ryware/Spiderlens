// Two pages' main content side by side, changed words marked (US2, FR-020).
// Aligned paragraphs share a row, so both sides scroll together.

import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import type { Comparison } from "../generated/Comparison";
import type { DiffParagraph } from "../generated/DiffParagraph";
import type { Transport } from "../transport";
import { Badge, Button, EmptyState, Kbd, Notice, Sheet, SheetClose, Switch } from "../components/ui";
import type { CompareRequest } from "./compare";

type Row = [DiffParagraph | null, DiffParagraph | null];

/** Pairs aligned paragraphs on one row; the rest get a row of their own. */
export function alignRows(a: DiffParagraph[], b: DiffParagraph[]): Row[] {
  const rows: Row[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    const pa = i < a.length ? a[i] : null;
    const pb = j < b.length ? b[j] : null;
    if (pa && pb && pa.pair === j) {
      rows.push([pa, pb]);
      i++;
      j++;
    } else if (pa && (pa.pair === null || !pb || pa.pair < j)) {
      rows.push([pa, null]);
      i++;
    } else if (pb && (pb.pair === null || !pa || pb.pair < i)) {
      rows.push([null, pb]);
      j++;
    } else if (pa && pa.pair !== null && pa.pair > j) {
      rows.push([null, pb]);
      j++;
    } else {
      rows.push([pa, null]);
      i++;
    }
  }
  return rows;
}

function Paragraph({ p }: { p: DiffParagraph | null }) {
  if (!p) return <div />;
  return (
    <p className="m-0 text-base leading-relaxed break-words text-fg">
      {p.spans.map((s, i) =>
        s.op === "changed" ? (
          <mark key={i} data-op="changed" className="rounded-sm bg-diff-changed text-fg">
            {s.text}
          </mark>
        ) : s.op === "excluded" ? (
          <span key={i} data-op="excluded" className="text-fg-3 italic">
            {s.text}
          </span>
        ) : (
          <span key={i}>{s.text}</span>
        ),
      )}
    </p>
  );
}

export function CompareSheet({
  transport,
  path,
  request,
  onRequest,
  onClose,
}: {
  transport: Transport;
  path: string;
  request: CompareRequest | null;
  onRequest: (r: CompareRequest) => void;
  onClose: () => void;
}) {
  const [full, setFull] = useState(false);
  const [cmp, setCmp] = useState<Comparison | null>(null);
  const [error, setError] = useState<string | null>(null);
  const target = request ? request.targets[request.index] : undefined;

  useEffect(() => {
    if (!request || !target) return;
    let off = false;
    setCmp(null);
    setError(null);
    transport.comparePages(path, request.base.id, target.id, full).then(
      (c) => !off && setCmp(c),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, request?.base.id, target?.id, full]);

  const step = (d: number) => {
    if (!request) return;
    const index = request.index + d;
    if (index >= 0 && index < request.targets.length) onRequest({ ...request, index });
  };
  const stepRef = useRef(step);
  stepRef.current = step;

  // [ and ] work wherever focus is while the sheet is open (the sheet's own
  // container may hold focus, outside the content).
  const open = !!request && !!target;
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "[" && e.key !== "]") return;
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
      e.preventDefault();
      stepRef.current(e.key === "]" ? 1 : -1);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  if (!request || !target) return null;
  const sim = cmp?.sim ?? target.sim;
  const rows = cmp?.a && cmp.b ? alignRows(cmp.a.paragraphs, cmp.b.paragraphs) : [];

  return (
    <Sheet open onOpenChange={(o) => !o && onClose()} title="Compare pages" className="h-160 w-275 sheet-max-w">
      <div className="flex min-h-0 flex-1 flex-col">
        <header className="flex items-center gap-3 border-b border-line px-4 py-3">
          <h2 className="m-0 text-md font-semibold">Compare pages</h2>
          {sim !== null && <Badge tone={sim === 100 ? "warn" : "info"}>{sim === 100 ? "Exact duplicate" : `${sim}% similar`}</Badge>}
          <div className="ml-auto flex items-center gap-2 text-sm text-fg-2">
            <label className="flex items-center gap-2">
              <Switch label="Show the whole page" checked={full} onChange={setFull} />
              Whole page
            </label>
            {request.targets.length > 1 && (
              <span className="flex items-center gap-1">
                <Button size="sm" variant="quiet" icon aria-label="Previous match" disabled={request.index === 0} onClick={() => step(-1)}>
                  <ChevronLeft aria-hidden className="size-4" />
                </Button>
                <span className="tabular" aria-live="polite">
                  {request.index + 1} of {request.targets.length}
                </span>
                <Button size="sm" variant="quiet" icon aria-label="Next match" disabled={request.index === request.targets.length - 1} onClick={() => step(1)}>
                  <ChevronRight aria-hidden className="size-4" />
                </Button>
                <span className="ml-1 flex items-center gap-0.5 text-fg-3">
                  <Kbd keys="[" />
                  <Kbd keys="]" />
                </span>
              </span>
            )}
            <SheetClose render={<Button size="sm" variant="quiet" icon aria-label="Close" />}>
              <X aria-hidden className="size-4" />
            </SheetClose>
          </div>
        </header>
        <div className="grid grid-cols-2 gap-6 border-b border-line px-4 py-2 text-sm">
          {[request.base.address, target.address].map((a, i) => (
            <div key={i} className="min-w-0 truncate font-medium text-fg" title={a} data-selectable>
              {a}
            </div>
          ))}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3" data-selectable>
          {error ? (
            <Notice kind="bad">{error}</Notice>
          ) : !cmp ? (
            <div className="text-sm text-fg-3">Loading…</div>
          ) : cmp.unavailable ? (
            <EmptyState title="The text is not available">
              {cmp.unavailable === "html_not_stored"
                ? "This crawl did not keep page HTML, so only the similarity is known. Turn on Store page HTML in Settings › Page data and crawl again to compare the text."
                : cmp.unavailable}
            </EmptyState>
          ) : (
            <div className="grid grid-cols-2 gap-x-6 gap-y-3" role="table" aria-label="Side by side comparison">
              {rows.map(([a, b], i) => (
                <div key={i} role="row" className="contents">
                  <div role="cell">
                    <Paragraph p={a} />
                  </div>
                  <div role="cell">
                    <Paragraph p={b} />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
        <footer className="flex items-center gap-4 border-t border-line px-4 py-2 text-xs text-fg-3">
          <span className="flex items-center gap-1.5">
            <mark className="rounded-sm bg-diff-changed px-1 text-fg">word</mark> only on this side
          </span>
          {full && <span className="italic">Grey text is left out of the comparison</span>}
        </footer>
      </div>
    </Sheet>
  );
}
