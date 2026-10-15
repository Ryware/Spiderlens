import { useEffect, useState } from "react";
import { Bar, Button, Input, Modal, Notice, NumberInput, Segmented, Textarea } from "../components/ui";
import type { BulkFormat } from "../generated/BulkFormat";
import type { BulkProgress } from "../generated/BulkProgress";
import type { BulkResult } from "../generated/BulkResult";
import type { FileOptions } from "../generated/FileOptions";
import { EXISTS, type Transport } from "../transport";

export const URLS_PER_ISSUE_MAX = 10_000;

/** Bulk tables in export order, matching crates/results/src/bulk TABLES. */
export const BULK_TABLES = [
  { id: "summary", title: "Crawl summary" },
  { id: "tabs", title: "All tabs" },
  { id: "issues", title: "All issues" },
  { id: "links", title: "All links" },
  { id: "images", title: "All images" },
  { id: "redirect_chains", title: "Redirect chains" },
  { id: "headers", title: "Response headers" },
] as const;

const isExists = (e: unknown) => String(e) === EXISTS;

/** Title, note and URLs per issue, then save as HTML or print to PDF (FR-013 to FR-017). */
export function ExportReportDialog({
  open,
  onOpenChange,
  transport,
  path,
  name,
  initial,
  onRemember,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  path: string;
  name: string;
  initial: FileOptions;
  onRemember: (f: FileOptions) => void;
}) {
  const [file, setFile] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [replace, setReplace] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  useEffect(() => {
    if (open) {
      setFile(initial);
      setError(null);
      setReplace(null);
      setSaved(null);
    }
  }, [open, initial]);
  const valid = Number.isInteger(file.urls_per_issue) && file.urls_per_issue >= 0 && file.urls_per_issue <= URLS_PER_ISSUE_MAX;

  const write = async (dest: string, overwrite: boolean) => {
    setError(null);
    try {
      const w = await transport.exportReportHtml(path, dest, file, overwrite);
      onRemember(file);
      setReplace(null);
      setSaved(w.path);
    } catch (e) {
      if (isExists(e)) setReplace(dest);
      else setError(String(e));
    }
  };
  const save = async () => {
    const dest = await transport.pickReportPath(name);
    if (dest) await write(dest, false);
  };
  const print = async () => {
    setError(null);
    try {
      await transport.printReport(path, file);
      onRemember(file);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Modal open={open} onOpenChange={onOpenChange} title="Save report" description="One HTML file that opens in any browser, with no internet needed. Print it to make a PDF.">
      <label className="flex flex-col gap-1 text-sm">
        Title
        <Input value={file.title} placeholder="Crawl report" onChange={(e) => setFile({ ...file, title: e.target.value })} />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        Note
        <Textarea value={file.note} rows={2} onChange={(e) => setFile({ ...file, note: e.target.value })} />
      </label>
      <label className="flex items-center justify-between gap-3 text-sm">
        URLs listed per issue
        <NumberInput value={file.urls_per_issue} min={0} max={URLS_PER_ISSUE_MAX} onChange={(v) => setFile({ ...file, urls_per_issue: v ?? NaN })} />
      </label>
      {!valid && <Notice kind="bad">Between 0 and {URLS_PER_ISSUE_MAX.toLocaleString()}.</Notice>}
      {replace && (
        <Notice kind="warn" role="alert">
          {replace} already exists.{" "}
          <button type="button" className="text-accent-text underline" onClick={() => void write(replace, true)}>
            Replace it
          </button>
        </Notice>
      )}
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      {saved && <p className="m-0 text-sm text-ok">Saved to {saved}</p>}
      <div className="flex justify-end gap-2">
        <Button variant="quiet" onClick={() => onOpenChange(false)}>
          Close
        </Button>
        <Button disabled={!valid} onClick={() => void print()}>
          Print or PDF…
        </Button>
        <Button variant="primary" disabled={!valid} onClick={() => void save()}>
          Save HTML…
        </Button>
      </div>
    </Modal>
  );
}

/** Many tables at once to a CSV folder or one Excel workbook (FR-018 to FR-025). */
export function BulkExportDialog({
  open,
  onOpenChange,
  transport,
  path,
  name,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  path: string;
  name: string;
}) {
  const [tables, setTables] = useState<string[]>(BULK_TABLES.map((t) => t.id));
  const [format, setFormat] = useState<BulkFormat>("xlsx");
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<BulkProgress | null>(null);
  const [result, setResult] = useState<BulkResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [replace, setReplace] = useState<string | null>(null);

  useEffect(() => {
    const off = transport.onBulkProgress(setProgress);
    return () => void off.then((o) => o());
  }, [transport]);
  useEffect(() => {
    if (open && !running) {
      setResult(null);
      setError(null);
      setReplace(null);
      setProgress(null);
    }
    // Only when the dialog opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const run = async (out: string, overwrite: boolean) => {
    setRunning(true);
    setError(null);
    setReplace(null);
    setResult(null);
    setProgress(null);
    try {
      setResult(await transport.bulkExport(path, { tables, format, out, overwrite }));
    } catch (e) {
      if (isExists(e)) setReplace(out);
      else if (String(e) === "export cancelled") setError("Export cancelled; nothing was written.");
      else setError(String(e));
    } finally {
      setRunning(false);
    }
  };
  const start = async () => {
    const out = await transport.pickBulkPath(name.replace(/-report$/, "-export"), format);
    if (out) await run(out, false);
  };
  const toggle = (id: string) => setTables((t) => (t.includes(id) ? t.filter((x) => x !== id) : BULK_TABLES.map((b) => b.id).filter((b) => b === id || t.includes(b))));
  const rows = result?.tables.reduce((n, t) => n + t.rows, 0) ?? 0;

  return (
    <Modal open={open} onOpenChange={(o) => !running && onOpenChange(o)} title="Bulk export" description="Export many tables at once, read from the saved crawl.">
      <fieldset className="m-0 flex flex-col gap-1 border-0 p-0" disabled={running}>
        <legend className="mb-1 text-sm font-medium">Tables</legend>
        {BULK_TABLES.map((t) => (
          <label key={t.id} className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={tables.includes(t.id)} onChange={() => toggle(t.id)} />
            {t.title}
          </label>
        ))}
      </fieldset>
      <Segmented
        label="Format"
        value={format}
        onChange={setFormat}
        options={[
          { value: "xlsx", label: "Excel workbook" },
          { value: "csv", label: "CSV folder" },
        ]}
      />
      {running && (
        <div className="flex flex-col gap-1 text-sm" role="status">
          <span>{progress ? `${progress.table}: ${progress.rows.toLocaleString()} rows` : "Starting…"}</span>
          <Bar value={progress ? progress.tables_done / Math.max(1, progress.tables_total) : 0} tone="accent" label="Bulk export progress" className="w-full" />
        </div>
      )}
      {replace && (
        <Notice kind="warn" role="alert">
          {replace} already exists.{" "}
          <button type="button" className="text-accent-text underline" onClick={() => void run(replace, true)}>
            Replace it
          </button>
        </Notice>
      )}
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      {result && (
        <div className="flex flex-col gap-1 text-sm" role="status">
          <p className="m-0 text-ok">
            Exported {rows.toLocaleString()} rows in {result.tables.length} tables to {result.files.length === 1 ? result.files[0] : `${result.files.length} files`}.
          </p>
          {result.split_sheets > 0 && <p className="m-0 text-fg-2">{result.split_sheets} tables continue on extra sheets past Excel's row limit.</p>}
          {result.truncated_cells > 0 && <p className="m-0 text-fg-2">{result.truncated_cells.toLocaleString()} cells were cut at Excel's cell limit.</p>}
        </div>
      )}
      <div className="flex justify-end gap-2">
        {running ? (
          <Button onClick={() => void transport.bulkExportCancel()}>Cancel export</Button>
        ) : (
          <>
            <Button variant="quiet" onClick={() => onOpenChange(false)}>
              Close
            </Button>
            <Button variant="primary" disabled={tables.length === 0} onClick={() => void start()}>
              Export…
            </Button>
          </>
        )}
      </div>
    </Modal>
  );
}
