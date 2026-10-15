import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ArrowUp, MoreHorizontal } from "lucide-react";
import { Badge, Button, Chip, Menu, Modal, Notice, Sheet, SheetClose } from "../components/ui";
import type { CrawlBackup, DeleteReport, HtmlSize, ManageList, ManagedCrawl, PurgeCheck, Transport } from "../transport";
import { fileStem, formatSize, formatWhen } from "./format";

type Filter = "all" | "unsaved" | "saved" | "unavailable";
type SortKey = "name" | "size" | "last_used" | "created";

const filters: { id: Filter; label: string; test: (c: ManagedCrawl) => boolean }[] = [
  { id: "all", label: "All", test: () => true },
  { id: "unsaved", label: "Unsaved", test: (c) => c.unsaved },
  { id: "saved", label: "Saved", test: (c) => !c.unsaved && c.available },
  { id: "unavailable", label: "Unavailable", test: (c) => !c.available },
];

const title = (c: { name: string; start_url: string | null; path: string }) => c.name || c.start_url || fileStem(c.path);
const time = (iso: string | null) => (iso ? Date.parse(iso) || 0 : 0);

function compare(a: ManagedCrawl, b: ManagedCrawl, key: SortKey): number {
  switch (key) {
    case "name":
      return title(a).localeCompare(title(b));
    case "size":
      return a.size_bytes - b.size_bytes;
    case "last_used":
      return time(a.last_used) - time(b.last_used);
    case "created":
      return time(a.created) - time(b.created);
  }
}

type Confirm =
  | { kind: "delete"; paths: string[] }
  | { kind: "permanent"; paths: string[] }
  | { kind: "purge"; paths: string[]; checks: PurgeCheck[] };

/**
 * Manage crawls (spec 019 US1 to US3, US5.1): every crawl the app knows,
 * what it costs on disk, and the actions that free space. Deletes go to the
 * trash; a permanent delete is asked for separately.
 */
export function ManageCrawls({
  open,
  onOpenChange,
  transport,
  onOpenCrawl,
  onShowInfo,
  onSaveAs,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  onOpenCrawl: (path: string) => void;
  onShowInfo: (path: string) => void;
  onSaveAs: (path: string) => void;
}) {
  const [list, setList] = useState<ManageList | null>(null);
  const [html, setHtml] = useState<Record<string, HtmlSize>>({});
  const [filter, setFilter] = useState<Filter>("all");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "last_used", desc: true });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [message, setMessage] = useState<{ kind: "info" | "bad" | "warn"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);

  const reload = useCallback(async () => {
    const g = ++generation.current;
    let l: ManageList;
    try {
      l = await transport.manageList();
    } catch (e) {
      setMessage({ kind: "bad", text: `Could not list crawls: ${String(e)}` });
      return;
    }
    if (g !== generation.current) return;
    setList(l);
    setSelected((s) => new Set([...s].filter((p) => l.crawls.some((c) => c.path === p))));
    // Sizes first, stored HTML afterwards, one file at a time (FR-004).
    for (const c of l.crawls) {
      if (!c.available) continue;
      try {
        const h = await transport.htmlSize(c.path);
        if (g !== generation.current) return;
        setHtml((m) => ({ ...m, [c.path]: h }));
      } catch {
        // A file that cannot be read keeps its placeholder.
      }
    }
  }, [transport]);

  useEffect(() => {
    if (!open) return;
    setMessage(null);
    void reload();
    const off = transport.onCrawlsChanged(() => void reload());
    return () => void off.then((f) => f());
  }, [open, reload, transport]);

  const rows = useMemo(() => {
    const f = filters.find((x) => x.id === filter)!;
    const r = (list?.crawls ?? []).filter(f.test);
    r.sort((a, b) => (sort.desc ? -1 : 1) * compare(a, b, sort.key));
    return r;
  }, [list, filter, sort]);

  const byPath = (p: string) => list?.crawls.find((c) => c.path === p) ?? null;
  const allBackups = useMemo(() => (list?.crawls ?? []).flatMap((c) => c.backups), [list]);
  const sizeOfPath = (p: string) => byPath(p)?.size_bytes ?? allBackups.find((b) => b.path === p)?.size_bytes ?? 0;
  const nameOfPath = (p: string) => {
    const c = byPath(p);
    return c ? title(c) : fileStem(p);
  };

  const toggle = (path: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });
  const allShown = rows.length > 0 && rows.every((r) => selected.has(r.path));
  const toggleAll = () => setSelected(allShown ? new Set() : new Set(rows.map((r) => r.path)));

  const sortBy = (key: SortKey) => setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));

  const reportDelete = (r: DeleteReport, permanent: boolean) => {
    const parts: string[] = [];
    if (r.deleted.length)
      parts.push(`${permanent ? "Deleted" : "Moved to the Trash"}: ${r.deleted.length} ${r.deleted.length === 1 ? "item" : "items"}, ${formatSize(r.freed_bytes)} freed.`);
    for (const s of r.skipped) parts.push(`${nameOfPath(s.path)} was not deleted: ${s.reason}`);
    setMessage(parts.length ? { kind: r.skipped.length ? "warn" : "info", text: parts.join(" ") } : null);
    if (r.trash_unavailable.length) setConfirm({ kind: "permanent", paths: r.trash_unavailable });
  };

  const runDelete = async (paths: string[], permanent: boolean) => {
    setConfirm(null);
    setBusy(true);
    try {
      const r = permanent ? await transport.crawlsDeletePermanently(paths) : await transport.crawlsDelete(paths);
      reportDelete(r, permanent);
    } catch (e) {
      setMessage({ kind: "bad", text: String(e) });
    } finally {
      setBusy(false);
      await reload();
    }
  };

  const askPurge = async (paths: string[]) => {
    const withHtml = paths.filter((p) => (html[p]?.pages ?? 0) > 0);
    if (!withHtml.length) return;
    try {
      const checks = await Promise.all(withHtml.map((p) => transport.purgeHtmlCheck(p)));
      setConfirm({ kind: "purge", paths: withHtml, checks });
    } catch (e) {
      setMessage({ kind: "bad", text: String(e) });
    }
  };

  const runPurge = async (paths: string[], checks: PurgeCheck[]) => {
    setConfirm(null);
    setBusy(true);
    let freed = 0;
    let uncompacted = 0;
    const failures: string[] = [];
    for (const [i, p] of paths.entries()) {
      try {
        const r = await transport.purgeHtml(p, checks[i].can_compact);
        freed += r.freed_bytes;
        if (!r.compacted) uncompacted++;
      } catch (e) {
        failures.push(`${nameOfPath(p)}: ${String(e)}`);
      }
    }
    const text = [
      `Stored HTML removed${freed ? `, ${formatSize(freed)} freed` : ""}.`,
      uncompacted ? "The space stays inside the file for this crawl to reuse; there was not enough free space to shrink it." : "",
      ...failures,
    ]
      .filter(Boolean)
      .join(" ");
    setMessage({ kind: failures.length ? "warn" : "info", text });
    setBusy(false);
    await reload();
  };

  const locate = async (path: string) => {
    const found = await transport.pickCrawlFile();
    if (!found) return;
    await transport.recentLocate(path, found);
    await reload();
  };
  const removeFromList = async (path: string) => {
    await transport.recentRemove(path);
    await reload();
  };

  const selectedPaths = [...selected];
  const selectedHtml = selectedPaths.filter((p) => (html[p]?.pages ?? 0) > 0 && !byPath(p)?.running && !byPath(p)?.view_only);
  const t = list?.totals;

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Manage crawls" className="w-240 sheet-max-w">
      <header className="flex flex-col gap-1 border-b border-line px-5 py-3">
        <div className="flex items-center gap-2">
          <h2 className="m-0 text-md font-semibold">Manage crawls</h2>
          <SheetClose render={<Button size="sm" variant="quiet" className="ml-auto" />}>Done</SheetClose>
        </div>
        {t && (
          <p className="m-0 flex flex-wrap gap-x-4 text-sm text-fg-2 tabular" data-testid="manage-totals">
            <span>All crawls: {formatSize(t.all)}</span>
            <span>Unsaved: {formatSize(t.unsaved)}</span>
            {t.leftovers > 0 && <span>Leftover files: {formatSize(t.leftovers)}</span>}
            {list.free_bytes !== null && <span>Free on the unsaved-crawls drive: {formatSize(list.free_bytes)}</span>}
          </p>
        )}
      </header>

      <div className="flex items-center gap-1 border-b border-line px-4 py-2" role="group" aria-label="Show">
        {filters.map((f) => (
          <Chip key={f.id} label={f.label} count={list ? list.crawls.filter(f.test).length : null} active={filter === f.id} onClick={() => setFilter(f.id)} />
        ))}
        {selected.size > 0 && (
          <div className="ml-auto flex items-center gap-2" role="toolbar" aria-label="Selected crawls">
            <span className="text-sm text-fg-2 tabular">{selected.size} selected</span>
            <Button size="sm" disabled={busy || selectedHtml.length === 0} onClick={() => void askPurge(selectedHtml)}>
              Remove stored HTML…
            </Button>
            <Button size="sm" variant="destructive" disabled={busy} onClick={() => setConfirm({ kind: "delete", paths: selectedPaths })}>
              Delete…
            </Button>
          </div>
        )}
      </div>

      {message && (
        <Notice kind={message.kind} role={message.kind === "bad" ? "alert" : "status"} className="mx-4 mt-3">
          {message.text}
        </Notice>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {!list && <p className="m-3 text-sm text-fg-3">Reading crawls…</p>}
        {list && rows.length === 0 && <p className="m-3 text-sm text-fg-3">No crawls here.</p>}
        {list && rows.length > 0 && (
          <table className="w-full border-collapse text-sm" aria-label="Crawls">
            <thead>
              <tr className="text-left text-xs text-fg-3">
                <th className="w-8 px-2 py-1">
                  <input type="checkbox" aria-label="Select all shown crawls" checked={allShown} onChange={toggleAll} />
                </th>
                <SortHeader label="Name" k="name" sort={sort} onSort={sortBy} />
                <SortHeader label="Last used" k="last_used" sort={sort} onSort={sortBy} />
                <SortHeader label="Created" k="created" sort={sort} onSort={sortBy} />
                <th className="px-2 py-1 text-right font-medium">URLs</th>
                <SortHeader label="Size" k="size" sort={sort} onSort={sortBy} align="right" />
                <th className="px-2 py-1 text-right font-medium">Stored HTML</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <CrawlRows
                  key={c.path}
                  c={c}
                  html={html[c.path]}
                  selected={selected.has(c.path)}
                  onToggle={() => toggle(c.path)}
                  actions={{
                    open: () => {
                      onOpenChange(false);
                      onOpenCrawl(c.path);
                    },
                    reveal: () => void transport.revealPath(c.path),
                    info: () => onShowInfo(c.path),
                    saveAs: () => onSaveAs(c.path),
                    purge: () => void askPurge([c.path]),
                    remove: () => void removeFromList(c.path),
                    locate: () => void locate(c.path),
                    del: () => setConfirm({ kind: "delete", paths: [c.path] }),
                    delBackup: (b) => setConfirm({ kind: "delete", paths: [b.path] }),
                    revealBackup: (b) => void transport.revealPath(b.path),
                  }}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {confirm?.kind === "delete" && (
        <DeleteConfirm
          paths={confirm.paths}
          saved={confirm.paths.filter((p) => byPath(p) && !byPath(p)!.unsaved).length}
          bytes={confirm.paths.reduce((n, p) => n + sizeOfPath(p), 0)}
          nameOf={nameOfPath}
          onCancel={() => setConfirm(null)}
          onConfirm={() => void runDelete(confirm.paths, false)}
        />
      )}
      {confirm?.kind === "permanent" && (
        <Modal
          open
          onOpenChange={(o) => !o && setConfirm(null)}
          title={`Delete permanently? The Trash is not available for ${confirm.paths.length === 1 ? nameOfPath(confirm.paths[0]) : `${confirm.paths.length} crawls`}.`}
          description="They are on a drive without a Trash, so this can't be undone."
        >
          <ul className="m-0 list-none p-0 text-sm text-fg-2">
            {confirm.paths.map((p) => (
              <li key={p} className="truncate" title={p}>
                {p}
              </li>
            ))}
          </ul>
          <div className="flex justify-end gap-2">
            <Button onClick={() => setConfirm(null)}>Keep</Button>
            <Button variant="destructive" onClick={() => void runDelete(confirm.paths, true)}>
              Delete permanently
            </Button>
          </div>
        </Modal>
      )}
      {confirm?.kind === "purge" && (
        <PurgeConfirm checks={confirm.checks} paths={confirm.paths} nameOf={nameOfPath} onCancel={() => setConfirm(null)} onConfirm={() => void runPurge(confirm.paths, confirm.checks)} />
      )}
    </Sheet>
  );
}

function SortHeader({
  label,
  k,
  sort,
  onSort,
  align,
}: {
  label: string;
  k: SortKey;
  sort: { key: SortKey; desc: boolean };
  onSort: (k: SortKey) => void;
  align?: "right";
}) {
  const active = sort.key === k;
  return (
    <th className={`px-2 py-1 font-medium ${align === "right" ? "text-right" : ""}`} aria-sort={active ? (sort.desc ? "descending" : "ascending") : "none"}>
      <button type="button" className="inline-flex items-center gap-1 hover:text-fg" onClick={() => onSort(k)}>
        {label}
        {active && (sort.desc ? <ArrowDown aria-hidden className="size-3" /> : <ArrowUp aria-hidden className="size-3" />)}
      </button>
    </th>
  );
}

interface RowActions {
  open: () => void;
  reveal: () => void;
  info: () => void;
  saveAs: () => void;
  purge: () => void;
  remove: () => void;
  locate: () => void;
  del: () => void;
  delBackup: (b: CrawlBackup) => void;
  revealBackup: (b: CrawlBackup) => void;
}

function CrawlRows({ c, html, selected, onToggle, actions }: { c: ManagedCrawl; html: HtmlSize | undefined; selected: boolean; onToggle: () => void; actions: RowActions }) {
  const name = title(c);
  const hasHtml = (html?.pages ?? 0) > 0;
  const items = c.available
    ? [
        { label: "Open", onSelect: actions.open, disabled: c.running },
        { label: "Show in folder", onSelect: actions.reveal },
        { label: "Crawl information", onSelect: actions.info },
        c.unsaved ? { label: "Save As…", onSelect: actions.saveAs } : null,
        null,
        {
          label: "Remove stored HTML…",
          onSelect: actions.purge,
          disabled: !hasHtml || c.running || c.view_only,
        },
        c.in_recent && !c.unsaved ? { label: "Remove from list", onSelect: actions.remove } : null,
        { label: "Delete…", onSelect: actions.del, destructive: true, disabled: c.running },
      ]
    : [
        { label: "Locate…", onSelect: actions.locate },
        { label: "Remove from list", onSelect: actions.remove },
      ];
  return (
    <>
      <tr data-path={c.path} className={`border-t border-line ${selected ? "bg-selection" : ""} ${c.available ? "" : "text-fg-3"}`}>
        <td className="px-2 py-1.5">
          <input type="checkbox" aria-label={`Select ${name}`} checked={selected} onChange={onToggle} />
        </td>
        <td className="max-w-80 px-2 py-1.5">
          <span className="flex items-center gap-1.5">
            <span className="truncate font-medium" title={c.path}>
              {name}
            </span>
            {c.unsaved && <Badge>Unsaved</Badge>}
            {c.running && <Badge tone="info">Running</Badge>}
            {!c.available && <Badge tone="warn">File not found</Badge>}
            {c.view_only && <Badge>View only</Badge>}
          </span>
          <span className="block truncate text-xs text-fg-3" title={c.path}>
            {c.unsaved ? "In the app's folder" : c.path}
          </span>
        </td>
        <td className="px-2 py-1.5 whitespace-nowrap text-fg-2">{c.last_used ? formatWhen(c.last_used) : "–"}</td>
        <td className="px-2 py-1.5 whitespace-nowrap text-fg-2">{c.created ? formatWhen(c.created) : "–"}</td>
        <td className="px-2 py-1.5 text-right tabular">{c.urls === null ? "–" : c.urls.toLocaleString()}</td>
        <td className="px-2 py-1.5 text-right tabular" data-testid="size">
          {c.available ? formatSize(c.size_bytes) : "–"}
        </td>
        <td className="px-2 py-1.5 text-right tabular text-fg-2" data-testid="html-size">
          {!c.available ? "–" : html === undefined ? "…" : html.pages === 0 ? "None" : formatSize(html.bytes)}
        </td>
        <td className="px-1 py-1">
          <Menu label={`Actions for ${name}`} trigger={<Button size="sm" variant="quiet" icon><MoreHorizontal aria-hidden className="size-4" /></Button>} items={items} />
        </td>
      </tr>
      {c.backups.map((b) => (
        <tr key={b.path} data-path={b.path} className="text-fg-2">
          <td />
          <td className="px-2 py-1 pl-6" colSpan={4}>
            <span className="truncate text-xs" title={b.path}>
              Backup before the format {b.format_version + 1} upgrade{b.modified ? `, ${formatWhen(b.modified)}` : ""}
            </span>
          </td>
          <td className="px-2 py-1 text-right text-xs tabular">{formatSize(b.size_bytes)}</td>
          <td />
          <td className="px-1 py-1">
            <Menu
              label={`Actions for the backup of ${name}`}
              trigger={<Button size="sm" variant="quiet" icon><MoreHorizontal aria-hidden className="size-4" /></Button>}
              items={[
                { label: "Show in folder", onSelect: () => actions.revealBackup(b) },
                { label: "Delete…", onSelect: () => actions.delBackup(b), destructive: true },
              ]}
            />
          </td>
        </tr>
      ))}
    </>
  );
}

function DeleteConfirm({
  paths,
  saved,
  bytes,
  nameOf,
  onCancel,
  onConfirm,
}: {
  paths: string[];
  saved: number;
  bytes: number;
  nameOf: (p: string) => string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const what = paths.length === 1 ? nameOf(paths[0]) : `${paths.length} crawls`;
  return (
    <Modal open onOpenChange={(o) => !o && onCancel()} title={`Move ${what} to the Trash?`} description={`Frees ${formatSize(bytes)}. You can restore from the Trash.`}>
      {saved > 0 && (
        <Notice kind="warn">
          {saved === paths.length && paths.length === 1 ? "This is a saved crawl file, not only an unsaved crawl." : `${saved} of these are saved crawl files, not only unsaved crawls.`}
        </Notice>
      )}
      <div className="flex justify-end gap-2">
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant="destructive" onClick={onConfirm}>
          Move to Trash
        </Button>
      </div>
    </Modal>
  );
}

function PurgeConfirm({
  checks,
  paths,
  nameOf,
  onCancel,
  onConfirm,
}: {
  checks: PurgeCheck[];
  paths: string[];
  nameOf: (p: string) => string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const bytes = checks.reduce((n, c) => n + c.html_bytes, 0);
  const tight = checks.filter((c) => !c.can_compact);
  const what = paths.length === 1 ? nameOf(paths[0]) : `${paths.length} crawls`;
  return (
    <Modal
      open
      onOpenChange={(o) => !o && onCancel()}
      title={`Remove stored HTML from ${what}?`}
      description={`Frees about ${formatSize(bytes)}. Every result stays as it is, but the page HTML can no longer be viewed, and pages crawled so far can't be rendered later.`}
    >
      {tight.length > 0 && (
        <Notice kind="warn">
          {`Not enough free space to shrink ${tight.length === paths.length && paths.length === 1 ? "the file" : `${tight.length} of the files`} (needs about ${formatSize(tight[0].new_size)}, ${formatSize(tight[0].free_bytes ?? 0)} free). The HTML is still removed, and the crawl reuses the space.`}
        </Notice>
      )}
      <div className="flex justify-end gap-2">
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant="destructive" onClick={onConfirm}>
          Remove HTML
        </Button>
      </div>
    </Modal>
  );
}
