import type { ReactNode } from "react";
import { Badge, Bar, Notice } from "../components/ui";
import type { Block } from "../generated/Block";
import type { Section } from "../generated/Section";
import type { Target } from "../generated/Target";
import { PRIORITY_LABEL, PRIORITY_TONE, points } from "./format";

type Open = (t: Target) => void;

/** A value that opens the matching rows when it has a target (FR-004). */
function Linked({ target, onOpen, children, label }: { target: Target | null; onOpen: Open; children: ReactNode; label?: string }) {
  if (!target) return <>{children}</>;
  return (
    <button type="button" aria-label={label} className="text-left text-accent-text hover:underline" onClick={() => onOpen(target)}>
      {children}
    </button>
  );
}

function BlockView({ block, onOpen }: { block: Block; onOpen: Open }) {
  switch (block.kind) {
    case "figures":
      return (
        <dl className="m-0 grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line md:grid-cols-3">
          {block.items.map((f) => (
            <div key={f.label} className="flex flex-col gap-0.5 bg-surface px-3 py-2">
              <dt className="text-xs text-fg-3">{f.label}</dt>
              <dd className="m-0 text-md font-semibold tabular">
                <Linked target={f.target} onOpen={onOpen} label={`${f.label}: ${f.value}`}>
                  {f.value}
                </Linked>
              </dd>
            </div>
          ))}
        </dl>
      );
    case "table":
      return (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-sm">
            <thead className="bg-surface-2 text-left text-xs text-fg-3">
              <tr>
                {block.columns.map((c) => (
                  <th key={c} className="px-3 py-1.5 font-medium">
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, i) => (
                <tr key={i} className="border-t border-line">
                  {row.map((c, j) => (
                    <td key={j} dir="auto" className="max-w-md truncate px-3 py-1.5 tabular">
                      <Linked target={c.target} onOpen={onOpen}>
                        {c.text}
                      </Linked>
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "bars":
      return (
        <ul className="m-0 flex list-none flex-col gap-1 p-0">
          {block.items.map((b) => (
            <li key={b.label} className="flex items-center gap-3 text-sm">
              <span className="w-32 shrink-0 truncate text-fg-2">{b.label}</span>
              <Bar value={b.share} tone="accent" label={b.label} className="min-w-0 flex-1" />
              <span className="w-20 shrink-0 text-right tabular">
                <Linked target={b.target} onOpen={onOpen}>
                  {b.value.toLocaleString()}
                </Linked>
              </span>
            </li>
          ))}
        </ul>
      );
    case "text":
      return <p className="m-0 text-sm text-fg-2">{block.text}</p>;
    case "issue": {
      const i = block.issue;
      return (
        <details className="group rounded-lg border border-line bg-surface" data-issue={i.id}>
          <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm">
            <Badge tone={PRIORITY_TONE[i.priority]}>{PRIORITY_LABEL[i.priority]}</Badge>
            <span className="min-w-0 flex-1 truncate">{i.name}</span>
            {i.unavailable ? (
              <span className="text-fg-3">Not checked</span>
            ) : (
              <span className="tabular">
                {i.count === null ? "Site" : i.count.toLocaleString()}
                {i.provisional && <span className="text-fg-3"> so far</span>}
              </span>
            )}
            {i.gain > 0 && <span className="w-14 text-right text-fg-3 tabular">+{points(i.gain)}</span>}
          </summary>
          <div className="flex flex-col gap-2 border-t border-line px-3 py-2 text-sm">
            {i.unavailable && <Notice>{i.unavailable}</Notice>}
            <p className="m-0">
              <span className="font-medium">Why it matters. </span>
              {i.why}
            </p>
            <p className="m-0">
              <span className="font-medium">How to fix. </span>
              {i.fix}
            </p>
            {i.count !== null && i.count > 0 && (
              <div>
                <Linked target={{ kind: "issue", id: i.id }} onOpen={onOpen}>
                  Show the {i.count.toLocaleString()} URLs
                </Linked>
              </div>
            )}
          </div>
        </details>
      );
    }
  }
}

/** One report section; one that cannot be built says why (FR-005). */
export function SectionView({ section, onOpen }: { section: Section; onOpen: Open }) {
  const id = `report-${section.id}`;
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3">
      <h2 id={id} className="m-0 flex items-center gap-2 text-md font-semibold">
        {section.title}
        {section.state.kind === "provisional" && <Badge tone="info">Provisional</Badge>}
      </h2>
      {section.state.kind === "unavailable" ? (
        <p className="m-0 text-sm text-fg-3">{section.state.reason}</p>
      ) : (
        section.blocks.map((b, i) => <BlockView key={i} block={b} onOpen={onOpen} />)
      )}
    </section>
  );
}
