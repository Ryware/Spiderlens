// Grid spike for spec 017 (research R3): can a DOM-virtualized table hold
// 1M+ rows fed in pages over an async boundary, in each engine Tauri uses?
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useVirtualizer } from "@tanstack/react-virtual";

const params = new URLSearchParams(location.search);
const COUNT = Number(params.get("rows") ?? 1_000_000);
const ROW = 28;
const PAGE = 200;
const LATENCY = Number(params.get("latency") ?? 4);

type Row = [string, number, string, string, number, number, number];
const pages = new Map<number, Row[]>();
const pending = new Set<number>();

// Stand-in for a `rows(view, offset, limit)` engine call.
function fetchPage(p: number): Promise<Row[]> {
  return new Promise((res) =>
    setTimeout(() => {
      const rows: Row[] = [];
      for (let i = p * PAGE; i < Math.min(COUNT, (p + 1) * PAGE); i++)
        rows.push([`https://shop.example.com/kitchen/item-${i}/variant-${i % 7}`, i % 50 === 0 ? 404 : 200, i % 9 === 0 ? "Noindex" : "Indexable", `Product ${i}: a reasonably long page title for testing ellipsis`, 30 + (i % 60), i % 3000, 100 + (i % 900)]);
      res(rows);
    }, LATENCY),
  );
}

function Grid() {
  const ref = useRef<HTMLDivElement>(null);
  const [, bump] = useState(0);
  const v = useVirtualizer({ count: COUNT, getScrollElement: () => ref.current, estimateSize: () => ROW, overscan: 10 });
  const items = v.getVirtualItems();
  useEffect(() => {
    const need = new Set(items.map((i) => Math.floor(i.index / PAGE)));
    need.forEach((p) => {
      if (pages.has(p) || pending.has(p)) return;
      pending.add(p);
      fetchPage(p).then((r) => { pages.set(p, r); pending.delete(p); bump((n) => n + 1); });
    });
  });
  (window as any).__v = v;
  return (
    <div className="vp" ref={ref} data-testid="vp">
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {items.map((it) => {
          const r = pages.get(Math.floor(it.index / PAGE))?.[it.index % PAGE];
          return (
            <div key={it.key} className="row" data-index={it.index} style={{ transform: `translateY(${it.start}px)` }}>
              {r ? <><span>{r[0]}</span><span><span className="pill">{r[1]}</span></span><span>{r[2]}</span><span>{r[3]}</span><span>{r[4]}</span><span>{r[5]}</span><span>{r[6]}</span></> : <span>…</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
(window as any).__measure = async () => {
  const vp = document.querySelector<HTMLDivElement>("[data-testid=vp]")!;
  const out: Record<string, unknown> = { rows: COUNT, wantedHeight: COUNT * ROW, scrollHeight: vp.scrollHeight };
  // 1. Jump to the end and check the last row is reachable and rendered.
  vp.scrollTop = vp.scrollHeight;
  await sleep(300);
  const idx = [...document.querySelectorAll<HTMLElement>(".row")].map((e) => Number(e.dataset.index));
  out.lastRenderedIndex = Math.max(...idx);
  out.lastRowFilled = !!document.querySelector(`.row[data-index="${COUNT - 1}"] span`)?.textContent?.includes("item-");
  // 2. Continuous scroll for 3 s at ~6000 px/s; record frame times.
  vp.scrollTop = 0; await sleep(200);
  const frames: number[] = []; let last = performance.now(); const t0 = last;
  await new Promise<void>((done) => {
    const step = (t: number) => { frames.push(t - last); last = t; vp.scrollTop += 100; if (t - t0 < 3000) requestAnimationFrame(step); else done(); };
    requestAnimationFrame(step);
  });
  frames.sort((a, b) => a - b);
  out.frames = frames.length;
  out.p50ms = +frames[Math.floor(frames.length * 0.5)].toFixed(1);
  out.p95ms = +frames[Math.floor(frames.length * 0.95)].toFixed(1);
  out.over33ms = frames.filter((f) => f > 33.4).length;
  out.domRows = document.querySelectorAll(".row").length;
  return out;
};

createRoot(document.getElementById("root")!).render(<Grid />);
