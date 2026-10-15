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
import { HeadersEditor } from "./HeadersEditor";
import { IssueSettingsPanel } from "../issues/IssueSettingsPanel";
import { LinkAnalysisBody } from "./LinkAnalysisSettings";
import { ListInputPanel } from "./ListInputPanel";
import { PatternTester } from "./PatternTester";
import { ScopeTester } from "./ScopeTester";
import { StorageSettings } from "./StorageSettings";

export type CategoryId = "list" | "limits" | "scope" | "identity" | "speed" | "robots" | "network" | "access" | "extraction" | "links" | "issues" | "storage" | "appearance";

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
  { id: "access", title: "Sign-in and cookies", description: "Cookies sent to the site, and what happens to the cookies it sets. Secret values are never saved.", prefixes: ["access"] },
  { id: "extraction", title: "Page data", description: "What SpiderLens keeps from each page it crawls.", prefixes: ["extraction"] },
  { id: "links", title: "Link analysis", description: "Link Score, and where on the page each link sits.", prefixes: ["links"] },
  { id: "issues", title: "Issues", description: "Which checks run on the results, how urgent each is, and their limits. Changes apply to the open crawl right away.", prefixes: ["issues"] },
  {
    id: "storage",
    title: "Storage",
    description: "Disk space and cleanup of unsaved crawls. Saved on this computer, not in the crawl profile.",
    prefixes: [],
    app: true,
  },
  {
    id: "appearance",
    title: "Appearance",
    description: "Brightness and accent colour. Saved on this computer, not in the crawl profile.",
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
}: {
  id: CategoryId;
  setup: CrawlSetup;
  transport: Transport;
  live?: boolean;
  /** The open crawl file, for testers that read it. */
  crawlPath?: string | null;
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
            {() => <ListInputPanel transport={transport} summary={setup.list} onParsed={setup.setList} />}
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
            <Row label="Use sitemaps from robots.txt" help="Also crawl the URLs listed in the sitemaps the site declares.">
              {(_, l) => <Switch labelledBy={l} checked={config.scope.use_sitemaps} onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, use_sitemaps: v } }))} />}
            </Row>
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

    case "access":
      return (
        <>
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
                  defaultDomain={hostOf(setup.startUrl)}
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
        </>
      );

    case "extraction":
      return (
        <Group>
          <Row
            label="Store page HTML"
            help="Keeps each page's HTML in the crawl file, compressed to about a sixth of its size, so it can be viewed and searched later. Turn off to keep crawl files small."
          >
            {(_, l) => <Switch labelledBy={l} checked={config.extraction.store_html} onChange={(v) => set((c) => ({ ...c, extraction: { ...c.extraction, store_html: v } }))} />}
          </Row>
        </Group>
      );

    case "links":
      return <LinkAnalysisBody links={config.links} issues={issues} transport={transport} path={crawlPath} onChange={(links) => set((c) => ({ ...c, links }))} />;
    case "issues":
      return <IssueSettingsPanel transport={transport} settings={config.issues} onChange={(v) => set((c) => ({ ...c, issues: v }))} />;
  }
}
