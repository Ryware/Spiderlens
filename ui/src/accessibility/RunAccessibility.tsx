// "Run accessibility checks" on a saved crawl (spec 026 T063): confirm, run the
// static checks on the stored HTML, then show what they found.

import { useEffect, useState } from "react";
import type { RerunSummary } from "../generated/RerunSummary";
import type { Transport } from "../transport";
import { Button, Modal, Notice } from "../components/ui";

type Phase = { kind: "confirm" } | { kind: "running" } | { kind: "done"; summary: RerunSummary } | { kind: "failed"; error: string };

/** "Checked 40 pages, 12 with findings, 3 without stored HTML". */
export function rerunText(s: RerunSummary): string {
  const pages = `${s.pages.toLocaleString()} ${s.pages === 1 ? "page" : "pages"}`;
  return `Checked ${pages}, ${s.with_findings.toLocaleString()} with findings, ${s.skipped_no_html.toLocaleString()} without stored HTML`;
}

export function RunAccessibilityDialog({
  open,
  onOpenChange,
  transport,
  path,
  onDone,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  path: string | null;
  /** Called after the checks finish, so the results can reload. */
  onDone: () => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "confirm" });
  useEffect(() => {
    if (open) setPhase({ kind: "confirm" });
  }, [open]);

  const run = () => {
    if (!path) return;
    setPhase({ kind: "running" });
    transport.accessibilityRun(path).then(
      (summary) => {
        setPhase({ kind: "done", summary });
        onDone();
      },
      (e) => setPhase({ kind: "failed", error: String(e) }),
    );
  };
  const busy = phase.kind === "running";
  return (
    <Modal open={open} onOpenChange={(o) => !busy && onOpenChange(o)} title="Run accessibility checks">
      {phase.kind === "confirm" && (
        <>
          <p className="m-0 text-sm text-fg-2">Re-run the static accessibility checks on this crawl's stored HTML? Earlier static results are replaced.</p>
          <div className="flex justify-end gap-2">
            <Button onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button variant="primary" onClick={run}>
              Run checks
            </Button>
          </div>
        </>
      )}
      {busy && (
        <p className="m-0 text-sm text-fg-3" role="status">
          Checking stored pages…
        </p>
      )}
      {phase.kind === "done" && (
        <>
          <p className="m-0 text-sm text-fg-2" role="status">
            {rerunText(phase.summary)}
          </p>
          <div className="flex justify-end">
            <Button variant="primary" onClick={() => onOpenChange(false)}>
              Close
            </Button>
          </div>
        </>
      )}
      {phase.kind === "failed" && (
        <>
          <Notice kind="bad" role="alert">
            The checks could not run: {phase.error}
          </Notice>
          <div className="flex justify-end">
            <Button onClick={() => onOpenChange(false)}>Close</Button>
          </div>
        </>
      )}
    </Modal>
  );
}
