// The crawl report (spec 010 US1, US2): health score, issues and sections,
// built by the engine from the saved crawl. Every figure with a target opens
// the matching rows in the results table.

import { useState } from "react";
import { Download, FileDown, RefreshCw } from "lucide-react";
import { Badge, Button, EmptyState, Notice, Switch } from "../components/ui";
import type { FileOptions } from "../generated/FileOptions";
import type { Target } from "../generated/Target";
import type { Transport } from "../transport";
import { BulkExportDialog, ExportReportDialog } from "./ExportDialogs";
import { reportName } from "./format";
import { ScoreCard } from "./ScoreCard";
import { SectionView } from "./SectionView";
import { useReport } from "./useReport";

export function ReportView({
  transport,
  path,
  running,
  onOpen,
  openOnFinish,
  onOpenOnFinish,
  reportFile,
  onReportFile,
}: {
  transport: Transport;
  path: string;
  running: boolean;
  onOpen: (t: Target) => void;
  openOnFinish: boolean;
  onOpenOnFinish: (v: boolean) => void;
  reportFile: FileOptions;
  onReportFile: (f: FileOptions) => void;
}) {
  const { report, error, loading, reload } = useReport(transport, path, running);
  const [saveOpen, setSaveOpen] = useState(false);
  const [bulkOpen, setBulkOpen] = useState(false);
  const name = reportName(report?.meta.start_url ?? null);

  return (
    <section aria-label="Crawl report" className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-4xl flex-col gap-6 p-6">
        <header className="flex flex-wrap items-center gap-2">
          <h1 className="m-0 min-w-0 flex-1 truncate text-lg font-semibold tracking-tight">Crawl report</h1>
          <Button size="sm" variant="quiet" icon aria-label="Refresh report" disabled={loading} onClick={() => void reload()}>
            <RefreshCw aria-hidden className="size-3.5" />
          </Button>
          <Button size="sm" disabled={!report} onClick={() => setSaveOpen(true)}>
            <FileDown aria-hidden className="size-3.5" />
            Save report…
          </Button>
          <Button size="sm" disabled={!report || running} title={running ? "Available when the crawl stops" : undefined} onClick={() => setBulkOpen(true)}>
            <Download aria-hidden className="size-3.5" />
            Bulk export…
          </Button>
        </header>
        {report?.meta.start_url && (
          <p className="-mt-4 m-0 truncate text-sm text-fg-3" dir="auto">
            {report.meta.start_url}
            {report.meta.custom_issue_settings && (
              <>
                {" "}
                <Badge tone="info">Custom issue settings</Badge>
              </>
            )}
          </p>
        )}

        {error && (
          <Notice kind="bad" role="alert">
            {error}
          </Notice>
        )}
        {!report && !error && <EmptyState title="Building the report…" />}
        {report && (
          <>
            {report.provisional && <Notice>The crawl is still running: figures are provisional and refresh as it goes.</Notice>}
            {report.incomplete && (
              <Notice kind="warn">
                {report.incomplete.not_crawled.toLocaleString()} URLs were not crawled ({report.incomplete.reasons.map((r) => `${r.label}: ${r.count.toLocaleString()}`).join(", ")}), so this report covers part of the site.
              </Notice>
            )}
            <ScoreCard report={report} onOpen={onOpen} />
            {report.sections.map((s) => (
              <SectionView key={s.id} section={s} onOpen={onOpen} />
            ))}
          </>
        )}
        <label className="flex items-center gap-2 border-t border-line pt-4 text-sm text-fg-2">
          <Switch checked={openOnFinish} onChange={onOpenOnFinish} label="Show the report when a crawl finishes" />
          Show the report when a crawl finishes
        </label>
      </div>
      <ExportReportDialog open={saveOpen} onOpenChange={setSaveOpen} transport={transport} path={path} name={name} initial={reportFile} onRemember={onReportFile} />
      <BulkExportDialog open={bulkOpen} onOpenChange={setBulkOpen} transport={transport} path={path} name={name} />
    </section>
  );
}
