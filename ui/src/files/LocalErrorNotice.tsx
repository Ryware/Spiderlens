import { HardDrive } from "lucide-react";
import { Button } from "../components/ui";
import type { LocalError, LocalErrorKind } from "../transport";
import { formatSize } from "./format";

export const localErrorTitle: Record<LocalErrorKind, string> = {
  disk_full: "Disk is full",
  disk_low: "Disk space is low",
  drive_gone: "The drive was disconnected",
  permission: "Permission denied",
  other: "Couldn't write the crawl file",
};

/**
 * A disk or write problem paused the crawl (spec 006 US5): what happened,
 * where the file is, and what to do. Never about the website.
 */
export function LocalErrorNotice({ error, onResume, onSaveAs, onKeepPaused }: { error: LocalError; onResume: () => void; onSaveAs: () => void; onKeepPaused: () => void }) {
  return (
    <div role="alert" aria-labelledby="local-error-title" className="flex flex-col gap-2 rounded-lg border border-bad bg-bad-soft px-4 py-3 text-sm">
      <div className="flex items-center gap-2">
        <HardDrive aria-hidden className="size-4 shrink-0 text-bad" />
        <h2 id="local-error-title" className="m-0 text-base font-semibold text-bad">
          {localErrorTitle[error.kind]}
        </h2>
      </div>
      <p className="m-0 text-fg">
        {error.message} The crawl is paused and nothing collected so far is lost. Fix the problem and resume, or save the crawl somewhere else.
      </p>
      <dl className="m-0 flex flex-col gap-0.5 text-fg-2" data-selectable>
        <div className="flex gap-3">
          <dt className="w-20 shrink-0 text-fg-3">Crawl file</dt>
          <dd className="m-0 min-w-0 font-code break-all">{error.path}</dd>
        </div>
        {error.free_bytes !== null && (
          <div className="flex gap-3">
            <dt className="w-20 shrink-0 text-fg-3">Free space</dt>
            <dd className="m-0 tabular">{formatSize(error.free_bytes)}</dd>
          </div>
        )}
      </dl>
      <div className="flex flex-wrap gap-2 pt-1">
        <Button variant="primary" size="sm" onClick={onResume}>
          Resume
        </Button>
        <Button size="sm" onClick={onSaveAs}>
          Save As elsewhere…
        </Button>
        <Button size="sm" variant="quiet" onClick={onKeepPaused}>
          Keep paused
        </Button>
      </div>
    </div>
  );
}
