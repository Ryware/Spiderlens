// Clusters of near duplicate pages, largest first, and one cluster's pages
// (US2, FR-016, FR-017).

import { useEffect, useState } from "react";
import { Columns2, Download, Layers } from "lucide-react";
import type { ClustersPage } from "../generated/ClustersPage";
import type { MembersPage } from "../generated/MembersPage";
import type { Transport } from "../transport";
import { Badge, Button, cn, EmptyState, Menu, Notice } from "../components/ui";
import type { OpenCompare } from "./compare";

const PAGE = 100;

const range = (lo: number, hi: number) => (lo === hi ? `${lo}%` : `${lo}–${hi}%`);

function Members({ transport, path, cluster, onCompare }: { transport: Transport; path: string; cluster: number; onCompare: OpenCompare }) {
  const [page, setPage] = useState<MembersPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let off = false;
    setPage(null);
    transport.nearDupMembers(path, cluster, 0, PAGE).then(
      (p) => !off && setPage(p),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, cluster]);
  if (error) return <Notice kind="bad">{error}</Notice>;
  if (!page) return <div className="p-3 text-sm text-fg-3">Loading…</div>;
  const rep = page.rows.find((r) => r.representative);
  const targets = page.rows.filter((r) => !r.representative).map((r) => ({ id: r.url_id, address: r.address, sim: r.sim }));
  const more = () =>
    transport.nearDupMembers(path, cluster, page.rows.length, PAGE).then(
      (p) => setPage({ ...p, rows: [...page.rows, ...p.rows] }),
      (e) => setError(String(e)),
    );
  return (
    <div className="flex flex-col">
      <ul aria-label={`Pages in cluster ${cluster}`} className="m-0 flex list-none flex-col p-0" data-selectable>
        {page.rows.map((r) => (
          <li key={r.url_id} className="flex items-center gap-2 border-b border-line px-3 py-1.5 text-sm">
            <span className="w-12 shrink-0 text-right tabular text-fg-2">{r.representative ? "" : r.sim === null ? "–" : `${r.sim}%`}</span>
            <span className="min-w-0 flex-1 truncate" title={r.address}>
              {r.address}
            </span>
            {r.representative && <Badge tone="info">Representative</Badge>}
            {r.exact && <Badge tone="warn">Exact</Badge>}
            {!r.representative && rep && (
              <Button
                size="sm"
                variant="quiet"
                aria-label={`Compare ${r.address} with ${rep.address}`}
                onClick={() => onCompare({ base: { id: rep.url_id, address: rep.address }, targets, index: targets.findIndex((t) => t.id === r.url_id) })}
              >
                <Columns2 aria-hidden className="size-3.5" />
                Compare
              </Button>
            )}
          </li>
        ))}
      </ul>
      {page.rows.length < page.total && (
        <Button size="sm" variant="quiet" className="m-2 self-start" onClick={() => void more()}>
          Show more ({(page.total - page.rows.length).toLocaleString()} left)
        </Button>
      )}
    </div>
  );
}

export function ClustersPanel({ transport, path, generation, onCompare }: { transport: Transport; path: string; generation: number; onCompare: OpenCompare }) {
  const [page, setPage] = useState<ClustersPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let off = false;
    transport.nearDupClusters(path, 0, PAGE).then(
      (p) => {
        if (off) return;
        setPage(p);
        setActive((a) => (a !== null && p.rows.some((r) => r.id === a) ? a : (p.rows[0]?.id ?? null)));
      },
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, generation]);

  const exportCsv = async (kind: "clusters" | "pairs", cluster: number | null) => {
    const dest = await transport.pickCsvPath(kind === "clusters" ? "near-duplicate-clusters" : cluster === null ? "near-duplicate-pairs" : `near-duplicate-cluster-${cluster}`);
    if (!dest) return;
    try {
      const rows = await transport.exportNearDup(path, kind, cluster, dest);
      setNotice(`Exported ${rows.toLocaleString()} rows to ${dest}`);
    } catch (e) {
      setNotice(String(e));
    }
  };

  if (error) return <EmptyState title="Could not read the clusters">{error}</EmptyState>;
  if (!page) return <div className="p-3 text-sm text-fg-3">Loading…</div>;
  if (page.total === 0)
    return (
      <EmptyState icon={<Layers className="size-6" />} title="No clusters">
        No pages are similar enough to group at this threshold.
      </EmptyState>
    );
  const more = () =>
    transport.nearDupClusters(path, page.rows.length, PAGE).then(
      (p) => setPage({ ...p, rows: [...page.rows, ...p.rows] }),
      (e) => setError(String(e)),
    );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5 text-sm">
        <span className="text-fg-3 tabular">
          {page.total.toLocaleString()} {page.total === 1 ? "cluster" : "clusters"}
        </span>
        <Menu
          label="Export clusters"
          trigger={
            <Button size="sm" variant="quiet" className="ml-auto">
              <Download aria-hidden className="size-3.5" />
              Export
            </Button>
          }
          items={[
            { label: "Export clusters as CSV…", onSelect: () => void exportCsv("clusters", null) },
            { label: "Export all pairs as CSV…", onSelect: () => void exportCsv("pairs", null) },
            { label: active !== null ? `Export cluster ${active} pairs as CSV…` : "Export cluster pairs as CSV…", onSelect: () => void exportCsv("pairs", active), disabled: active === null },
          ]}
        />
      </div>
      {notice && (
        <Notice kind="info" role="status" className="mx-3 my-1.5">
          {notice}
        </Notice>
      )}
      <div className="flex min-h-0 flex-1">
        <div className="w-1/2 min-w-0 overflow-y-auto border-r border-line">
          <ul role="listbox" aria-label="Clusters" className="m-0 flex list-none flex-col p-0">
            {page.rows.map((c) => (
              <li
                key={c.id}
                role="option"
                aria-selected={c.id === active}
                tabIndex={c.id === active ? 0 : -1}
                onClick={() => setActive(c.id)}
                onKeyDown={(e) => {
                  const i = page.rows.findIndex((r) => r.id === c.id);
                  const next = e.key === "ArrowDown" ? page.rows[i + 1] : e.key === "ArrowUp" ? page.rows[i - 1] : undefined;
                  if (next) {
                    e.preventDefault();
                    setActive(next.id);
                    (e.currentTarget.parentElement?.children[i + (e.key === "ArrowDown" ? 1 : -1)] as HTMLElement | undefined)?.focus();
                  }
                }}
                className={cn("flex cursor-default items-center gap-2 border-b border-line px-3 py-1.5 text-sm outline-none focus-visible:bg-surface-2", c.id === active ? "bg-selection" : "hover:bg-surface-2")}
              >
                <span className="w-10 shrink-0 text-fg-3 tabular">#{c.id}</span>
                <span className="w-16 shrink-0 tabular">{c.size.toLocaleString()} pages</span>
                <span className="w-20 shrink-0 text-fg-2 tabular">{range(c.min_sim, c.max_sim)}</span>
                <span className="min-w-0 flex-1 truncate text-fg-2" title={c.representative}>
                  {c.representative}
                </span>
                {c.exact && <Badge tone="warn">Exact</Badge>}
              </li>
            ))}
          </ul>
          {page.rows.length < page.total && (
            <Button size="sm" variant="quiet" className="m-2" onClick={() => void more()}>
              Show more ({(page.total - page.rows.length).toLocaleString()} left)
            </Button>
          )}
        </div>
        <div className="w-1/2 min-w-0 overflow-y-auto">{active !== null && <Members transport={transport} path={path} cluster={active} onCompare={onCompare} />}</div>
      </div>
    </div>
  );
}
