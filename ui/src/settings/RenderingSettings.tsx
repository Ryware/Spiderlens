import { useEffect, useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Issue } from "../generated/Issue";
import type { RenderBlock } from "../generated/RenderBlock";
import type { RenderSettings } from "../generated/RenderSettings";
import type { WindowPreset } from "../generated/WindowPreset";
import type { BrowserStatus, Transport } from "../transport";
import { Button, Input, Notice, NumberInput, Segmented, Select, Switch, Textarea } from "../components/ui";
import { Group, Row } from "./fields";
import { PatternTester } from "./PatternTester";

const DEFAULT_BLOCK: RenderBlock = { images: true, media: true, fonts: true, stylesheets: false, third_party: false };
const LOAD_ALL: RenderBlock = { images: false, media: false, fonts: false, stylesheets: false, third_party: false };

const BLOCKS: { key: keyof RenderBlock; label: string; help: string }[] = [
  { key: "images", label: "Block images", help: "Pictures are not downloaded. Rendering is faster and the page text is the same." },
  { key: "media", label: "Block video and audio", help: "Media files are not downloaded." },
  { key: "fonts", label: "Block fonts", help: "Web fonts are not downloaded." },
  { key: "stylesheets", label: "Block stylesheets", help: "Style sheets are not downloaded. Pages that depend on them to show content may look empty." },
  { key: "third_party", label: "Block third-party resources", help: "Nothing is requested from other sites, such as analytics and ad scripts." },
];

/** One pattern per line. */
function PatternLines({ id, value, onChange, placeholder }: { id: string; value: string[]; onChange: (v: string[]) => void; placeholder: string }) {
  return (
    <Textarea
      id={id}
      rows={3}
      placeholder={placeholder}
      value={value.join("\n")}
      onChange={(e) => onChange(e.target.value === "" ? [] : e.target.value.split("\n"))}
    />
  );
}

/** The browser rendering would use, with "Change browser" (spec 014 FR-045). */
function BrowserLine({ transport }: { transport: Transport }) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [path, setPath] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void transport.renderBrowserStatus().then((s) => alive && setStatus(s));
    return () => {
      alive = false;
    };
  }, [transport]);

  const use = async () => {
    setError(null);
    try {
      const s = await transport.renderBrowserSetPath(path.trim() || null);
      setStatus(s);
      if (s.usable) setChoosing(false);
      else if (s.problem) setError(s.problem);
    } catch (e) {
      setError(String(e));
    }
  };
  const browse = async () => {
    const picked = await transport.pickBrowserFile();
    if (picked) setPath(picked);
  };

  const view = status?.usable ?? null;
  const source = view?.source === "downloaded" ? "downloaded by SpiderLens" : view?.source === "user_path" ? "chosen by you" : "installed";
  return (
    <Row
      label="Browser"
      stacked
      help="Pages are rendered in a headless Chromium browser. SpiderLens uses the one it downloaded, or an installed Chrome, Chromium or Edge that you choose."
    >
      {() => (
        <div className="flex w-full flex-col gap-2">
          <div className="flex items-center gap-3">
            <span className="min-w-0 flex-1 text-sm" role="status">
              {status === null ? "Looking for a browser" : view ? `${view.name} ${view.version} (${source})` : (status.problem ?? "No browser found")}
            </span>
            <Button size="sm" onClick={() => setChoosing((c) => !c)}>
              Change browser
            </Button>
          </div>
          {choosing && (
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <Input aria-label="Browser path" placeholder="Path to Chrome, Chromium or Edge" value={path} onChange={(e) => setPath(e.target.value)} />
                <Button size="sm" onClick={() => void browse()}>
                  Browse
                </Button>
                <Button size="sm" onClick={() => void use()}>
                  Use this browser
                </Button>
              </div>
              <p className="m-0 text-xs text-fg-3">Leave the path empty to go back to the detected browser.</p>
            </div>
          )}
          {error && <Notice kind="bad">{error}</Notice>}
        </div>
      )}
    </Row>
  );
}

/** Settings > JavaScript rendering (spec 014 FR-005, FR-008 to FR-010, FR-014, FR-027, FR-030, FR-037). */
export function RenderingSettings({
  config,
  set,
  at,
  transport,
}: {
  config: CrawlConfig;
  set: (f: (c: CrawlConfig) => CrawlConfig) => void;
  at: (path: string) => Issue[];
  transport: Transport;
}) {
  const r = config.rendering;
  const patch = (p: Partial<RenderSettings>) => set((c) => ({ ...c, rendering: { ...c.rendering, ...p } }));
  const on = r.mode === "javascript";
  const loadAll = !Object.values(r.block).some(Boolean);
  const num = (v: number | null, current: number) => (v === null || Number.isNaN(v) ? current : v);
  const patternIssues = (group: "include" | "exclude") => r[group].flatMap((_, i) => at(`rendering.${group}[${i}]`));

  return (
    <>
      <Group>
        <Row
          label="Mode"
          help={
            on
              ? "Each HTML page is fetched as usual, then rendered in a browser; the tabs show the rendered values."
              : "Pages are read from their HTML as served. No browser is started."
          }
          issues={at("rendering.mode")}
        >
          {() => (
            <Segmented
              label="Rendering mode"
              value={r.mode}
              options={[
                { value: "raw", label: "Raw HTML" },
                { value: "javascript", label: "JavaScript rendering" },
              ]}
              onChange={(mode) => patch({ mode })}
            />
          )}
        </Row>
        <BrowserLine transport={transport} />
      </Group>
      {on && (
        <>
          <Group title="Time">
            <Row label="Maximum render time" help="A page that has not settled by then is stored as it is and marked timed out (1 to 60 seconds)." issues={at("rendering.max_render_secs")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} value={r.max_render_secs} onChange={(v) => patch({ max_render_secs: v === null ? Number.NaN : v })} />
                  <span className="text-xs text-fg-3">seconds</span>
                </span>
              )}
            </Row>
            <Row label="Minimum wait" help="Every page waits this long before it may count as settled. Raise it for pages that start slowly." issues={at("rendering.min_wait_secs")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} value={r.min_wait_secs} onChange={(v) => patch({ min_wait_secs: v === null ? Number.NaN : v })} />
                  <span className="text-xs text-fg-3">seconds</span>
                </span>
              )}
            </Row>
          </Group>
          <Group title="Window">
            <Row label="Window" help="Auto uses a phone window for a mobile user agent, such as Googlebot smartphone, and a desktop window otherwise." issues={at("rendering.window").filter((x) => x.path !== "rendering.window.width" && x.path !== "rendering.window.height")}>
              {(i) => (
                <Select
                  id={i}
                  className="w-48"
                  value={r.window.preset}
                  onChange={(e) => {
                    const preset = e.target.value as WindowPreset;
                    patch({
                      window: {
                        preset,
                        width: preset === "custom" ? (r.window.width ?? 1024) : r.window.width,
                        height: preset === "custom" ? (r.window.height ?? 768) : r.window.height,
                      },
                    });
                  }}
                >
                  <option value="auto">Auto</option>
                  <option value="mobile">Mobile (411 x 731)</option>
                  <option value="desktop">Desktop (1024 x 768)</option>
                  <option value="custom">Custom size</option>
                </Select>
              )}
            </Row>
            {r.window.preset === "custom" && (
              <>
                <Row label="Window width" help="320 to 3,840 pixels." issues={at("rendering.window.width")}>
                  {(i) => <NumberInput id={i} value={r.window.width} optional onChange={(width) => patch({ window: { ...r.window, width: width === null || Number.isNaN(width) ? null : Math.round(width) } })} />}
                </Row>
                <Row label="Window height" help="480 to 4,320 pixels." issues={at("rendering.window.height")}>
                  {(i) => <NumberInput id={i} value={r.window.height} optional onChange={(height) => patch({ window: { ...r.window, height: height === null || Number.isNaN(height) ? null : Math.round(height) } })} />}
                </Row>
              </>
            )}
          </Group>
          <Group title="Resources">
            <Row label="Load all resources" help="Turn on to download everything a page asks for, for the most faithful result. Slower, and uses more of the site's capacity.">
              {(_, l) => <Switch labelledBy={l} checked={loadAll} onChange={(all) => patch({ block: all ? LOAD_ALL : DEFAULT_BLOCK })} />}
            </Row>
            {BLOCKS.map((b) => (
              <Row key={b.key} label={b.label} help={b.help}>
                {(_, l) => <Switch labelledBy={l} checked={r.block[b.key]} onChange={(v) => patch({ block: { ...r.block, [b.key]: v } })} />}
              </Row>
            ))}
            <Row label="Render error pages" help="Also render pages that answer with a 4xx or 5xx status and still send HTML.">
              {(_, l) => <Switch labelledBy={l} checked={r.render_error_pages} onChange={(render_error_pages) => patch({ render_error_pages })} />}
            </Row>
          </Group>
          <Group title="Which pages">
            <Row label="Only render pages matching" stacked help="One regular expression per line. Leave empty to render every page. Pages outside the rules are listed as not rendered." issues={patternIssues("include")}>
              {(i) => <PatternLines id={i} placeholder="/products/" value={r.include} onChange={(include) => patch({ include })} />}
            </Row>
            <Row label="Never render pages matching" stacked issues={patternIssues("exclude")}>
              {(i) => <PatternLines id={i} placeholder="\?print=" value={r.exclude} onChange={(exclude) => patch({ exclude })} />}
            </Row>
            <Row label="Try the patterns" stacked help="See which URLs the rendering patterns keep, and which pattern decided.">
              {() => <PatternTester transport={transport} include={r.include} exclude={r.exclude} />}
            </Row>
          </Group>
          <Group title="Resources used">
            <Row label="Pages rendered at once" help="Auto is the computer's memory in GB divided by 4, between 1 and 16. Rendering slows down by itself when memory runs short." issues={at("rendering.concurrency")}>
              {(i) => <NumberInput id={i} value={r.concurrency} optional placeholder="Auto" onChange={(v) => patch({ concurrency: v === null || Number.isNaN(v) ? null : Math.round(v) })} />}
            </Row>
            <Row label="Browser memory budget" help="Auto is one eighth of the computer's memory (256 to 65,536 MB)." issues={at("rendering.memory_budget_mb")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} className="w-28" value={r.memory_budget_mb} optional placeholder="Auto" onChange={(v) => patch({ memory_budget_mb: v === null || Number.isNaN(v) ? null : Math.round(v) })} />
                  <span className="text-xs text-fg-3">MB</span>
                </span>
              )}
            </Row>
            <Row label="Screenshots" help="Keeps a picture of every rendered page in the crawl file. Uses disk space.">
              {(_, l) => <Switch labelledBy={l} checked={r.screenshots} onChange={(screenshots) => patch({ screenshots })} />}
            </Row>
          </Group>
          <Group title="JavaScript dependence">
            <Row label="JavaScript content: new text" help="A page counts as containing JavaScript content when rendering adds at least this share of new text (1 to 1,000 percent)…" issues={at("rendering.js_content_min_percent")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} value={r.js_content_min_percent} onChange={(v) => patch({ js_content_min_percent: num(v, r.js_content_min_percent) })} />
                  <span className="text-xs text-fg-3">%</span>
                </span>
              )}
            </Row>
            <Row label="JavaScript content: new words" help="…and at least this many words (0 to 100,000)." issues={at("rendering.js_content_min_words")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} value={r.js_content_min_words} onChange={(v) => patch({ js_content_min_words: num(v, r.js_content_min_words) })} />
                  <span className="text-xs text-fg-3">words</span>
                </span>
              )}
            </Row>
            <Row label="Hidden text threshold" help="A page is flagged when this share of its text is only there after rendering (1 to 100 percent)." issues={at("rendering.hidden_content_percent")}>
              {(i) => (
                <span className="flex items-center gap-2">
                  <NumberInput id={i} value={r.hidden_content_percent} onChange={(v) => patch({ hidden_content_percent: num(v, r.hidden_content_percent) })} />
                  <span className="text-xs text-fg-3">%</span>
                </span>
              )}
            </Row>
          </Group>
        </>
      )}
    </>
  );
}
