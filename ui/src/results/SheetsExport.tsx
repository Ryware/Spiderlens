// Export the tab's view (its filter, search, sort and visible columns) to a
// new Google spreadsheet, and open a Looker Studio report on it (spec 024
// US6). Nothing is sent until the person presses Export; a view that does
// not fit one spreadsheet says so first (FR-030).

import { useCallback, useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import { Bar, Button, Modal, Notice, Segmented } from "../components/ui";
import type { Overflow } from "../generated/Overflow";
import type { Segment } from "../generated/Segment";
import type { SheetExport } from "../generated/SheetExport";
import type { SheetsPlan } from "../generated/SheetsPlan";
import type { Status } from "../generated/Status";
import type { ViewQuery } from "../generated/ViewQuery";
import type { Transport } from "../transport";

const n = (v: number) => v.toLocaleString();

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

/** The sentence shown when the rows do not fit one spreadsheet. */
export function overflowWarning(p: SheetsPlan): string | null {
  if (p.fits) return null;
  if (p.rows_per_spreadsheet === 0) {
    return `This export has ${n(p.columns)} columns. A spreadsheet holds ${n(p.capacity)} cells, so not even one row fits. Hide some columns first.`;
  }
  return `This export has ${n(p.cells)} cells (${n(p.rows)} rows by ${n(p.columns)} columns). A spreadsheet holds ${n(p.capacity)}. Export the first ${n(p.rows_per_spreadsheet)} rows, or split it across ${n(p.spreadsheets)} spreadsheets.`;
}

export function SheetsExport({
  transport,
  path,
  query,
  columns,
  segments,
  title,
  looker,
  open,
  onOpenChange,
}: {
  transport: Transport;
  path: string;
  query: ViewQuery;
  columns: string[];
  segments?: Segment[];
  /** The tab's name: the worksheet's title and part of the spreadsheet's. */
  title: string;
  /** Open a Looker Studio report as soon as the sheet exists. */
  looker: boolean;
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [plan, setPlan] = useState<SheetsPlan | null>(null);
  const [overflow, setOverflow] = useState<Overflow>("truncate");
  const [sent, setSent] = useState<{ sent: number; total: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<SheetExport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => transport.integrationsStatus().then(setStatus, (e) => setError(message(e))), [transport]);

  useEffect(() => {
    if (!open) return;
    setDone(null);
    setError(null);
    setPlan(null);
    void reload();
    let off = false;
    transport.integrationsSheetsPlan(path, query, columns, segments).then(
      (p) => !off && setPlan(p),
      (e) => !off && setError(message(e)),
    );
    return () => {
      off = true;
    };
    // The view is fixed while the dialog is open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, transport, path]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let off = false;
    transport.onSheetsProgress((p) => setSent(p)).then((u) => (off ? u() : (unlisten = u)));
    return () => {
      off = true;
      unlisten?.();
    };
  }, [transport]);

  const sheets = status?.providers.find((p) => p.provider === "sheets");
  const account = status?.connections.find((c) => c.kind === "google" && c.scopes.includes("sheets")) ?? null;

  const turnOn = async () => {
    if (!sheets) return;
    try {
      await transport.integrationsSettingsSave("sheets", { ...sheets.settings, enabled: true });
      await reload();
    } catch (e) {
      setError(message(e));
    }
  };
  const connect = async () => {
    setError(null);
    try {
      await transport.integrationsConnectGoogle(["sheets"]);
      await reload();
    } catch (e) {
      setError(message(e));
    }
  };
  const run = async () => {
    if (!plan) return;
    setError(null);
    setBusy(true);
    setSent(null);
    try {
      const out = await transport.integrationsExportSheets(
        path,
        query,
        columns,
        segments,
        `SpiderLens - ${title}`,
        title,
        account?.label ?? null,
        plan.fits ? "refuse" : overflow,
      );
      setDone(out);
      if (looker && out.spreadsheets[0]) await transport.integrationsOpenLooker(out.spreadsheets[0]);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const warning = plan ? overflowWarning(plan) : null;
  const blocked = plan !== null && !plan.fits && plan.rows_per_spreadsheet === 0;
  const ready = sheets?.settings.enabled === true && account !== null;

  return (
    <Modal
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      title={looker ? "Open in Looker Studio" : "Export to Google Sheets"}
      description="Creates a new spreadsheet in your Google Drive from the rows and columns shown. SpiderLens can only see files it creates."
    >
      {status && sheets && !sheets.settings.enabled && (
        <div className="flex flex-col gap-2">
          <Notice>Google Sheets is off. {sheets.sends}</Notice>
          <div>
            <Button size="sm" onClick={() => void turnOn()}>Turn on Google Sheets</Button>
          </div>
        </div>
      )}
      {status && sheets?.settings.enabled && account === null && (
        <div className="flex flex-col gap-2">
          <Notice>Connect a Google account that may create spreadsheets (access to files SpiderLens creates only).</Notice>
          <div>
            <Button size="sm" variant="primary" onClick={() => void connect()}>Connect Google for Sheets</Button>
          </div>
        </div>
      )}
      {ready && plan && (
        <p className="m-0 text-sm text-fg">
          {n(plan.rows)} rows by {n(plan.columns)} columns will be sent to a new spreadsheet in {account.label}&apos;s Google Drive.
        </p>
      )}
      {ready && warning && (
        <Notice kind="warn" role="alert">{warning}</Notice>
      )}
      {ready && plan && !plan.fits && !blocked && !done && (
        <Segmented<Overflow>
          label="When it does not fit"
          value={overflow}
          onChange={setOverflow}
          options={[
            { value: "truncate", label: `First ${n(plan.rows_per_spreadsheet)} rows` },
            { value: "split", label: `Split into ${n(plan.spreadsheets)} spreadsheets` },
          ]}
        />
      )}
      {busy && sent && (
        <div role="status" aria-label="Export progress" className="flex items-center gap-3 text-sm text-fg-2">
          <Bar value={sent.total ? sent.sent / sent.total : 0} tone="accent" className="w-40" />
          <span>{n(sent.sent)} of {n(sent.total)} rows sent</span>
        </div>
      )}
      {done && (
        <div className="flex flex-col gap-2">
          <Notice kind={done.truncated ? "warn" : "info"} role="status">
            {done.truncated
              ? `Sent the first ${n(done.rows)} of ${n(done.total_rows)} rows.`
              : `Sent ${n(done.rows)} rows to ${done.spreadsheets.length === 1 ? "a new spreadsheet" : `${done.spreadsheets.length} new spreadsheets`}.`}
          </Notice>
          <ul aria-label="Spreadsheets" className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
            {done.spreadsheets.map((s) => (
              <li key={s.spreadsheet_id}>
                <a className="text-accent-text underline-offset-2 hover:underline" href={s.url} target="_blank" rel="noreferrer">
                  {s.title}
                </a>
                <span className="text-fg-3"> ({n(s.rows)} rows)</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {error && <Notice kind="bad" role="alert">{error}</Notice>}
      <div className="flex justify-end gap-2">
        {done && done.spreadsheets[0] && !looker && (
          <Button onClick={() => void transport.integrationsOpenLooker(done.spreadsheets[0])}>
            <ExternalLink aria-hidden className="mr-1 size-3.5" />
            Open in Looker Studio
          </Button>
        )}
        <Button variant="quiet" disabled={busy} onClick={() => onOpenChange(false)}>
          {done ? "Close" : "Cancel"}
        </Button>
        {!done && (
          <Button variant="primary" disabled={!ready || !plan || blocked || busy} onClick={() => void run()}>
            {looker ? "Export and open in Looker Studio" : "Export"}
          </Button>
        )}
      </div>
    </Modal>
  );
}
