import type { ReactNode } from "react";
import type { QueryMode } from "../generated/QueryMode";
import type { RobotsMode } from "../generated/RobotsMode";
import type { SubdomainMode } from "../generated/SubdomainMode";
import type { UaPreset } from "../generated/UaPreset";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { Transport } from "../transport";
import { Input, NumberInput, Segmented, Select, Switch, Textarea } from "../components/ui";
import { Group, issuesAt, Row } from "./fields";
import { CookiesEditor, hostOf } from "./CookiesEditor";
import { LoginPanel } from "./LoginPanel";
import { ProxyPanel } from "./ProxyPanel";
import { originOf, SignInsEditor } from "./SignInsEditor";
import { HeadersEditor } from "./HeadersEditor";
import { IssueSettingsPanel } from "../issues/IssueSettingsPanel";
import { LinkAnalysisBody } from "./LinkAnalysisSettings";
import { ListInputPanel } from "./ListInputPanel";
import { PatternTester } from "./PatternTester";
import { ScopeTester } from "./ScopeTester";
import { CustomRulesPanel } from "./CustomRules";
import { DictionaryManager } from "../spelling/DictionaryManager";
import { SegmentsSettings } from "../segments/SegmentsSettings";
import { SitemapSources } from "./SitemapSources";
import { StorageSettings } from "./StorageSettings";
import { AccessibilitySettings } from "../accessibility/AccessibilitySettings";
import { RenderingSettings } from "./RenderingSettings";

export type CategoryId = "list" | "limits" | "scope" | "identity" | "speed" | "robots" | "network" | "access" | "extraction" | "content" | "segments" | "links" | "issues" | "custom" | "storage" | "spelling" | "rendering" | "accessibility" | "appearance" | "assistants";

export interface Category {
  id: CategoryId;
  title: string;
  description: string;
  /** Issue paths that belong to this category; its counts come from these (FR-011). */
  prefixes: string[];
  listOnly?: boolean;
  /** An app preference, not part of the crawl profile (spec 018 FR-012, spec 019 FR-025). */
  app?: boolean;
}

export const categories: Category[] = [
  { id: "list", title: "URL list", description: "Paste URLs or load a text or CSV file. Duplicates and invalid lines are reported.", prefixes: ["input.urls"], listOnly: true },
  { id: "limits", title: "Limits", description: "Where the crawl stops.", prefixes: ["limits"] },
  { id: "scope", title: "Scope", description: "Which URLs belong to this crawl.", prefixes: ["scope"] },
  { id: "identity", title: "Identity", description: "How SpiderLens introduces itself to the site.", prefixes: ["identity"] },
  { id: "speed", title: "Speed", description: "How hard SpiderLens works the site. Polite is right for sites you don't own.", prefixes: ["speed"] },
  { id: "robots", title: "robots.txt", description: "Whether to follow the site's crawling rules.", prefixes: ["robots"] },
  { id: "network", title: "Network and retries", description: "Timeouts, size limits and what happens when a request fails.", prefixes: ["network", "retry"] },
  { id: "access", title: "Sign-in and cookies", description: "Sign-ins, the login page, cookies sent to the site, what happens to the cookies it sets, and the proxy. Passwords and secret values are never saved.", prefixes: ["access"] },
  { id: "extraction", title: "Page data", description: "What SpiderLens keeps from each page it crawls.", prefixes: ["extraction"] },
  {
    id: "content",
    title: "Duplicate content",
    description: "How SpiderLens finds pages whose main content is nearly the same. Everything runs on this computer.",
    prefixes: ["content"],
  },
  { id: "segments", title: "Segments", description: "Group URLs into parts of the site. Every results tab can show and filter by them.", prefixes: ["segments"] },
  { id: "links", title: "Link analysis", description: "Link Score, and where on the page each link sits.", prefixes: ["links"] },
  { id: "issues", title: "Issues", description: "Which checks run on the results, how urgent each is, and their limits. Changes apply to the open crawl right away.", prefixes: ["issues"] },
  { id: "custom", title: "Custom rules", description: "Extract values and search every page. Rules run on each page's HTML as it arrives.", prefixes: ["custom"] },
  {
    id: "storage",
    title: "Storage",
    description: "Disk space and cleanup of unsaved crawls. Saved on this computer, not in the crawl profile.",
    prefixes: [],
    app: true,
  },
  {
    id: "spelling",
    title: "Spelling",
    description: "Spelling is checked after the crawl, from the stored pages, so it never slows crawling. Dictionaries stay on this computer.",
    prefixes: ["spelling"],
  },
  {
    id: "rendering",
    title: "JavaScript rendering",
    description: "Render each page in a browser, as search engines do, and compare it with the raw HTML. Slower, and needs Google Chrome, Chromium or Microsoft Edge.",
    prefixes: ["rendering"],
  },
  {
    id: "accessibility",
    title: "Accessibility",
    description: "Check pages against WCAG and for mobile friendliness. Static checks read the stored HTML; full checks run in the rendered page.",
    prefixes: ["accessibility"],
  },
  {
    id: "appearance",
    title: "Appearance",
    description: "Brightness and accent colour. Saved on this computer, not in the crawl profile.",
    prefixes: [],
    app: true,
  },
  {
    id: "assistants",
    title: "AI assistants",
    description: "Let assistants such as Claude read your crawls through SpiderLens's local MCP server. Saved on this computer.",
    prefixes: [],
    app: true,
  },
];

/** Categories a running crawl takes on at once, with "Apply to running crawl" (spec 004 D13). */
export const liveCategories: CategoryId[] = ["speed", "network"];

export const issuesFor = (setup: CrawlSetup, c: Category) => c.prefixes.flatMap((p) => issuesAt(setup.issues, p));

/** One pattern per line. */
function Lines({ id, value, onChange, placeholder }: { id: string; value: string[]; onChange: (v: string[]) => void; placeholder?: string }) {
  return <Textarea id={id} rows={3} placeholder={placeholder} value={value.join("\n")} onChange={(e) => onChange(e.target.value === "" ? [] : e.target.value.split("\n"))} />;
}

/** `live`: a crawl is running and only the settings it can take on at once are editable. */
export function CategoryBody({
  id,
  setup,
  transport,
  live,
  crawlPath = null,
  crawlRunning = false,
  onCategory,
}: {
  id: CategoryId;
  setup: CrawlSetup;
  transport: Transport;
  live?: boolean;
  /** The open crawl, which custom rules can run on again (spec 013 US3). */
  crawlPath?: string | null;
  crawlRunning?: boolean;
  /** Switches the open settings category (used by the Accessibility notice). */
  onCategory?: (c: CategoryId) => void;
}): ReactNode {
  if (id === "storage") return <StorageSettings transport={transport} />;
  const { config, presets, set, issues } = setup;
  if (!config || !presets) return null;
  const at = (p: string) => issuesAt(issues, p);

  switch (id) {
    case "list":
      return (
        <Group>
          <Row label="URLs to crawl" stacked issues={at("input.urls")}>
            {() => <ListInputPanel transport={transport} config={config} summary={setup.list} onParsed={setup.setList} />}
          </Row>
        </Group>
      );

    case "limits":
      return (
        <Group>
          <Row label="Max URLs" help="Blank means no limit." issues={at("limits.max_urls")}>
            {(i) => <NumberInput id={i} optional value={config.limits.max_urls} onChange={(v) => set((c) => ({ ...c, limits: { ...c.limits, max_urls: v } }))} placeholder="No limit" />}
          </Row>
          <Row
            label="Max depth"
            help={config.mode === "list" ? "Not used in List mode." : "Clicks from the start URL. Blank means no limit."}
            issues={at("limits.max_depth")}
          >
            {(i) => (
              <NumberInput
                id={i}
                optional
                disabled={config.mode === "list"}
                value={config.limits.max_depth}
                onChange={(v) => set((c) => ({ ...c, limits: { ...c.limits, max_depth: v } }))}
                placeholder="No limit"
              />
            )}
          </Row>
          <Row
            label="List depth"
            help={config.mode === "list" ? "Clicks to follow from the listed URLs. 0 crawls only the list." : "Used in List mode only."}
            issues={at("limits.list_depth")}
          >
            {(i) => (
              <NumberInput
                id={i}
                disabled={config.mode !== "list"}
                value={config.limits.list_depth}
                onChange={(v) => set((c) => ({ ...c, limits: { ...c.limits, list_depth: v ?? NaN } }))}
              />
            )}
          </Row>
        </Group>
      );

    case "scope":
      return (
        <>
          <Group title="Patterns">
            <Row label="Include patterns" stacked help="One regular expression per line. Blank includes everything." issues={at("scope.include")}>
              {(i) => <Lines id={i} placeholder="/blog/" value={config.scope.include} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, include: v } }))} />}
            </Row>
            <Row label="Exclude patterns" stacked help="One regular expression per line." issues={at("scope.exclude")}>
              {(i) => <Lines id={i} placeholder="\?sort=" value={config.scope.exclude} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, exclude: v } }))} />}
            </Row>
            <Row label="Try the patterns" stacked help="See which URLs the patterns keep, and which pattern decided.">
              {() => <PatternTester transport={transport} include={config.scope.include} exclude={config.scope.exclude} />}
            </Row>
          </Group>
          <Group title="Links">
            <Row label="Subdomains" issues={at("scope.subdomains")}>
              {(i) => (
                <Select id={i} className="w-48" value={config.scope.subdomains} onChange={(e) => set((c) => ({ ...c, scope: { ...c.scope, subdomains: e.target.value as SubdomainMode } }))}>
                  <option value="this_host">This subdomain only</option>
                  <option value="all_subdomains">All subdomains</option>
                </Select>
              )}
            </Row>
            <Row label="Check external links" help="Request each once to see if it works; never follow it.">
              {(_, l) => <Switch labelledBy={l} checked={config.scope.check_external} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, check_external: v } }))} />}
            </Row>
            <Row label="Follow internal nofollow links">
              {(_, l) => <Switch labelledBy={l} checked={config.scope.follow_internal_nofollow} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, follow_internal_nofollow: v } }))} />}
            </Row>
            <Row label="Follow external nofollow links">
              {(_, l) => <Switch labelledBy={l} checked={config.scope.follow_external_nofollow} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, follow_external_nofollow: v } }))} />}
            </Row>
            <Row label="Stay in the start folder" help="Internal URLs outside the start URL's folder are not crawled.">
              {(_, l) => <Switch labelledBy={l} checked={config.scope.stay_in_folder} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, stay_in_folder: v } }))} />}
            </Row>
          </Group>
          <Group title="Sitemaps">
            <SitemapSources
              value={config.scope.sitemaps}
              onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, sitemaps: v } }))}
              issues={issues}
              transport={transport}
            />
          </Group>
          <Group title="URLs">
            <Row label="Query parameters" help="Removing parameters makes URLs that differ only by them count once." issues={at("scope.query_params.mode")}>
              {(i) => (
                <Select
                  id={i}
                  className="w-48"
                  value={config.scope.query_params.mode}
                  onChange={(e) => set((c) => ({ ...c, scope: { ...c.scope, query_params: { ...c.scope.query_params, mode: e.target.value as QueryMode } } }))}
                >
                  <option value="keep">Keep all</option>
                  <option value="remove">Remove some</option>
                  <option value="remove_all">Remove all</option>
                </Select>
              )}
            </Row>
            {config.scope.query_params.mode === "remove" && (
              <Row label="Parameters to remove" stacked help="One name per line, for example utm_source. Names are case-sensitive." issues={at("scope.query_params.names")}>
                {(i) => (
                  <Lines
                    id={i}
                    placeholder="utm_source"
                    value={config.scope.query_params.names}
                    onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, query_params: { ...c.scope.query_params, names: v } } }))}
                  />
                )}
              </Row>
            )}
            <Row label="Sort parameters" help="?b=2&a=1 and ?a=1&b=2 count as one URL.">
              {(_, l) => (
                <Switch
                  labelledBy={l}
                  checked={config.scope.query_params.sort}
                  onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, query_params: { ...c.scope.query_params, sort: v } } }))}
                />
              )}
            </Row>
            <Row label="Max URL length" help="Longer URLs are reported as malformed and not crawled." issues={at("scope.max_url_length")}>
              {(i) => <NumberInput id={i} value={config.scope.max_url_length} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, max_url_length: v ?? NaN } }))} />}
            </Row>
          </Group>
          <Group title="Test URLs">
            <Row label="Try the whole scope" stacked help="Paste URLs to see whether the crawl would request them, and which setting decides.">
              {() => <ScopeTester transport={transport} config={config} startUrl={setup.validation?.normalized_start_url ?? null} />}
            </Row>
          </Group>
        </>
      );

    case "identity": {
      const ua = config.identity.user_agent;
      const setUa = (patch: Partial<typeof ua>) => set((c) => ({ ...c, identity: { ...c.identity, user_agent: { ...ua, ...patch } } }));
      const preset = presets.user_agents.find((p) => p.preset === ua.preset);
      return (
        <>
          <Group>
            <Row label="User agent" help={preset?.user_agent ? <code className="font-code" data-selectable>{preset.user_agent}</code> : undefined} issues={at("identity.user_agent")}>
              {(i) => (
                <Select id={i} className="w-56" value={ua.preset} onChange={(e) => setUa({ preset: e.target.value as UaPreset })}>
                  {presets.user_agents.map((p) => (
                    <option key={p.preset} value={p.preset}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              )}
            </Row>
            {ua.preset === "custom" && (
              <>
                <Row label="Custom user agent">
                  {(i) => <Input id={i} mono className="w-80" value={ua.custom_string ?? ""} onChange={(e) => setUa({ custom_string: e.target.value })} />}
                </Row>
                <Row label="robots.txt token" help="The name robots.txt rules are matched against.">
                  {(i) => <Input id={i} mono className="w-56" value={ua.robots_token ?? ""} onChange={(e) => setUa({ robots_token: e.target.value })} />}
                </Row>
              </>
            )}
          </Group>
          <Group title="Request headers">
            <Row label="Headers sent with every request" stacked help="Secret values are never saved in crawl files or profiles." issues={at("identity.headers")}>
              {() => <HeadersEditor headers={config.identity.headers} onChange={(headers) => set((c) => ({ ...c, identity: { ...c.identity, headers } }))} />}
            </Row>
          </Group>
        </>
      );
    }

    case "speed":
      return (
        <Group>
          <Row label="Preset" help={setup.speedPreset === "fast" ? "Fast is for sites you control." : undefined}>
            {() => (
              <Segmented
                label="Speed preset"
                value={setup.speedPreset}
                options={[
                  { value: "polite", label: "Polite" },
                  { value: "fast", label: "Fast" },
                  { value: "custom", label: "Custom", disabled: setup.speedPreset !== "custom" },
                ]}
                onChange={(v) => {
                  if (v === "polite" || v === "fast") set((c) => ({ ...c, speed: presets[v] }));
                }}
              />
            )}
          </Row>
          <Row label="Max concurrent requests" help="Across all sites in this crawl." issues={at("speed.max_concurrent")}>
            {(i) => <NumberInput id={i} value={config.speed.max_concurrent} onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_concurrent: v ?? NaN } }))} />}
          </Row>
          <Row label="Max requests per site" issues={at("speed.max_per_host")}>
            {(i) => <NumberInput id={i} value={config.speed.max_per_host} onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_per_host: v ?? NaN } }))} />}
          </Row>
          <Row label="Max URLs per second" help="Blank means no cap." issues={at("speed.max_urls_per_sec")}>
            {(i) => (
              <NumberInput id={i} optional step={0.5} placeholder="No cap" value={config.speed.max_urls_per_sec} onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_urls_per_sec: v } }))} />
            )}
          </Row>
          <Row label="Slow down automatically when a site struggles">
            {(_, l) => <Switch labelledBy={l} checked={config.speed.adaptive} onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, adaptive: v } }))} />}
          </Row>
        </Group>
      );

    case "robots":
      return (
        <Group>
          <Row label="robots.txt behaviour" issues={at("robots")}>
            {(i) => (
              <Select id={i} className="w-48" value={config.robots.mode} onChange={(e) => set((c) => ({ ...c, robots: { ...c.robots, mode: e.target.value as RobotsMode } }))}>
                <option value="respect">Respect</option>
                <option value="ignore_report">Ignore but report</option>
                <option value="ignore">Ignore</option>
              </Select>
            )}
          </Row>
          <Row label="Honour crawl-delay" help="Wait as long between requests as the site asks.">
            {(_, l) => <Switch labelledBy={l} checked={config.robots.honour_crawl_delay} onChange={(v) => set((c) => ({ ...c, robots: { ...c.robots, honour_crawl_delay: v } }))} />}
          </Row>
        </Group>
      );

    case "network":
      return (
        <>
          <Group title="Network" disabled={live}>
            <Row label="Connect timeout (seconds)" issues={at("network.connect_timeout_secs")}>
              {(i) => <NumberInput id={i} value={config.network.connect_timeout_secs} onChange={(v) => set((c) => ({ ...c, network: { ...c.network, connect_timeout_secs: v ?? NaN } }))} />}
            </Row>
            <Row label="Read timeout (seconds)" issues={at("network.read_timeout_secs")}>
              {(i) => <NumberInput id={i} value={config.network.read_timeout_secs} onChange={(v) => set((c) => ({ ...c, network: { ...c.network, read_timeout_secs: v ?? NaN } }))} />}
            </Row>
            <Row label="Max redirect hops" issues={at("network.max_redirects")}>
              {(i) => <NumberInput id={i} value={config.network.max_redirects} onChange={(v) => set((c) => ({ ...c, network: { ...c.network, max_redirects: v ?? NaN } }))} />}
            </Row>
            <Row label="Max page size (MB)" issues={at("network.max_body_mb")}>
              {(i) => <NumberInput id={i} value={config.network.max_body_mb} onChange={(v) => set((c) => ({ ...c, network: { ...c.network, max_body_mb: v ?? NaN } }))} />}
            </Row>
          </Group>
          <Group title="Retries">
            <Row label="Attempts per URL" help="Including the first. 1 means no retry." issues={at("retry.attempts")}>
              {(i) => <NumberInput id={i} value={config.retry.attempts} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, attempts: v ?? NaN } }))} />}
            </Row>
            <Row label="First wait (seconds)" help="Before the second attempt. Each wait after that is twice as long." issues={at("retry.base_wait_secs")}>
              {(i) => (
                <NumberInput
                  id={i}
                  min={0.1}
                  max={60}
                  step={0.5}
                  value={config.retry.base_wait_secs}
                  onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, base_wait_secs: v ?? NaN } }))}
                />
              )}
            </Row>
            <Row label="Max wait between attempts (seconds)" issues={at("retry.max_wait_secs")}>
              {(i) => <NumberInput id={i} value={config.retry.max_wait_secs} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, max_wait_secs: v ?? NaN } }))} />}
            </Row>
            <Row label="Retry server errors (5xx)">
              {(_, l) => <Switch labelledBy={l} checked={config.retry.retry_5xx} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, retry_5xx: v } }))} />}
            </Row>
            <Row label="Retry rate limiting (429)">
              {(_, l) => <Switch labelledBy={l} checked={config.retry.retry_429} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, retry_429: v } }))} />}
            </Row>
          </Group>
          <Group title="Sites that keep failing">
            <Row label="Pause a site after failures in a row" help="Only failures that may pass count, such as timeouts. Other sites keep being crawled." issues={at("retry.host_pause_after")}>
              {(i) => (
                <NumberInput id={i} min={1} max={100} value={config.retry.host_pause_after} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, host_pause_after: v ?? NaN } }))} />
              )}
            </Row>
            <Row label="First pause (seconds)" help="Then one test request is sent. If it fails, the pause doubles." issues={at("retry.host_pause_secs")}>
              {(i) => (
                <NumberInput id={i} min={1} max={3600} value={config.retry.host_pause_secs} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, host_pause_secs: v ?? NaN } }))} />
              )}
            </Row>
            <Row label="Longest pause (seconds)" issues={at("retry.host_pause_max_secs")}>
              {(i) => (
                <NumberInput
                  id={i}
                  min={1}
                  max={86400}
                  value={config.retry.host_pause_max_secs}
                  onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, host_pause_max_secs: v ?? NaN } }))}
                />
              )}
            </Row>
            <Row label="Give up after failed tests" help={'Its remaining URLs are then listed as "Host unavailable" and can be retried later.'} issues={at("retry.host_give_up_after")}>
              {(i) => (
                <NumberInput id={i} min={1} max={50} value={config.retry.host_give_up_after} onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, host_give_up_after: v ?? NaN } }))} />
              )}
            </Row>
          </Group>
        </>
      );

    case "access": {
      const host = hostOf(setup.startUrl);
      const ua = config.identity.user_agent;
      const userAgent = ua.preset === "custom" ? ua.custom_string : (presets.user_agents.find((p) => p.preset === ua.preset)?.user_agent ?? null);
      return (
        <>
          <Group title="Sign-in">
            <Row
              label="Sites that ask for a user name and password"
              stacked
              help="For password-protected staging sites (Basic, Digest or NTLM). Sent only to the site it is for."
              issues={at("access.sign_ins")}
            >
              {() => (
                <SignInsEditor
                  signIns={config.access.sign_ins}
                  defaultOrigin={originOf(setup.startUrl)}
                  onChange={(sign_ins) => set((c) => ({ ...c, access: { ...c.access, sign_ins } }))}
                />
              )}
            </Row>
            <LoginPanel
              login={config.access.login}
              startUrl={setup.startUrl}
              internalHosts={host ? [host] : []}
              userAgent={userAgent}
              transport={transport}
              issues={at}
              onChange={(login) => set((c) => ({ ...c, access: { ...c.access, login } }))}
            />
          </Group>
          <Group title="Cookies">
            <Row
              label="Cookies sent with every request"
              stacked
              help="Sent only to this crawl's own site. Paste the Cookie line from a signed-in browser to crawl as that user."
              issues={at("access.cookies")}
            >
              {() => (
                <CookiesEditor
                  cookies={config.access.cookies}
                  defaultDomain={host}
                  onChange={(cookies) => set((c) => ({ ...c, access: { ...c.access, cookies } }))}
                />
              )}
            </Row>
            <Row
              label="Cookies the site sets"
              help={
                config.access.cookie_policy === "keep"
                  ? "Sent back for the rest of the crawl, like a browser. They are recorded in the Cookies report either way."
                  : "Recorded in the Cookies report but not sent back, so every page is seen as a first visit."
              }
              issues={at("access.cookie_policy")}
            >
              {() => (
                <Segmented
                  label="Cookies the site sets"
                  value={config.access.cookie_policy}
                  options={[
                    { value: "ignore", label: "Ignore" },
                    { value: "keep", label: "Keep" },
                  ]}
                  onChange={(cookie_policy) => set((c) => ({ ...c, access: { ...c.access, cookie_policy } }))}
                />
              )}
            </Row>
          </Group>
          <Group title="Proxy">
            <ProxyPanel
              proxy={config.access.proxy}
              probeUrl={setup.startUrl}
              transport={transport}
              issues={at}
              onChange={(proxy) => set((c) => ({ ...c, access: { ...c.access, proxy } }))}
            />
          </Group>
        </>
      );
    }

    case "rendering":
      return <RenderingSettings config={config} set={set} at={at} transport={transport} />;
    case "accessibility":
      return <AccessibilitySettings config={config} set={set} at={at} onOpenRendering={onCategory ? () => onCategory("rendering") : undefined} />;
    case "extraction": {
      const pdf = config.extraction.pdf;
      const setPdf = (p: Partial<typeof pdf>) => set((c) => ({ ...c, extraction: { ...c.extraction, pdf: { ...c.extraction.pdf, ...p } } }));
      return (
        <>
          <Group>
            <Row
              label="Store page HTML"
              help="Keeps each page's HTML in the crawl file, compressed to about a sixth of its size, so it can be viewed and searched later. Turn off to keep crawl files small."
            >
              {(_, l) => <Switch labelledBy={l} checked={config.extraction.store_html} onChange={(v) => set((c) => ({ ...c, extraction: { ...c.extraction, store_html: v } }))} />}
            </Row>
            <Row
              label="Check AMP"
              help="Follows amphtml links and checks pages marked as AMP against a core subset of AMP's rules. The results are in the AMP tab."
            >
              {(_, l) => <Switch labelledBy={l} checked={config.extraction.check_amp} onChange={(v) => set((c) => ({ ...c, extraction: { ...c.extraction, check_amp: v } }))} />}
            </Row>
          </Group>
          <Group title="PDFs">
            <Row
              label="Crawl PDFs"
              help="Downloads PDFs and reads their properties, page count, words and links. Reading happens beside the crawl, so pages are not slowed down. With Store page HTML on, the PDF text is kept too."
            >
              {(_, l) => <Switch labelledBy={l} checked={pdf.crawl} onChange={(v) => setPdf({ crawl: v })} />}
            </Row>
            <Row label="Follow links in PDFs" help="Crawls the pages PDFs link to. When off, the links are still listed.">
              {(_, l) => <Switch labelledBy={l} disabled={!pdf.crawl} checked={pdf.follow_links} onChange={(v) => setPdf({ follow_links: v })} />}
            </Row>
            <Row label="PDF size limit (MB)" help="Larger PDFs are not downloaded past the limit and are listed as over it." issues={at("extraction.pdf.max_mb")}>
              {(i) => <NumberInput id={i} disabled={!pdf.crawl} value={pdf.max_mb} onChange={(v) => setPdf({ max_mb: v ?? NaN })} />}
            </Row>
            <Row label="Time to read one PDF (seconds)" help="A PDF that takes longer is listed as partly read, with what was read by then." issues={at("extraction.pdf.time_limit_secs")}>
              {(i) => <NumberInput id={i} disabled={!pdf.crawl} value={pdf.time_limit_secs} onChange={(v) => setPdf({ time_limit_secs: v ?? NaN })} />}
            </Row>
          </Group>
        </>
      );
    }
    case "content": {
      const ct = config.content;
      const put = (patch: Partial<typeof ct>) => set((c) => ({ ...c, content: { ...c.content, ...patch } }));
      return (
        <>
          <Group title="Matching">
            <Row label="Near duplicate threshold" help="Pages at least this similar, in percent, count as near duplicates. 90 is a good start; 100 finds only exact copies." issues={at("content.threshold")}>
              {(i) => <NumberInput id={i} min={50} max={100} value={ct.threshold} onChange={(v) => put({ threshold: v ?? NaN })} />}
            </Row>
            <Row label="Show closest matches from" help="Each page lists its closest matches at least this similar, even below the threshold." issues={at("content.report_floor")}>
              {(i) => <NumberInput id={i} min={50} max={100} value={ct.report_floor} onChange={(v) => put({ report_floor: v ?? NaN })} />}
            </Row>
            <Row label="Minimum words" help="Pages with fewer words of main content are not compared." issues={at("content.min_words")}>
              {(i) => <NumberInput id={i} min={0} value={ct.min_words} onChange={(v) => put({ min_words: v ?? NaN })} />}
            </Row>
            <Row label="Indexable pages only" help="Leaves out noindex pages and pages that name another page as canonical.">
              {(_, l) => <Switch labelledBy={l} checked={ct.indexable_only} onChange={(v) => put({ indexable_only: v })} />}
            </Row>
          </Group>
          <Group title="Main content">
            <Row
              label="Content area selector"
              stacked
              help="CSS selector of the element that holds the main content. Blank uses <main>, then <article>, then the page without navigation, header, footer and sidebars."
              issues={at("content.content_selector")}
            >
              {(i) => <Input id={i} mono placeholder="#content" value={ct.content_selector ?? ""} onChange={(e) => put({ content_selector: e.target.value === "" ? null : e.target.value })} />}
            </Row>
            <Row label="Leave out" stacked help="CSS selectors of parts to ignore, one per line, such as related products or cookie banners." issues={at("content.exclude_selectors")}>
              {(i) => <Lines id={i} placeholder=".related-products" value={ct.exclude_selectors} onChange={(v) => put({ exclude_selectors: v })} />}
            </Row>
          </Group>
        </>
      );
    }

    case "spelling": {
      const sp = config.spelling;
      const setSp = (p: Partial<typeof sp>) => set((c) => ({ ...c, spelling: { ...c.spelling, ...p } }));
      return (
        <>
          <Group>
            <Row label="Check spelling after the crawl" help="Or check any time with Check spelling on the Spelling tab.">
              {(_, l) => <Switch labelledBy={l} checked={sp.check} onChange={(v) => setSp({ check: v })} />}
            </Row>
            <Row label="Language of pages without a lang attribute" issues={at("spelling.default_lang")}>
              {(i) => <Input id={i} className="w-32" value={sp.default_lang} onChange={(e) => setSp({ default_lang: e.target.value })} />}
            </Row>
            <Row label="Skip words in capitals" help="Words like NASA or SKU are usually names, not mistakes.">
              {(_, l) => <Switch labelledBy={l} checked={sp.skip_all_caps} onChange={(v) => setSp({ skip_all_caps: v })} />}
            </Row>
            <Row label="Words to ignore" help="One per line: brand names, product names, jargon. Case is ignored." stacked>
              {(i) => <Lines id={i} value={sp.ignore} onChange={(ignore) => setSp({ ignore })} placeholder="SpiderLens" />}
            </Row>
          </Group>
          <Group title="Dictionaries">
            <Row
              label="Pages are checked with the dictionary for their language"
              help="Words in another script, such as Hebrew on an English page, use that script's dictionary when one is installed."
              stacked
            >
              {() => <DictionaryManager transport={transport} />}
            </Row>
          </Group>
        </>
      );
    }

    case "segments":
      return <SegmentsSettings segments={config.segments} issues={issuesAt(issues, "segments")} onChange={(segments) => set((c) => ({ ...c, segments }))} />;
    case "custom":
      return <CustomRulesPanel setup={setup} transport={transport} crawlPath={crawlPath} crawlRunning={crawlRunning} />;
    case "links":
      return <LinkAnalysisBody links={config.links} issues={issues} transport={transport} path={crawlPath} onChange={(links) => set((c) => ({ ...c, links }))} />;
    case "issues":
      return <IssueSettingsPanel transport={transport} settings={config.issues} onChange={(v) => set((c) => ({ ...c, issues: v }))} />;
  }
}
