import { useEffect, useState } from "react";
import type { RetryPreview, Transport } from "../transport";
import { Button, Modal, Notice } from "../components/ui";
import { retryPhases, type CrawlRun } from "./useCrawlRun";

/** Whether "Retry failed URLs" can run now, and why not (spec 004 FR-034). */
export interface RetryAvailability {
  available: boolean;
  /** URLs it would retry. */
  total: number;
  reason: string | null;
}

const RUNNING = "Wait until the crawl is paused or finished.";
const NO_FILE = "Run or open a crawl first.";

/**
 * Asks the engine whether the open crawl has failures worth retrying, again
 * whenever the crawl settles. While a crawl runs the answer is known without
 * asking.
 */
export function useRetryAvailability(transport: Transport, run: CrawlRun): RetryAvailability {
  const settled = retryPhases.includes(run.phase);
  const [preview, setPreview] = useState<RetryPreview | null>(null);
  useEffect(() => {
    setPreview(null);
    if (!run.path || !settled) return;
    let live = true;
    transport.retryPreview(run.path).then(
      (p) => live && setPreview(p),
      (e) => live && setPreview({ total: 0, by_class: [], available: false, reason: String(e) }),
    );
    return () => {
      live = false;
    };
  }, [transport, run.path, settled, run.report]);
  if (!run.path) return { available: false, total: 0, reason: NO_FILE };
  if (!settled) return { available: false, total: 0, reason: RUNNING };
  if (!preview) return { available: false, total: 0, reason: null };
  return { available: preview.available, total: preview.total, reason: preview.reason };
}

/**
 * Confirmation for "Retry failed URLs" (spec 004 US5.1, FR-031): how many
 * URLs, by error class, before anything changes. Counts are read fresh when
 * it opens.
 */
export function RetryDialog({
  open,
  onOpenChange,
  transport,
  path,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  path: string | null;
  onConfirm: () => void;
}) {
  const [preview, setPreview] = useState<RetryPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open || !path) return;
    setPreview(null);
    setError(null);
    transport.retryPreview(path).then(setPreview, (e) => setError(String(e)));
  }, [open, path, transport]);

  const urls = (n: number) => `${n.toLocaleString()} ${n === 1 ? "URL" : "URLs"}`;
  return (
    <Modal open={open} onOpenChange={onOpenChange} title="Retry failed URLs">
      {error && (
        <Notice kind="bad" role="alert">
          Could not count the failed URLs: {error}
        </Notice>
      )}
      {!preview && !error && <p className="m-0 text-sm text-fg-3">Counting failed URLs…</p>}
      {preview && !preview.available && <Notice kind="info">{preview.reason ?? "There is nothing to retry."}</Notice>}
      {preview?.available && (
        <>
          <p className="m-0 text-sm text-fg-2">
            {urls(preview.total)} failed for a reason that may have passed. Only these are requested again, each with a fresh set of attempts. Other errors stay as
            they are.
          </p>
          <div className="overflow-hidden rounded-lg border border-line">
            <table aria-label="Failed URLs by error" className="w-full table-fixed border-collapse text-sm">
              <tbody className="divide-y divide-line">
                {preview.by_class.map((c) => (
                  <tr key={c.class} data-class={c.class}>
                    <td className="px-3 py-1.5">{c.label}</td>
                    <td className="w-20 px-3 py-1.5 text-right tabular">{c.count.toLocaleString()}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <div className="flex justify-end gap-2">
        <Button onClick={() => onOpenChange(false)}>{preview?.available ? "Cancel" : "Close"}</Button>
        {preview?.available && (
          <Button
            variant="primary"
            onClick={() => {
              onOpenChange(false);
              onConfirm();
            }}
          >
            Retry {urls(preview.total)}
          </Button>
        )}
      </div>
    </Modal>
  );
}
