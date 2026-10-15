import { useEffect, useRef, useState } from "react";
import { FolderOpen } from "lucide-react";
import type { Changefreq } from "../generated/Changefreq";
import type { GenerateReport } from "../generated/GenerateReport";
import type { ImageEntries } from "../generated/ImageEntries";
import type { SitemapGenerateOptions } from "../generated/SitemapGenerateOptions";
import type { Transport } from "../transport";
import { Button, Input, Kbd, Notice, NumberInput, Segmented, Select, Sheet, SheetClose, Switch, Textarea } from "../components/ui";
import { Group, Row } from "../settings/fields";

const FREQS: Changefreq[] = ["always", "hourly", "daily", "weekly", "monthly", "yearly", "never"];
const lines = (v: string) => (v === "" ? [] : v.split("\n"));
const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
const message = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/**
 * Generate XML sitemap (spec 011 US5, FR-017 to FR-021): options on the
 * left of each row, a live count of what would be written, and the folder.
 * The options are the profile's `sitemap_generate`, so the next crawl of the
 * site starts from the same choices.
 */
export function GenerateSitemapSheet({
  open,
  onOpenChange,
  transport,
  path,
  running,
  options,
  onOptions,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  transport: Transport;
  /** The open crawl file. */
  path: string;
  /** A crawl is running: generating waits for it to finish. */
  running: boolean;
  options: SitemapGenerateOptions;
  onOptions: (o: SitemapGenerateOptions) => void;
}) {
  const [folder, setFolder] = useState<string | null>(null);
  const [preview, setPreview] = useState<GenerateReport | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [written, setWritten] = useState<GenerateReport | null>(null);
  const set = (patch: Partial<SitemapGenerateOptions>) => {
    setWritten(null);
    onOptions({ ...options, ...patch });
  };

  // Counts follow the options as they change; a newer request wins.
  const seq = useRef(0);
  useEffect(() => {
    if (!open) return;
    const n = ++seq.current;
    const t = setTimeout(() => {
      transport.sitemapPreview(path, options, folder).then(
        (r) => n === seq.current && (setPreview(r), setProblem(null)),
        (e) => n === seq.current && (setPreview(null), setProblem(message(e))),
      );
    }, 150);
    return () => clearTimeout(t);
  }, [open, transport, path, options, folder]);

  const chooseFolder = async () => {
    const f = await transport.pickFolder();
    if (f) (setFolder(f), setWritten(null));
  };
  const replace = (preview?.existing.length ?? 0) > 0;
  const generate = async () => {
    if (!folder) return;
    setBusy(true);
    try {
      const r = await transport.sitemapGenerate(path, options, folder, replace);
      setWritten(r);
      setProblem(null);
      setPreview({ ...r, existing: r.files.map((f) => f.name) });
    } catch (e) {
      setProblem(message(e));
    } finally {
      setBusy(false);
    }
  };

  const lastmodKind = options.lastmod.kind;
  const priorityKind = options.priority.kind;
  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Generate XML sitemap" className="h-160 w-150 sheet-max-w">
      <header className="flex flex-col gap-0.5 px-6 pt-5 pb-3">
        <h2 className="m-0 text-md font-semibold">Generate XML sitemap</h2>
        <p className="m-0 text-sm text-fg-3">
          Indexable pages that answered 200, with no canonical pointing elsewhere. Files over 50,000 URLs or 50 MB are split, with an index.
        </p>
      </header>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-4">
        <Group title="Pages">
          <Row label="Include pages marked noindex">
            {(_, l) => <Switch labelledBy={l} checked={options.include_noindex} onChange={(include_noindex) => set({ include_noindex })} />}
          </Row>
          <Row label="Include pages canonicalised elsewhere">
            {(_, l) => <Switch labelledBy={l} checked={options.include_canonicalised} onChange={(include_canonicalised) => set({ include_canonicalised })} />}
          </Row>
          <Row label="Only addresses matching" stacked help="One pattern per line. Blank keeps every page.">
            {(i) => <Textarea id={i} rows={2} placeholder="^https://example\.com/blog/" value={options.include.join("\n")} onChange={(e) => set({ include: lines(e.target.value) })} />}
          </Row>
          <Row label="Leave out addresses matching" stacked help="One pattern per line.">
            {(i) => <Textarea id={i} rows={2} placeholder="/tag/" value={options.exclude.join("\n")} onChange={(e) => set({ exclude: lines(e.target.value) })} />}
          </Row>
        </Group>

        <Group title="Fields">
          <Row label="Last modified" help={lastmodKind === "last_modified_header" ? "From the page's Last-Modified header; left out when there is none." : undefined}>
            {(i) => (
              <div className="flex items-center gap-2">
                {lastmodKind === "fixed" && (
                  <Input
                    aria-label="Last modified date"
                    type="date"
                    value={options.lastmod.kind === "fixed" ? options.lastmod.date : ""}
                    onChange={(e) => set({ lastmod: { kind: "fixed", date: e.target.value } })}
                  />
                )}
                <Select
                  id={i}
                  value={lastmodKind}
                  onChange={(e) => {
                    const k = e.target.value as typeof lastmodKind;
                    set({ lastmod: k === "fixed" ? { kind: "fixed", date: new Date().toISOString().slice(0, 10) } : { kind: k } });
                  }}
                >
                  <option value="last_modified_header">Last-Modified header</option>
                  <option value="crawl_date">Crawl date</option>
                  <option value="fixed">A date</option>
                  <option value="none">Leave out</option>
                </Select>
              </div>
            )}
          </Row>
          <Row label="Change frequency">
            {(i) => (
              <Select id={i} value={options.changefreq ?? ""} onChange={(e) => set({ changefreq: e.target.value === "" ? null : (e.target.value as Changefreq) })}>
                <option value="">Leave out</option>
                {FREQS.map((f) => (
                  <option key={f} value={f}>
                    {f[0].toUpperCase() + f.slice(1)}
                  </option>
                ))}
              </Select>
            )}
          </Row>
          <Row label="Priority" help={priorityKind === "by_depth" ? "1.0 for the start page, 0.1 less per click, down to 0.1." : undefined}>
            {(i) => (
              <div className="flex items-center gap-2">
                {options.priority.kind === "fixed" && (
                  <NumberInput aria-label="Priority value" min={0} max={1} step={0.1} value={options.priority.value} onChange={(v) => set({ priority: { kind: "fixed", value: v ?? 0.5 } })} />
                )}
                <Select
                  id={i}
                  value={priorityKind}
                  onChange={(e) => {
                    const k = e.target.value as typeof priorityKind;
                    set({ priority: k === "fixed" ? { kind: "fixed", value: 0.5 } : { kind: k } });
                  }}
                >
                  <option value="off">Leave out</option>
                  <option value="by_depth">By click depth</option>
                  <option value="fixed">The same for all</option>
                </Select>
              </div>
            )}
          </Row>
          <Row label="Images" help="Images that answered 200, on this site or the hosts below, up to 1,000 per page.">
            {() => (
              <Segmented<ImageEntries>
                label="Images"
                size="sm"
                value={options.images}
                onChange={(images) => set({ images })}
                options={[
                  { value: "off", label: "Off" },
                  { value: "inline", label: "In the page sitemap" },
                  { value: "separate", label: "Separate file" },
                ]}
              />
            )}
          </Row>
          {options.images !== "off" && (
            <Row label="Image hosts" stacked help="Other hosts whose images may be listed, such as a CDN. One per line.">
              {(i) => <Textarea id={i} rows={2} placeholder="cdn.example.com" value={options.image_hosts.join("\n")} onChange={(e) => set({ image_hosts: lines(e.target.value) })} />}
            </Row>
          )}
          <Row label="hreflang alternates" help="Only alternates that are themselves in the sitemap.">
            {(_, l) => <Switch labelledBy={l} checked={options.hreflang} onChange={(hreflang) => set({ hreflang })} />}
          </Row>
        </Group>

        <Group title="Files">
          <Row label="URLs per file" help="At most 50,000.">
            {(i) => (
              <NumberInput id={i} min={1} max={50000} value={options.max_urls_per_file} onChange={(v) => set({ max_urls_per_file: v === null || Number.isNaN(v) ? 50000 : v })} />
            )}
          </Row>
          <Row label="Compress (.xml.gz)">{(_, l) => <Switch labelledBy={l} checked={options.gzip} onChange={(gzip) => set({ gzip })} />}</Row>
          <Row label="File name" help="Files are named after it, for example sitemap.xml or sitemap-2.xml.">
            {(i) => <Input id={i} mono className="w-40" value={options.file_prefix} onChange={(e) => set({ file_prefix: e.target.value })} />}
          </Row>
          <Row label="Published at" help="The folder address the files will have. Blank means the root of the start host.">
            {(i) => <Input id={i} mono className="w-64" placeholder={preview?.base_url ?? "https://example.com/"} value={options.base_url} onChange={(e) => set({ base_url: e.target.value })} />}
          </Row>
          <Row label="Folder" stacked>
            {() => (
              <div className="flex items-center gap-2">
                <Input aria-label="Folder" mono readOnly className="min-w-0 flex-1" placeholder="No folder chosen" value={folder ?? ""} title={folder ?? undefined} />
                <Button size="sm" onClick={() => void chooseFolder()}>
                  <FolderOpen className="size-3.5" /> Choose folder…
                </Button>
              </div>
            )}
          </Row>
        </Group>
      </div>
      <footer className="flex flex-col gap-2 border-t border-line px-6 py-3">
        <div aria-live="polite" className="flex flex-col gap-1">
          {problem ? (
            <Notice kind="bad" role="alert">
              {problem}
            </Notice>
          ) : written ? (
            <Notice>
              Wrote {plural(written.files.length, "file")} with {plural(written.pages, "page")} to {folder}.
            </Notice>
          ) : preview ? (
            <Notice>
              {plural(preview.pages, "page")}
              {preview.images > 0 && `, ${plural(preview.images, "image")}`} in {plural(preview.files.length, "file")}
              {preview.skipped_by_pattern > 0 && `; ${plural(preview.skipped_by_pattern, "page")} left out by patterns`}.
            </Notice>
          ) : null}
          {!written && replace && <Notice kind="warn">Replaces {preview!.existing.join(", ")} in this folder.</Notice>}
          {running && <Notice kind="warn">Sitemaps can be generated once the crawl has finished.</Notice>}
        </div>
        <div className="flex items-center justify-end gap-2">
          <SheetClose render={<Button />}>
            {written ? "Done" : "Cancel"} <Kbd keys="Escape" />
          </SheetClose>
          <Button variant="primary" disabled={!folder || !preview || running || busy || written !== null} onClick={() => void generate()}>
            {replace ? "Replace and generate" : "Generate"}
          </Button>
        </div>
      </footer>
    </Sheet>
  );
}
