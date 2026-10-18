// The detail pane for the selected URL (US3, FR-019..FR-025).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Columns2, Copy, CornerDownRight, FileCode, MousePointerClick, TableProperties } from "lucide-react";
import type { OpenCompare } from "../content/compare";
import type { CustomResult } from "../generated/CustomResult";
import type { CustomRules } from "../generated/CustomRules";
import type { RuleTestOutcome } from "../transport";
import type { LinkMetrics } from "../generated/LinkMetrics";

import type { ListKind } from "../generated/ListKind";
import type { ListPage } from "../generated/ListPage";
import type { PairsPage } from "../generated/PairsPage";
import type { AgentField } from "../generated/AgentField";
import type { PageAgentFindings } from "../generated/PageAgentFindings";
import type { AmpDetail } from "../generated/AmpDetail";
import type { A11yDetail } from "../generated/A11yDetail";
import type { A11yCheckDetail } from "../generated/A11yCheckDetail";
import type { MobileDetail } from "../generated/MobileDetail";
import type { SpellDetail } from "../generated/SpellDetail";
import type { PdfDetail } from "../generated/PdfDetail";
import type { UrlDetail } from "../generated/UrlDetail";
import type { UrlIssue } from "../generated/UrlIssue";
import { PriorityBadge } from "../issues/priority";
import type { Transport } from "../transport";
import { Badge, Bar, Button, cn, ContextMenu, EmptyState, Notice, Segmented, Select, type Tone } from "../components/ui";
import { RuleFound } from "../settings/CustomRules";
import { AiPageSection } from "./AiPageSection";
import { CookieFlags, expiresText } from "./CookiesView";

import { cellTone, formatBytes, toneText } from "./format";
import { HtmlViewer } from "./HtmlViewer";
import { SearchPreview } from "./SearchPreview";
import { SitemapEntryList, useSitemapEntries } from "../sitemaps/InspectorSitemaps";

const LIST_PAGE = 100;

const stateTone = (d: UrlDetail): Tone => {
  if (d.error_class || d.final_state.startsWith("Failed")) return "bad";
  const t = cellTone("status", d.status);
  if (t) return t;
  return d.not_crawled_reason ? "warn" : "neutral";
};

const fieldLabels: Record<string, string> = {
  indexability: "Indexability",
  headers: "Response headers",
  headings: "Headings",
  anchor_text: "Anchor and alt text",
  size: "Size",
  response_time: "Response time",
  attempts: "Retry attempts",
  raw_html: "Page HTML",
  pdf: "PDF properties",
  link_metrics: "Link Score",
  cookies: "Cookies",
  spelling: "Spelling",
};

const ampCheck: Record<string, string> = {
  runtime: "Runtime script",
  boilerplate: "Boilerplate style",
  viewport: "Viewport",
  canonical: "Canonical",
  author_script: "Author script",
  style_size: "Custom CSS size",
  disallowed: "Disallowed element",
};

const impactClass: Record<string, string> = {
  critical: "text-impact-critical",
  serious: "text-impact-serious",
  moderate: "text-impact-moderate",
  minor: "text-impact-minor",
};

const NODE_CAP = 20;

function A11yCheck({ check, capped }: { check: A11yCheckDetail; capped: boolean }) {
  const impact = check.impact ?? "none";
  return (
    <li className="flex flex-col gap-0.5 border-t border-line pt-1.5 first:border-t-0 first:pt-0">
      <span className="flex flex-wrap items-center gap-1.5">
        <span className={cn("text-xs font-semibold", impactClass[impact] ?? "text-fg-2")}>{impact.charAt(0).toUpperCase() + impact.slice(1)}</span>
        <span className="text-sm font-medium">{check.title}</span>
        {check.kind !== "violation" && <Badge tone="neutral">{check.kind === "incomplete" ? "Needs review" : check.kind}</Badge>}
      </span>
      <span className="text-xs text-fg-3">
        {check.wcag ? check.wcag.split(" ").join(", ") : "Best practice"} · {check.nodes_total.toLocaleString()} {check.nodes_total === 1 ? "element" : "elements"}
        {check.help_url && (
          <>
            {" · "}
            <a className="text-accent-text underline" href={check.help_url} target="_blank" rel="noreferrer noopener">
              How to fix
            </a>
          </>
        )}
      </span>
      {(capped || check.nodes_total > check.nodes.length) && check.nodes.length > 0 && check.nodes_total > check.nodes.length && (
        <span className="text-xs text-fg-3">
          Showing the first {check.nodes.length.toLocaleString()} elements of {check.nodes_total.toLocaleString()}
        </span>
      )}
      <ul className="m-0 flex list-none flex-col gap-1 p-0" data-selectable>
        {check.nodes.slice(0, NODE_CAP).map((n, i) => (
          <li key={`${n.selector}-${i}`} className="flex flex-col">
            <code className="font-mono text-xs break-all">{n.selector}</code>
            {n.excerpt && <code className="font-mono text-xs break-all text-fg-3">{n.excerpt}</code>}
          </li>
        ))}
      </ul>
    </li>
  );
}

/** Accessibility checks on this page (spec 026). */
function AccessibilitySection({ a }: { a: A11yDetail }) {
  const failed = a.checks.filter((c) => c.kind === "violation").length;
  return (
    <Section title={`Accessibility (${failed === 0 ? "no failures" : `${failed} failed`})`}>
      {a.full_reason && <p className="m-0 text-xs text-fg-3">Full checks: {a.full_reason}</p>}
      {a.checks.length === 0 && <p className="m-0 text-sm text-fg-3">No problems found by the checks that ran.</p>}
      <ul className="m-0 flex list-none flex-col gap-2 p-0">
        {a.checks.map((c) => (
          <A11yCheck key={`${c.source}-${c.check_id}`} check={c} capped={a.capped} />
        ))}
      </ul>
      <p className="m-0 text-xs text-fg-3">{a.disclaimer}</p>
    </Section>
  );
}

/** Mobile friendliness of this page (spec 026). */
function MobileSection({ m }: { m: MobileDetail }) {
  const px = (v: number | null) => (v === null ? null : `${Math.round(v).toLocaleString()} px`);
  const rows: [string, ReactNode][] = [
    ["Viewport", m.viewport ? <code className="font-mono">{m.viewport}</code> : "None"],
    ["Viewport state", m.viewport_state],
    ["Measured", m.measured],
    ["Content width", px(m.content_px)],
    ["Window width", px(m.window_px)],
    ["Widest element", m.widest ? <code className="font-mono break-all">{m.widest}</code> : null],
    ["Small text", m.small_text_share === null ? null : `${Math.round(m.small_text_share).toLocaleString()}% of text`],
    ["Small tap targets", m.small_targets === null ? null : m.small_targets.toLocaleString()],
  ];
  return (
    <Section title="Mobile">
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-sm" data-selectable>
        {rows
          .filter(([, v]) => v !== null && v !== undefined && v !== "")
          .map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-fg-3">{k}</dt>
              <dd className="m-0">{v}</dd>
            </div>
          ))}
      </dl>
      {m.examples.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-0.5 p-0" data-selectable>
          {m.examples.map((e, i) => (
            <li key={`${e.selector}-${i}`} className="text-xs">
              <code className="font-mono break-all">{e.selector}</code>
              <span className="text-fg-3">{` · ${Math.round(e.w)} x ${Math.round(e.h)} px`}</span>
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

/** What the AMP checks found (spec 027 part E). */
function AmpSection({ amp }: { amp: AmpDetail }) {
  const title = amp.format === "email" ? "AMP for email" : "AMP";
  return (
    <Section title={`${title} (${amp.failures.length === 0 ? "passes" : `${amp.failures.length} failed`})`}>
      {amp.paired === false && <Notice kind="warn">The canonical page has no amphtml link back to this page.</Notice>}
      {amp.failures.length === 0 && amp.paired !== false && <p className="m-0 text-sm text-fg-3">Passes every check SpiderLens runs.</p>}
      <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm" data-selectable>
        {amp.failures.map((f, i) => (
          <li key={`${f.check}-${i}`} className="flex flex-col">
            <span>
              <span className="font-medium text-bad">{ampCheck[f.check] ?? f.check}</span>
              {f.pos !== null && <span className="text-xs text-fg-3"> · element {f.pos.toLocaleString()}</span>}
            </span>
            <span className="text-xs text-fg-2">{f.detail}</span>
          </li>
        ))}
      </ul>
      <p className="m-0 text-xs text-fg-3">A core subset of AMP's rules, not the official AMP validator.</p>
    </Section>
  );
}

/** What was read from a PDF (spec 027 part C). */
function PdfSection({ pdf }: { pdf: PdfDetail }) {
  const rows: [string, ReactNode][] = [
    ["Reading", <span className={pdf.status === "read" ? undefined : "text-warn"}>{pdf.status_label}</span>],
    ["Title", pdf.title ?? <span className="text-warn">Missing</span>],
  ];
  const opt = (k: string, v: string | null) => v && rows.push([k, v]);
  opt("Author", pdf.author);
  opt("Subject", pdf.subject);
  opt("Keywords", pdf.keywords);
  if (pdf.pages !== null) rows.push(["Pages", pdf.pages_read !== null && pdf.pages_read < pdf.pages ? `${pdf.pages_read.toLocaleString()} of ${pdf.pages.toLocaleString()} read` : pdf.pages.toLocaleString()]);
  if (pdf.words !== null) rows.push(["Words", pdf.no_text ? <span className="text-warn">No text (scanned)</span> : pdf.words.toLocaleString()]);
  opt("Language", pdf.lang);
  rows.push(["Tagged", pdf.tagged ? "Yes" : "No"]);
  rows.push(["Links", pdf.links.toLocaleString()]);
  opt("Created", pdf.created);
  opt("Modified", pdf.modified);
  opt("Creator", pdf.creator);
  opt("Producer", pdf.producer);
  return (
    <Section title="PDF">
      {pdf.status_reason && <Notice kind={pdf.status === "failed" ? "bad" : "warn"}>{pdf.status_reason}</Notice>}
      <Facts rows={rows} />
    </Section>
  );
}

const sourceLabel: Record<string, string> = { title: "Title", description: "Meta description", heading: "Heading", body: "Body" };

/** Misspelt words on the page, with suggestions (spec 027 part D). */
function SpellingSection({ spell, ignored, onIgnore }: { spell: SpellDetail; ignored: string[]; onIgnore?: (word: string) => void }) {
  const isIgnored = (w: string) => ignored.some((x) => x.toLowerCase() === w.toLowerCase());
  const status =
    spell.status === "no_dictionary"
      ? `Not checked: no dictionary for ${spell.lang ?? "this language"}.`
      : spell.status === "partly_checked"
        ? "Only the title, description and headings were checked: this crawl did not keep page HTML."
        : null;
  return (
    <Section title={`Spelling (${spell.errors.toLocaleString()})`} aside={spell.lang && <span className="text-xs text-fg-3">Dictionary {spell.lang}</span>}>
      {status && <Notice kind="warn">{status}</Notice>}
      {spell.lang_mismatch && <Notice kind="warn">Most words are in another script than the page's language. Check its lang attribute.</Notice>}
      {spell.findings.length === 0 && !status && <p className="m-0 text-sm text-fg-3">No spelling errors.</p>}
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm" data-selectable>
        {spell.findings.map((f, i) => (
          <li key={`${f.word}-${f.source}-${i}`} className="flex flex-col gap-0.5">
            <div className="flex items-center gap-1.5">
              <span className="font-medium text-bad">{f.word}</span>
              <span className="text-xs text-fg-3">{sourceLabel[f.source] ?? f.source}</span>
              {f.site_wide && <Badge tone="neutral">Site-wide</Badge>}
              {onIgnore && (
                <Button size="sm" variant="quiet" className="ml-auto" disabled={isIgnored(f.word)} onClick={() => onIgnore(f.word)}>
                  {isIgnored(f.word) ? "Ignored" : "Ignore"}
                </Button>
              )}
            </div>
            {f.suggestions.length > 0 && <span className="text-xs text-fg-2">Did you mean {f.suggestions.slice(0, 3).join(", ")}?</span>}
            <span className="text-xs break-words text-fg-3" dir="auto">
              {f.context}
            </span>
          </li>
        ))}
      </ul>
      {ignored.length > 0 && onIgnore && <p className="m-0 text-xs text-fg-3">Ignored words apply from the next check.</p>}
    </Section>
  );
}

function Section({ title, children, aside }: { title: string; children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5 border-b border-line px-3 py-3">
      <div className="flex items-center gap-2">
        <h3 className="m-0 text-xs font-semibold text-fg-3">{title}</h3>
        {aside && <div className="ml-auto">{aside}</div>}
      </div>
      {children}
    </section>
  );
}

/** Link Score and link counts (spec 023 FR-008). */
function LinkScoreSection({ m }: { m: LinkMetrics }) {
  const n = (v: number) => v.toLocaleString();
  return (
    <Section title="Link Score">
      <div className="flex items-center gap-3">
        <span className="text-xl font-semibold tabular text-fg">{m.score.toFixed(1)}</span>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <Bar value={m.score / 100} tone="accent" label="Link Score" className="w-full" />
          <span className="text-xs text-fg-3">
            Rank {n(m.rank)} of {n(m.of)} internal URLs
          </span>
        </div>
      </div>
      <Facts
        rows={[
          ["Unique inlinks", n(m.unique_inlinks)],
          ["Followed inlinks", n(m.followed_inlinks)],
          ["All inlinks", n(m.all_inlinks)],
          ["Unique outlinks", n(m.unique_outlinks)],
          ["External outlinks", n(m.external_outlinks)],
          ["Pages linking", `${m.pct_linking.toFixed(1)}%`],
        ]}
      />
    </Section>
  );
}

function Facts({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="m-0 grid grid-cols-3 gap-x-3 gap-y-1 text-sm" data-selectable>
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-fg-3">{k}</dt>
          <dd className="col-span-2 m-0 min-w-0 break-words text-fg">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/** What a URL row in the inspector's lists can do (T027). */
interface RowActions {
  copy: (url: string) => void;
  /** Absent when there is no table to show the URL in. */
  show?: (id: number) => void;
}

/**
 * A list row naming one URL, with Copy URL and Show in table: buttons that
 * appear on hover or keyboard focus, and the same actions on right-click.
 */
function UrlRow({ actions, id, url, className, children }: { actions: RowActions; id: number; url: string; className?: string; children: ReactNode }) {
  const { copy, show } = actions;
  return (
    <ContextMenu
      items={[
        { label: "Copy URL", onSelect: () => copy(url) },
        ...(show ? [{ label: "Show in table", onSelect: () => show(id) }] : []),
      ]}
    >
      <li className={cn("group", className)}>
        {children}
        <span className="-my-1 flex shrink-0 gap-0.5 self-center opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
          <Button size="sm" variant="quiet" icon aria-label={`Copy ${url}`} title="Copy URL" onClick={() => copy(url)}>
            <Copy aria-hidden className="size-3.5" />
          </Button>
          {show && (
            <Button size="sm" variant="quiet" icon aria-label={`Show ${url} in table`} title="Show in table" onClick={() => show(id)}>
              <TableProperties aria-hidden className="size-3.5" />
            </Button>
          )}
        </span>
      </li>
    </ContextMenu>
  );
}

/** Anchor text, or for images whether the alt text is there. */
function LinkText({ kind, text }: { kind: ListKind; text: string | null }) {
  if (kind === "images") {
    if (text === null) return <span className="shrink-0 text-xs text-warn">No alt attribute</span>;
    if (text === "") return <span className="shrink-0 text-xs text-warn">Empty alt</span>;
  }
  if (!text) return null;
  return (
    <span className="max-w-2/5 shrink-0 truncate text-fg-3" title={text}>
      {text}
    </span>
  );
}

/** Link positions (spec 023 US4), as the lists show them. */
const positionLabel: Record<string, string> = {
  navigation: "Navigation",
  header: "Header",
  content: "Content",
  sidebar: "Sidebar",
  footer: "Footer",
  unknown: "Position unknown",
};

/** Filter by position on the Inlinks and Outlinks lists (FR-024). */
function PositionFilter({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <Select aria-label="Link position" className="mb-1 w-44 self-start" value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">All positions</option>
      {Object.entries(positionLabel).map(([v, label]) => (
        <option key={v} value={v}>
          {v === "unknown" ? "Unknown" : label}
        </option>
      ))}
    </Select>
  );
}

function LinkList({ transport, path, id, kind, total, actions }: { transport: Transport; path: string; id: number; kind: ListKind; total: number; actions: RowActions }) {
  const [page, setPage] = useState<ListPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [position, setPosition] = useState("");
  // Positions exist once Crawl Analysis has worked them out; the filter
  // shows from then on, even when it leaves the list empty.
  // Keyed by URL and list, so both reset when either changes.
  const [positioned, setPositioned] = useState(false);
  useEffect(() => {
    let off = false;
    setPage(null);
    setError(null);
    transport.urlList(path, id, kind, 0, LIST_PAGE, position || null).then(
      (p) => {
        if (off) return;
        setPage(p);
        if (p.rows.some((r) => r.position !== null)) setPositioned(true);
      },
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, kind, position]);
  const more = () =>
    page &&
    transport.urlList(path, id, kind, page.rows.length, LIST_PAGE, position || null).then(
      (p) => setPage({ ...p, from: 0, rows: [...page.rows, ...p.rows], headings: [...page.headings, ...p.headings] }),
      (e) => setError(String(e)),
    );
  const filter = positioned && (kind === "inlinks" || kind === "outlinks") && <PositionFilter value={position} onChange={setPosition} />;
  if (error) return <Notice kind="bad">{error}</Notice>;
  if (!page) return <div className="text-sm text-fg-3">Loading…</div>;
  if (page.unavailable) return <div className="text-sm text-fg-3">{page.unavailable}</div>;
  const shown = kind === "headings" ? page.headings.length : page.rows.length;
  // With a position chosen, the list's own total is the filtered count.
  const all = position ? page.total : total;
  if (shown === 0)
    return (
      <div className="flex flex-col">
        {filter}
        <div className="text-sm text-fg-3">{position ? "No links at this position." : "None."}</div>
      </div>
    );
  return (
    <div className="flex flex-col">
      {filter}
      {kind === "headings" ? (
        <ol className="m-0 flex list-none flex-col p-0" data-selectable>
          {page.headings.map((h, i) => (
            <li key={i} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className="w-6 shrink-0 text-xs font-medium text-fg-3">H{h.level}</span>
              <span className={cn("min-w-0 flex-1 break-words", h.level === 1 ? "text-fg" : "text-fg-2")}>{h.text || <span className="text-fg-3">(empty)</span>}</span>
            </li>
          ))}
        </ol>
      ) : (
      <ul className="m-0 flex list-none flex-col p-0" data-selectable>
        {page.rows.map((r, i) => {
          const tone = cellTone("status", r.status);
          return (
            <UrlRow key={`${r.id}-${i}`} actions={actions} id={r.id} url={r.url} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className={cn("w-8 shrink-0 text-right tabular", tone ? toneText[tone] : "text-fg-3")}>{r.status ?? "–"}</span>
              <span className="min-w-0 flex-1 truncate" title={r.url}>
                {r.url}
              </span>
              <LinkText kind={kind} text={r.text} />
              {r.position && (
                <span className="shrink-0 text-xs text-fg-3" title="Where the link sits on its page">
                  {positionLabel[r.position] ?? r.position}
                </span>
              )}
              {(r.nofollow || !r.followed || r.sponsored || r.ugc) && (
                <span className="shrink-0 text-xs text-fg-3">
                  {[r.nofollow ? "nofollow" : !r.followed ? "not followed" : null, r.sponsored && "sponsored", r.ugc && "ugc"].filter(Boolean).join(", ")}
                </span>
              )}
            </UrlRow>
          );
        })}
      </ul>
      )}
      {shown < all && (
        <Button size="sm" variant="quiet" className="mt-1 self-start" onClick={() => void more()}>
          Show more ({(all - shown).toLocaleString()} left)
        </Button>
      )}
    </div>
  );
}

const pairStatus: Record<string, string> = {
  too_short: "Not compared: too little main content.",
  not_indexable: "Not compared: the page is not indexable.",
  no_signature: "Not compared: no content was read from this page.",
};

/** This page's closest matches (US1, FR-018). */
function Duplicates({ transport, path, id, address, generation, onCompare }: { transport: Transport; path: string; id: number; address: string; generation: number; onCompare: OpenCompare }) {
  const [page, setPage] = useState<PairsPage | null>(null);
  useEffect(() => {
    let off = false;
    setPage(null);
    transport.nearDupPairs(path, id).then(
      (p) => !off && setPage(p),
      () => {},
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, generation]);
  if (!page || page.status === null) return null;
  const targets = page.rows.map((r) => ({ id: r.other_id, address: r.address, sim: r.sim }));
  const near = page.rows.filter((r) => r.sim >= (page.threshold ?? 90)).length;
  const title =
    page.near_count === 0
      ? "Duplicates"
      : near < page.near_count || page.near_count_is_min
        ? `Duplicates (${near} of ${page.near_count_is_min ? "at least " : ""}${page.near_count.toLocaleString()})`
        : `Duplicates (${page.near_count.toLocaleString()})`;
  return (
    <Section title={title}>
      {page.status !== "compared" ? (
        <div className="text-sm text-fg-3">{pairStatus[page.status] ?? page.status}</div>
      ) : page.rows.length === 0 ? (
        <div className="text-sm text-fg-3">No other page is at least {page.threshold ?? 90}% similar.</div>
      ) : (
        <ul className="m-0 flex list-none flex-col p-0" data-selectable aria-label="Closest matches">
          {page.rows.map((r, i) => (
            <li key={r.other_id} className="flex items-center gap-2 border-b border-line py-1 text-sm last:border-b-0">
              <span className={cn("w-10 shrink-0 text-right tabular", r.sim >= (page.threshold ?? 90) ? "text-fg" : "text-fg-3")}>{r.sim}%</span>
              <span className="min-w-0 flex-1 truncate" title={r.address}>
                {r.address}
              </span>
              {r.exact && <Badge tone="warn">Exact</Badge>}
              <Button size="sm" variant="quiet" icon aria-label={`Compare with ${r.address}`} onClick={() => onCompare({ base: { id, address }, targets, index: i })}>
                <Columns2 aria-hidden className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

/**
 * The URL's issues, highest priority first; a click opens that issue (spec
 * 009 FR-017). Link issues list their targets, each openable in the table.
 */
function IssueList({
  issues,
  rulesVersion,
  onIssue,
  onOpenUrl,
}: {
  issues: UrlIssue[] | null;
  rulesVersion?: number;
  onIssue?: (id: string) => void;
  onOpenUrl?: (url: string) => void;
}) {
  if (!issues) return <div className="text-sm text-fg-3">Loading…</div>;
  if (issues.length === 0) return <div className="text-sm text-fg-3">No issues found{rulesVersion ? ` (rule set v${rulesVersion})` : ""}.</div>;
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {issues.map((i) => (
        <li key={i.id} className="border-b border-line py-1 last:border-b-0">
          <button type="button" onClick={() => onIssue?.(i.id)} className="flex w-full items-baseline gap-2 text-left text-sm hover:text-accent-text">
            <PriorityBadge priority={i.priority} className="shrink-0" />
            <span className="min-w-0 flex-1">
              <span className="text-fg">{i.name}</span>
              {i.evidence && <span className="block break-words text-fg-3" data-selectable>{i.evidence}</span>}
            </span>
          </button>
          {i.targets.length > 0 && (
            <ul aria-label={`${i.name}: targets`} className="m-0 mt-1 flex max-h-40 list-none flex-col overflow-y-auto p-0 pl-6">
              {i.targets.map((t) => (
                <li key={t.id} className="flex items-baseline gap-2 text-sm">
                  <span className={cn("w-8 shrink-0 tabular", toneText[cellTone("status", t.status) as keyof typeof toneText] ?? "text-fg-3")}>{t.status ?? "–"}</span>
                  <button type="button" onClick={() => onOpenUrl?.(t.url)} className="min-w-0 flex-1 truncate text-left text-fg-2 hover:text-accent-text" title={`Show ${t.url} in the table`}>
                    {t.url}
                  </button>
                </li>
              ))}
              {i.targets_total > i.targets.length && (
                <li className="text-xs text-fg-3">and {(i.targets_total - i.targets.length).toLocaleString()} more</li>
              )}
            </ul>
          )}
        </li>
      ))}
    </ul>
  );
}

/** What each custom rule found on this page (spec 013 FR-024); nothing when the crawl has no rules. */
function CustomSection({
  transport,
  path,
  id,
  epoch,
  rules,
  htmlStored,
}: {
  transport: Transport;
  path: string;
  id: number;
  epoch: number;
  rules?: CustomRules;
  htmlStored: boolean;
}) {
  const [results, setResults] = useState<CustomResult[]>([]);
  const [tried, setTried] = useState<RuleTestOutcome | string | null>(null);
  useEffect(() => setTried(null), [path, id]);
  const canTry = htmlStored && rules !== undefined && rules.extractors.length + rules.searches.length > 0;
  // The settings' rules on this page's stored HTML, before running them on every page (US4).
  const tryRules = () =>
    rules &&
    transport.customTestPage(rules, path, id).then(
      (o) => setTried(o),
      (e) => setTried(String(e)),
    );
  useEffect(() => {
    let off = false;
    transport.urlCustom(path, id).then(
      (r) => !off && setResults(r),
      () => !off && setResults([]),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, epoch]);
  if (results.length === 0 && !canTry) return null;
  const shown: CustomResult[] = tried && typeof tried !== "string" ? tried.results.map((r) => ({ ...r, run_state: null })) : results;
  return (
    <Section
      title={tried && typeof tried !== "string" ? "Custom rules (settings, not saved)" : "Custom rules"}
      aside={
        canTry &&
        (tried ? (
          <Button size="sm" variant="quiet" onClick={() => setTried(null)}>
            Show stored
          </Button>
        ) : (
          <Button size="sm" variant="quiet" onClick={() => void tryRules()}>
            Test settings rules
          </Button>
        ))
      }
    >
      {typeof tried === "string" && <Notice kind="bad">{tried}</Notice>}
      <dl className="m-0 flex flex-col gap-2 text-sm">
        {shown.map((r, i) => (
          <div key={`${r.kind}-${i}`} className="flex flex-col gap-0.5">
            <dt className="flex items-baseline gap-2 text-fg-2">
              <span className="min-w-0 truncate font-medium">{r.name}</span>
              <span className="ml-auto shrink-0 text-xs text-fg-3 tabular">
                {r.count === null ? "Not run" : `${r.count.toLocaleString()} ${r.kind === "search" ? (r.count === 1 ? "match" : "matches") : r.count === 1 ? "value" : "values"}`}
              </span>
            </dt>
            <dd className="m-0 min-w-0">
              {r.count !== null && <RuleFound kind={r.kind} flags={r.flags} values={r.values} error={r.error} />}
              {r.run_state === "stale" && <p className="m-0 text-xs text-warn">The rule changed after it ran. Run the rules again to update.</p>}
            </dd>
          </div>
        ))}
      </dl>
    </Section>
  );
}

const actionKinds: Record<string, string> = {
  anchor_no_href: "Link without an address",
  anchor_hash: "Link to #",
  anchor_js: "javascript: link",
  onclick: "Click handler",
  role_button: "Button role",
  role_link: "Link role",
};

const linkKinds: Record<string, string> = {
  markdown_alternate: "Markdown version",
  service_desc: "API description",
  service_doc: "API documentation",
  api_catalog: "API catalog",
};

/** A field's accessible name, or why an agent cannot tell what it is for. */
function FieldName({ f }: { f: AgentField }) {
  if (f.name_source === "none") return <span className="text-warn">No label</span>;
  return (
    <span className="min-w-0 truncate text-fg" title={f.accessible_name ?? undefined}>
      {f.accessible_name}
      {f.name_source === "placeholder" && <span className="ml-1.5 text-xs text-warn">placeholder only</span>}
    </span>
  );
}

/** How an AI agent would see this page's forms and actions (spec 016 FR-019). */
function AgentsSection({ transport, path, id }: { transport: Transport; path: string; id: number }) {
  const [f, setF] = useState<PageAgentFindings | null>(null);
  useEffect(() => {
    let off = false;
    setF(null);
    transport.agentsPage(path, id).then(
      (x) => !off && setF(x),
      () => !off && setF(null),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id]);
  if (!f || f.id !== id || !f.available || !f.checked) return null;
  const empty = f.forms.length === 0 && f.actions_total === 0 && f.nameless_buttons === 0 && f.links.length === 0;
  const count = (n: number) => <span className={n > 0 ? "text-warn" : undefined}>{n.toLocaleString()}</span>;
  return (
    <Section title="AI agents">
      {empty ? (
        <p className="m-0 text-sm text-fg-3">No forms or actions on this page.</p>
      ) : (
        <>
          <Facts
            rows={[
              ["Forms", f.forms.length.toLocaleString()],
              ["Fields without a label", count(f.unlabelled_fields)],
              ["Buttons without a name", count(f.nameless_buttons)],
              ["Actions without a link", count(f.actions_total)],
            ]}
          />
          {f.forms.map((form, i) => (
            <div key={i} aria-label={`Form ${i + 1}`} role="group" className="mt-1 flex flex-col gap-1 rounded-md border border-line p-2">
              <div className="flex flex-wrap items-center gap-1.5 text-sm">
                <span className="font-medium text-fg uppercase">{form.method}</span>
                <span className="min-w-0 flex-1 truncate text-fg-2" title={form.action ?? undefined} data-selectable>
                  {form.action ?? "this page"}
                </span>
                {form.source === "rendered" && <Badge tone="info">After JavaScript</Badge>}
                {form.is_search && <Badge>Search</Badge>}
                {form.tool_name && <Badge tone="ok">Agent tool: {form.tool_name}</Badge>}
                {form.challenge && <Badge tone="warn">CAPTCHA: {form.challenge}</Badge>}
                {!form.has_submit && <Badge tone="warn">No submit button</Badge>}
              </div>
              {form.fields.length > 0 && (
                <ul className="m-0 flex list-none flex-col p-0" data-selectable>
                  {form.fields.map((fl, j) => (
                    <li key={j} className="flex items-baseline gap-2 border-t border-line py-0.5 text-sm">
                      <span className="w-20 shrink-0 truncate font-mono text-xs text-fg-3" title={fl.name ?? undefined}>
                        {fl.name ?? "–"}
                      </span>
                      <span className="w-16 shrink-0 text-xs text-fg-3">{fl.kind}</span>
                      <FieldName f={fl} />
                      {fl.required && <span className="ml-auto shrink-0 text-xs text-fg-3">required</span>}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ))}
          {f.actions.length > 0 && (
            <ul aria-label="Actions without a link" className="m-0 mt-1 flex list-none flex-col p-0" data-selectable>
              {f.actions.map((a, i) => (
                <li key={i} className="flex items-baseline gap-2 border-b border-line py-1 text-sm last:border-b-0">
                  <span className="min-w-0 flex-1 truncate text-fg">{a.text || <span className="text-fg-3">(no text)</span>}</span>
                  <span className="shrink-0 text-xs text-fg-3">{actionKinds[a.kind] ?? a.kind}</span>
                </li>
              ))}
            </ul>
          )}
          {f.links.length > 0 && (
            <Facts rows={f.links.map((l) => [linkKinds[l.kind] ?? l.kind, <span className="break-all">{l.href}</span>] as [string, ReactNode])} />
          )}
        </>
      )}
    </Section>
  );
}

export function Inspector({
  transport,
  path,
  id,
  duplicates,
  onIgnoreWord,
  ignored = [],
  issuesVersion = 0,
  rulesVersion,
  onIssue,
  onOpenUrl,
  epoch = 0,
  rules,
  onAnchorCloud,

  onShowInTable,
  aiChecks = false,
}: {
  transport: Transport;
  path: string | null;
  id: number | null;
  /** Shows the page's near duplicates when the crawl has been analysed (spec 020). */
  duplicates?: { generation: number; onCompare: OpenCompare };
  /** Adds a word to the crawl settings' ignore list. */
  onIgnoreWord?: (word: string) => void;
  ignored?: string[];
  /** Changes when issue settings change, so the list is fetched again. */
  issuesVersion?: number;
  onIssue?: (ruleId: string) => void;
  /** The issue rule set version, named when a URL has no issues (FR-013). */
  rulesVersion?: number;
  /** Shows a URL in the Internal table (an issue's link target). */
  onOpenUrl?: (url: string) => void;
  epoch?: number;
  /** The custom rules in the settings, to try on this page (spec 013 US4). */
  rules?: CustomRules;
  /** Opens the anchor text cloud of this URL's inlinks (spec 022 US4). */
  onAnchorCloud?: () => void;

  /** Selects the URL in a results tab; false when no tab lists it. */
  onShowInTable?: (id: number) => Promise<boolean>;
  aiChecks?: boolean;
}) {
  const [detail, setDetail] = useState<UrlDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [issues, setIssues] = useState<UrlIssue[] | null>(null);
  const sitemapEntries = useSitemapEntries(transport, path ?? "", path ? id : null);

  useEffect(() => {
    if (!path || id === null) return;
    let off = false;
    setIssues(null);
    transport.urlIssues(path, id).then(
      (r) => !off && setIssues(r),
      () => !off && setIssues([]),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id, issuesVersion]);
  const [list, setList] = useState<ListKind>("inlinks");
  const [htmlOpen, setHtmlOpen] = useState(false);
  const [status, setStatus] = useState<{ kind: "info" | "warn" | "bad"; text: string } | null>(null);
  const statusTimer = useRef<ReturnType<typeof setTimeout>>();
  const say = (kind: "info" | "warn" | "bad", text: string) => {
    clearTimeout(statusTimer.current);
    setStatus({ kind, text });
    statusTimer.current = setTimeout(() => setStatus(null), 4000);
  };
  useEffect(() => () => clearTimeout(statusTimer.current), []);
  const actions: RowActions = {
    copy: (url) =>
      void transport.copyText(url).then(
        () => say("info", "Copied."),
        (e) => say("bad", String(e)),
      ),
    show: onShowInTable
      ? (target) =>
          void onShowInTable(target).then(
            (found) => !found && say("warn", "That URL is not in any results table of this crawl."),
            (e) => say("bad", String(e)),
          )
      : undefined,
  };

  useEffect(() => {
    if (!path || id === null) {
      setDetail(null);
      return;
    }
    let off = false;
    setError(null);
    transport.urlDetail(path, id).then(
      (d) => !off && setDetail(d),
      (e) => !off && setError(String(e)),
    );
    return () => {
      off = true;
    };
  }, [transport, path, id]);

  if (!path || id === null) {
    return (
      <aside aria-label="Details" className="h-full overflow-y-auto bg-surface">
        <EmptyState icon={<MousePointerClick className="size-6" />} title="Nothing selected">
          Select a URL in the results to see its response, redirects and links here.
        </EmptyState>
      </aside>
    );
  }
  if (error) {
    return (
      <aside aria-label="Details" className="h-full overflow-y-auto bg-surface p-3">
        <Notice kind="bad">{error}</Notice>
      </aside>
    );
  }
  if (!detail || detail.id !== id) return <aside aria-label="Details" className="h-full bg-surface" aria-busy />;

  const d = detail;
  const tone = stateTone(d);
  const facts: [string, ReactNode][] = [
    ["Status", d.status !== null ? `${d.status}${d.status_text ? ` ${d.status_text}` : ""}` : "No response"],
    ["Content type", d.content_type ?? "–"],
    ["Depth", d.depth.toLocaleString()],
    ["Found on", d.found_on ? <span className="break-all">{d.found_on}</span> : "Start URL"],
    ["Discovered via", d.discovered_via],
    ["In sitemap", d.in_sitemap ? "Yes" : "No"],
  ];
  if (d.size !== null) facts.push(["Size", formatBytes(d.size)]);
  if (d.response_ms !== null) facts.push(["Response time", `${d.response_ms.toLocaleString()} ms`]);
  if (d.attempts !== null && (d.attempts > 1 || d.manual_retry)) {
    facts.push(["Attempts", `${d.attempts}${d.manual_retry ? ", last by Retry failed URLs" : ""}`]);
  }
  if (d.indexability) {
    const non = d.indexability.startsWith("Non-indexable");
    facts.push(["Indexability", <span className={non ? "text-warn" : undefined}>{d.indexability}</span>]);
  }
  if (d.fetched_at !== null) facts.push(["Fetched", new Date(d.fetched_at).toLocaleString()]);
  const counts: Record<ListKind, number> = { inlinks: d.inlinks, outlinks: d.outlinks, images: d.images, headings: d.headings };
  // Fields the file has no data for, grouped by why. Page HTML has its own
  // section, which says how to get it.
  const missing = new Map<string, string[]>();
  for (const [k, why] of Object.entries(d.unavailable)) if (k !== "raw_html") missing.set(why, [...(missing.get(why) ?? []), fieldLabels[k] ?? k]);

  return (
    <aside aria-label="Details" className="flex h-full flex-col overflow-y-auto bg-surface">
      <header className="flex flex-col gap-2 border-b border-line px-3 py-3">
        <div className="flex items-center gap-2">
          <Badge tone={tone}>{d.final_state}</Badge>
          {!d.internal && <Badge tone="neutral">External</Badge>}
        </div>
        <h2 className="m-0 text-base font-medium break-all text-fg" data-selectable title={d.url}>
          {d.url_readable}
        </h2>
      </header>

      {(d.error_label || d.not_crawled_reason) && (
        <Section title={d.error_label ? "Error" : "Not crawled"}>
          <Notice kind={d.error_label ? "bad" : "warn"}>
            <span data-selectable>
              {d.error_label ?? d.not_crawled_reason}
              {d.error_message ? `: ${d.error_message}` : d.reason_detail ? `: ${d.reason_detail}` : ""}
              {d.robots_rule ? ` (rule ${d.robots_rule})` : ""}
            </span>
          </Notice>
        </Section>
      )}

      <Section title={issues && issues.length > 0 ? `Issues (${issues.length})` : "Issues"}>
        <IssueList issues={issues} rulesVersion={rulesVersion} onIssue={onIssue} onOpenUrl={onOpenUrl} />
      </Section>

      <Section title="Response">
        <Facts rows={facts} />
      </Section>

      {d.link_metrics && <LinkScoreSection m={d.link_metrics} />}

      {aiChecks && d.internal && (
        <AiPageSection
          transport={transport}
          path={path}
          id={d.id}
          section={(title, body) => <Section title={title}>{body}</Section>}
        />
      )}

      {d.redirects.length > 0 && (
        <Section title={`Redirects (${d.redirects.length - 1} ${d.redirects.length === 2 ? "hop" : "hops"})`}>
          {d.redirect_loop && <Notice kind="bad">The chain loops back on itself.</Notice>}
          {d.redirect_over_limit && <Notice kind="warn">The chain is longer than the crawl's redirect limit.</Notice>}
          <ol className="m-0 flex list-none flex-col gap-1 p-0 text-sm" data-selectable>
            {d.redirects.map((h, i) => {
              const t = cellTone("status", h.status);
              return (
                <UrlRow key={`${h.id}-${i}`} actions={actions} id={h.id} url={h.url} className="flex items-baseline gap-1.5">
                  {i > 0 ? <CornerDownRight aria-hidden className="size-3 shrink-0 text-fg-3" /> : <span className="w-3 shrink-0" />}
                  <span className={cn("w-8 shrink-0 tabular", t ? toneText[t] : "text-fg-3")}>{h.status ?? "–"}</span>
                  <span className="min-w-0 flex-1 break-all">{h.url}</span>
                </UrlRow>
              );
            })}
          </ol>
        </Section>
      )}

      {d.in_sitemap && (
        <Section title={`Sitemaps (${sitemapEntries.length})`}>
          <SitemapEntryList entries={sitemapEntries} />
        </Section>
      )}

      {duplicates && d.internal && <Duplicates transport={transport} path={path} id={d.id} address={d.url} generation={duplicates.generation} onCompare={duplicates.onCompare} />}
      {d.pdf && <PdfSection pdf={d.pdf} />}
      {d.snippet && (
        <Section title="Search preview">
          <SearchPreview url={d.url} snippet={d.snippet} />
        </Section>
      )}

      <Section
        title="Links"
        aside={
          <Segmented<ListKind>
            size="sm"
            label="Link list"
            value={list}
            onChange={setList}
            options={[
              { value: "inlinks", label: `In ${d.inlinks.toLocaleString()}` },
              { value: "outlinks", label: `Out ${d.outlinks.toLocaleString()}` },
              { value: "images", label: `Images ${d.images.toLocaleString()}` },
              ...(d.unavailable.headings ? [] : [{ value: "headings" as const, label: `Headings ${d.headings.toLocaleString()}` }]),
            ]}
          />
        }
      >
        <LinkList key={`${path}|${d.id}|${list}`} transport={transport} path={path} id={d.id} kind={list} total={counts[list]} actions={actions} />
        {list === "inlinks" && d.inlinks > 0 && onAnchorCloud && (
          <Button size="sm" variant="quiet" className="mt-1 self-start" onClick={onAnchorCloud}>
            Anchor text cloud
          </Button>
        )}
      </Section>

      <AgentsSection transport={transport} path={path} id={d.id} />
      {d.amp && <AmpSection amp={d.amp} />}
      {d.accessibility && <AccessibilitySection a={d.accessibility} />}
      {d.mobile && <MobileSection m={d.mobile} />}
      <CustomSection transport={transport} path={path} id={d.id} epoch={epoch} rules={rules} htmlStored={d.html_stored} />
      {d.spelling && <SpellingSection spell={d.spelling} ignored={ignored} onIgnore={onIgnoreWord} />}

      {d.headers.length > 0 && (
        <Section title={`Response headers (${d.headers.length})`}>
          <dl className="m-0 flex flex-col gap-0.5 font-mono text-xs" data-selectable>
            {d.headers.map(([k, v], i) => (
              <div key={`${k}-${i}`} className="flex gap-2">
                <dt className="shrink-0 text-fg-3">{k}</dt>
                <dd className="m-0 min-w-0 break-all text-fg-2">{v}</dd>
              </div>
            ))}
          </dl>
        </Section>
      )}

      <Section
        title="Page HTML"
        aside={
          d.html_stored && (
            <Button size="sm" onClick={() => setHtmlOpen(true)}>
              <FileCode className="size-3.5" /> View source
            </Button>
          )
        }
      >
        <p className="m-0 text-sm text-fg-3">
          {d.html_stored
            ? "The page's source as received, searchable and ready to copy."
            : d.unavailable.raw_html
              ? "Not stored in this crawl. Turn on Settings › Page data › Store page HTML and crawl again to keep each page's source."
              : "None stored for this URL: it is not an HTML page or no body was received."}
        </p>
        <HtmlViewer transport={transport} path={path} id={d.id} url={d.url} open={htmlOpen} onOpenChange={setHtmlOpen} />
      </Section>
      {d.cookies.length > 0 && (
        <Section title={`Cookies set (${d.cookies.length})`}>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label="Cookies set" data-selectable>
            {d.cookies.map((c) => (
              <li key={`${c.name}\n${c.domain}\n${c.path}`} className="flex flex-col gap-0.5">
                <div className="flex min-w-0 items-baseline gap-2 text-sm">
                  <span className="min-w-0 truncate font-mono text-fg">{c.name}</span>
                  <span className="min-w-0 truncate text-fg-3">
                    {c.domain}
                    {c.path !== "/" && c.path}
                  </span>
                  <span className="ml-auto shrink-0 text-xs text-fg-3">{expiresText(c)}</span>
                </div>
                <CookieFlags c={c} />
              </li>
            ))}
          </ul>
        </Section>
      )}

      {missing.size > 0 && (
        <Section title="Not collected">
          {[...missing].map(([why, fields]) => (
            <p key={why} className="m-0 text-sm text-fg-3">
              <span className="text-fg-2">{fields.join(", ")}.</span> {why}
            </p>
          ))}
        </Section>
      )}

      {/* Copy and show-in-table results, announced as they happen. */}
      <div role="status" aria-live="polite" className="sticky bottom-0 mt-auto">
        {status && (
          <Notice kind={status.kind} className="m-2 rounded-md border border-line bg-surface px-2 py-1.5 shadow-popover">
            {status.text}
          </Notice>
        )}
      </div>
    </aside>
  );
}
