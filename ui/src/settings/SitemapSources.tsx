import { FolderOpen, Plus, X } from "lucide-react";
import type { Issue } from "../generated/Issue";
import type { SitemapSettings } from "../generated/SitemapSettings";
import type { Transport } from "../transport";
import { Button, Input, Switch, Textarea } from "../components/ui";
import { IssueList, issuesAt, Row } from "./fields";

/** Settings › Scope › Sitemaps (spec 011 FR-001, FR-002, FR-009). */
export function SitemapSources({
  value,
  onChange,
  issues,
  transport,
}: {
  value: SitemapSettings;
  onChange: (s: SitemapSettings) => void;
  issues: Issue[];
  transport: Transport;
}) {
  const set = (patch: Partial<SitemapSettings>) => onChange({ ...value, ...patch });
  const at = (p: string) => issuesAt(issues, `scope.sitemaps.${p}`);
  const addFile = async () => {
    const path = await transport.pickSitemapFile();
    if (path) set({ files: [...value.files, { path, base_url: "" }] });
  };
  return (
    <>
      <Row
        label="Read sitemaps"
        help="From robots.txt, or /sitemap.xml when robots.txt lists none, plus the ones below. Needed for the sitemap audit and orphan pages."
        issues={at("read")}
      >
        {(_, l) => <Switch labelledBy={l} checked={value.read} onChange={(read) => set({ read })} />}
      </Row>
      <Row label="Crawl URLs found only in sitemaps" help="When off, URLs that no page links to are listed as found only in a sitemap, without a request.">
        {(_, l) => (
          <Switch labelledBy={l} disabled={!value.read} checked={value.read && value.crawl_sitemap_only} onChange={(crawl_sitemap_only) => set({ crawl_sitemap_only })} />
        )}
      </Row>
      <Row label="More sitemaps" stacked help="One sitemap address per line, for sitemaps robots.txt does not list." issues={at("urls")}>
        {(i) => (
          <Textarea
            id={i}
            rows={2}
            placeholder="https://example.com/news-sitemap.xml"
            disabled={!value.read}
            value={value.urls.join("\n")}
            onChange={(e) => set({ urls: e.target.value === "" ? [] : e.target.value.split("\n") })}
          />
        )}
      </Row>
      <Row label="Sitemap files on this computer" stacked help="Check a sitemap before it is published. Give the address it will have, so its URLs are checked against the right location." issues={[]}>
        {() => (
          <div className="flex w-full flex-col gap-2">
            {value.files.map((f, i) => (
              <div key={i} className="flex flex-col gap-1">
                <div className="flex flex-wrap items-center gap-2">
                  <Input aria-label={`Sitemap file ${i + 1}`} mono readOnly className="min-w-0 flex-1" value={f.path} title={f.path} />
                  <Input
                    aria-label={`Sitemap file ${i + 1} address`}
                    placeholder="https://example.com/sitemap.xml"
                    mono
                    className="min-w-0 flex-1"
                    value={f.base_url}
                    onChange={(e) => set({ files: value.files.map((x, j) => (j === i ? { ...x, base_url: e.target.value } : x)) })}
                  />
                  <Button variant="quiet" icon size="sm" aria-label={`Remove sitemap file ${i + 1}`} onClick={() => set({ files: value.files.filter((_, j) => j !== i) })}>
                    <X className="size-3.5" />
                  </Button>
                </div>
                <IssueList issues={at(`files[${i}]`)} />
              </div>
            ))}
            <div>
              <Button size="sm" disabled={!value.read} onClick={() => void addFile()}>
                {value.files.length === 0 ? <FolderOpen className="size-3.5" /> : <Plus className="size-3.5" />} Add sitemap file…
              </Button>
            </div>
          </div>
        )}
      </Row>
    </>
  );
}
