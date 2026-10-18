import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { Button } from "../components/ui";
import type { CleanupReport, Transport } from "../transport";
import { fileStem, formatSize } from "./format";

/**
 * What automatic cleanup did (spec 019 FR-020, FR-021): the one-time notice
 * naming unsaved crawls that will go to the Trash at the next start, or how
 * much it removed. Links to Manage crawls and Settings › Storage.
 */
export function CleanupNotice({ transport, onManage, onSettings }: { transport: Transport; onManage: () => void; onSettings: () => void }) {
  const [report, setReport] = useState<CleanupReport | null>(null);
  useEffect(() => {
    const off = transport.onCleanupDone(setReport);
    return () => void off.then((f) => f());
  }, [transport]);
  if (!report) return null;

  const close = () => setReport(null);
  const removed = report.removed.length;
  const scheduled = report.schedule_removed.length;
  const announced = report.announced;
  return (
    <div
      role="status"
      aria-labelledby="cleanup-title"
      className="fixed right-4 bottom-10 z-30 flex w-96 flex-col gap-2 rounded-lg border border-line bg-surface px-4 py-3 text-sm shadow-popover"
    >
      <div className="flex items-center gap-2">
        <Trash2 aria-hidden className="size-4 shrink-0 text-fg-3" />
        <h2 id="cleanup-title" className="m-0 text-base font-semibold">
          {announced.length > 0 && removed + scheduled === 0 ? "Old unsaved crawls will be removed" : "Cleaned up unused files"}
        </h2>
      </div>
      {removed + scheduled + report.leftovers > 0 && (
        <p className="m-0 text-fg-2">
          {removed > 0 && `${removed} unsaved ${removed === 1 ? "crawl" : "crawls"} not used for a while ${removed === 1 ? "was" : "were"} moved to the Trash. `}
          {scheduled > 0 && `${scheduled} older scheduled ${scheduled === 1 ? "run was" : "runs were"} moved to the Trash. `}
          {report.leftovers > 0 && `${report.leftovers} leftover ${report.leftovers === 1 ? "file" : "files"} from earlier crashes ${report.leftovers === 1 ? "was" : "were"} removed. `}
          {formatSize(report.freed_bytes)} freed.
        </p>
      )}
      {announced.length > 0 && (
        <>
          <p className="m-0 text-fg-2">
            SpiderLens moves unsaved crawls you have not opened for a while to the Trash. These will go the next time it starts, unless you save or open them:
          </p>
          <ul className="m-0 list-disc pl-5 text-fg-2">
            {announced.slice(0, 5).map((p) => (
              <li key={p} className="truncate" title={p}>
                {fileStem(p)}
              </li>
            ))}
            {announced.length > 5 && <li>and {announced.length - 5} more</li>}
          </ul>
        </>
      )}
      <div className="flex flex-wrap gap-2 pt-1">
        <Button
          size="sm"
          onClick={() => {
            close();
            onManage();
          }}
        >
          Manage crawls…
        </Button>
        <Button
          size="sm"
          onClick={() => {
            close();
            onSettings();
          }}
        >
          Cleanup settings…
        </Button>
        <Button size="sm" variant="quiet" className="ml-auto" onClick={close}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}
