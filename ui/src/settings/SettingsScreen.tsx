import { useEffect, useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { CustomHeader } from "../generated/CustomHeader";
import type { RobotsMode } from "../generated/RobotsMode";
import type { SubdomainMode } from "../generated/SubdomainMode";
import type { UaPreset } from "../generated/UaPreset";
import type { ListSummary, Presets, Transport, Validation } from "../transport";
import { Field, IssueList, Lines, NumberField, OptionalNumber, Section, Toggle, issuesAt } from "./fields";
import { ListInputPanel } from "./ListInputPanel";
import { PatternTester } from "./PatternTester";
import { type Loaded, ProfileBar } from "./ProfileBar";

/** How long to wait after typing before asking the engine to validate. */
export const VALIDATE_DEBOUNCE_MS = 200;

export function SettingsScreen({ transport }: { transport: Transport }) {
  const [config, setConfig] = useState<CrawlConfig | null>(null);
  const [presets, setPresets] = useState<Presets | null>(null);
  const [startUrl, setStartUrl] = useState("");
  const [list, setList] = useState<ListSummary | null>(null);
  const [validation, setValidation] = useState<Validation | null>(null);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [ready, setReady] = useState<string | null>(null);

  useEffect(() => {
    transport.defaults(false).then(setConfig);
    transport.presets().then(setPresets);
  }, [transport]);

  // All validation happens in Rust; the UI only displays what comes back.
  useEffect(() => {
    if (!config) return;
    const t = setTimeout(() => {
      transport.validate(config, config.mode === "spider" ? startUrl : null).then(setValidation);
    }, VALIDATE_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [transport, config, startUrl, list]);

  if (!config || !presets) return <p>Loading settings…</p>;

  const issues = validation?.issues ?? [];
  const errors = issues.filter((i) => i.severity === "error");
  const warnings = issues.filter((i) => i.severity === "warning");
  const set = (patch: (c: CrawlConfig) => CrawlConfig) => {
    setReady(null);
    setConfig(patch(config));
  };

  const start = () => {
    if (errors.length) return;
    if (config.mode === "spider" && validation?.normalized_start_url) {
      setStartUrl(validation.normalized_start_url);
    }
    // Crawling itself arrives with spec 003; until then Start confirms the
    // configuration that would be used.
    setReady(
      config.mode === "spider"
        ? `Ready to crawl ${validation?.normalized_start_url ?? startUrl}.`
        : `Ready to crawl ${list?.urls.toLocaleString() ?? 0} URLs.`,
    );
  };

  const ua = config.identity.user_agent;
  const speedPreset = validation?.speed_preset ?? "polite";

  return (
    <div className="settings">
      <div className="start-bar">
        <div role="radiogroup" aria-label="Crawl mode" className="modes">
          {(["spider", "list"] as const).map((m) => (
            <label key={m}>
              <input
                type="radio"
                name="mode"
                checked={config.mode === m}
                onChange={() => set((c) => ({ ...c, mode: m }))}
              />{" "}
              {m === "spider" ? "Spider" : "List"}
            </label>
          ))}
        </div>
        {config.mode === "spider" ? (
          <input
            className="start-url"
            aria-label="Start URL"
            placeholder="example.com"
            value={startUrl}
            onChange={(e) => {
              setReady(null);
              setStartUrl(e.target.value);
            }}
            onKeyDown={(e) => e.key === "Enter" && start()}
          />
        ) : null}
        <button type="button" className="primary" disabled={errors.length > 0 || !validation} onClick={start}>
          Start
        </button>
      </div>
      {config.mode === "spider" && startUrl !== "" && <IssueList issues={issuesAt(issues, "input.start_url")} />}
      {config.mode === "list" && (
        <>
          <ListInputPanel transport={transport} summary={list} onParsed={setList} />
          {list && <IssueList issues={issuesAt(issues, "input.urls")} />}
        </>
      )}
      {ready && (
        <p className="ready" role="status">
          {ready}
        </p>
      )}
      {warnings.length > 0 && (
        <div className="warning-summary" aria-label="Less respectful settings">
          <IssueList issues={warnings} />
        </div>
      )}

      <ProfileBar
        transport={transport}
        config={config}
        startUrl={startUrl || null}
        loaded={loaded}
        onLoad={(l, url) => {
          setLoaded(l);
          setConfig(l.config);
          if (url) setStartUrl(url);
        }}
        onSaved={setLoaded}
        onReset={async () => {
          setLoaded(null);
          setConfig(await transport.defaults(true));
        }}
      />

      <Section title="Limits">
        <OptionalNumber
          label="Max URLs"
          hint="Blank means no limit."
          value={config.limits.max_urls}
          onChange={(v) => set((c) => ({ ...c, limits: { ...c.limits, max_urls: v } }))}
        />
        <OptionalNumber
          label="Max depth"
          hint={config.mode === "list" ? "Not used in List mode." : "Clicks from the start URL. Blank means no limit."}
          disabled={config.mode === "list"}
          value={config.limits.max_depth}
          onChange={(v) => set((c) => ({ ...c, limits: { ...c.limits, max_depth: v } }))}
        />
        <IssueList issues={issuesAt(issues, "limits")} />
      </Section>

      <Section title="Scope">
        <Lines
          label="Include patterns"
          placeholder="/blog/"
          value={config.scope.include}
          onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, include: v } }))}
        />
        <IssueList issues={issuesAt(issues, "scope.include")} />
        <Lines
          label="Exclude patterns"
          placeholder="\?sort="
          value={config.scope.exclude}
          onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, exclude: v } }))}
        />
        <IssueList issues={issuesAt(issues, "scope.exclude")} />
        <PatternTester transport={transport} include={config.scope.include} exclude={config.scope.exclude} />
        <Field label="Subdomains">
          <select
            aria-label="Subdomains"
            value={config.scope.subdomains}
            onChange={(e) =>
              set((c) => ({ ...c, scope: { ...c.scope, subdomains: e.target.value as SubdomainMode } }))
            }
          >
            <option value="this_host">This subdomain only</option>
            <option value="all_subdomains">All subdomains</option>
          </select>
        </Field>
        <Toggle
          label="Check external links (request once, never follow)"
          checked={config.scope.check_external}
          onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, check_external: v } }))}
        />
        <Toggle
          label="Follow nofollow links"
          checked={config.scope.follow_nofollow}
          onChange={(v) => set((c) => ({ ...c, scope: { ...c.scope, follow_nofollow: v } }))}
        />
      </Section>

      <Section title="Identity">
        <Field label="User agent">
          <select
            aria-label="User agent"
            value={ua.preset}
            onChange={(e) =>
              set((c) => ({
                ...c,
                identity: { ...c.identity, user_agent: { ...ua, preset: e.target.value as UaPreset } },
              }))
            }
          >
            {presets.user_agents.map((p) => (
              <option key={p.preset} value={p.preset}>
                {p.label}
              </option>
            ))}
          </select>
        </Field>
        {ua.preset === "custom" && (
          <>
            <Field label="Custom user agent">
              <input
                aria-label="Custom user agent"
                value={ua.custom_string ?? ""}
                onChange={(e) =>
                  set((c) => ({
                    ...c,
                    identity: { ...c.identity, user_agent: { ...ua, custom_string: e.target.value } },
                  }))
                }
              />
            </Field>
            <Field label="robots.txt token">
              <input
                aria-label="robots.txt token"
                value={ua.robots_token ?? ""}
                onChange={(e) =>
                  set((c) => ({
                    ...c,
                    identity: { ...c.identity, user_agent: { ...ua, robots_token: e.target.value } },
                  }))
                }
              />
            </Field>
          </>
        )}
        <IssueList issues={issuesAt(issues, "identity.user_agent")} />
        <HeadersEditor
          headers={config.identity.headers}
          onChange={(headers) => set((c) => ({ ...c, identity: { ...c.identity, headers } }))}
        />
        <IssueList issues={issuesAt(issues, "identity.headers")} />
      </Section>

      <Section title="Speed">
        <Field label="Preset">
          <select
            aria-label="Speed preset"
            value={speedPreset}
            onChange={(e) => {
              const v = e.target.value;
              if (v === "polite" || v === "fast") set((c) => ({ ...c, speed: presets[v] }));
            }}
          >
            <option value="polite">Polite</option>
            <option value="fast">Fast (sites you control)</option>
            <option value="custom" disabled>
              Custom
            </option>
          </select>
        </Field>
        <NumberField
          label="Max concurrent requests"
          value={config.speed.max_concurrent}
          onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_concurrent: v } }))}
        />
        <NumberField
          label="Max requests per site"
          value={config.speed.max_per_host}
          onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_per_host: v } }))}
        />
        <OptionalNumber
          label="Max URLs per second"
          hint="Blank means no cap."
          step={0.5}
          value={config.speed.max_urls_per_sec}
          onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, max_urls_per_sec: v } }))}
        />
        <Toggle
          label="Slow down automatically when a site struggles"
          checked={config.speed.adaptive}
          onChange={(v) => set((c) => ({ ...c, speed: { ...c.speed, adaptive: v } }))}
        />
        <IssueList issues={issuesAt(issues, "speed")} />
      </Section>

      <Section title="robots.txt">
        <Field label="robots.txt">
          <select
            aria-label="robots.txt behaviour"
            value={config.robots.mode}
            onChange={(e) => set((c) => ({ ...c, robots: { ...c.robots, mode: e.target.value as RobotsMode } }))}
          >
            <option value="respect">Respect</option>
            <option value="ignore_report">Ignore but report</option>
            <option value="ignore">Ignore</option>
          </select>
        </Field>
        <Toggle
          label="Honour crawl-delay"
          checked={config.robots.honour_crawl_delay}
          onChange={(v) => set((c) => ({ ...c, robots: { ...c.robots, honour_crawl_delay: v } }))}
        />
        <IssueList issues={issuesAt(issues, "robots")} />
      </Section>

      <Section title="Network and retries">
        <NumberField
          label="Connect timeout (seconds)"
          value={config.network.connect_timeout_secs}
          onChange={(v) => set((c) => ({ ...c, network: { ...c.network, connect_timeout_secs: v } }))}
        />
        <NumberField
          label="Read timeout (seconds)"
          value={config.network.read_timeout_secs}
          onChange={(v) => set((c) => ({ ...c, network: { ...c.network, read_timeout_secs: v } }))}
        />
        <NumberField
          label="Max redirect hops"
          value={config.network.max_redirects}
          onChange={(v) => set((c) => ({ ...c, network: { ...c.network, max_redirects: v } }))}
        />
        <NumberField
          label="Max page size (MB)"
          value={config.network.max_body_mb}
          onChange={(v) => set((c) => ({ ...c, network: { ...c.network, max_body_mb: v } }))}
        />
        <IssueList issues={issuesAt(issues, "network")} />
        <NumberField
          label="Attempts per URL"
          value={config.retry.attempts}
          onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, attempts: v } }))}
        />
        <NumberField
          label="Max wait between attempts (seconds)"
          value={config.retry.max_wait_secs}
          onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, max_wait_secs: v } }))}
        />
        <Toggle
          label="Retry server errors (5xx)"
          checked={config.retry.retry_5xx}
          onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, retry_5xx: v } }))}
        />
        <Toggle
          label="Retry rate limiting (429)"
          checked={config.retry.retry_429}
          onChange={(v) => set((c) => ({ ...c, retry: { ...c.retry, retry_429: v } }))}
        />
        <IssueList issues={issuesAt(issues, "retry")} />
      </Section>
    </div>
  );
}

function HeadersEditor({ headers, onChange }: { headers: CustomHeader[]; onChange: (h: CustomHeader[]) => void }) {
  const update = (i: number, h: CustomHeader) => onChange(headers.map((x, j) => (j === i ? h : x)));
  return (
    <div className="headers">
      <span className="label">Request headers</span>
      {headers.map((h, i) => {
        const secret = "secret" in h.value;
        const value = "plain" in h.value ? h.value.plain : (h.value.value ?? "");
        return (
          <div className="row" key={i}>
            <input
              aria-label={`Header ${i + 1} name`}
              placeholder="Name"
              value={h.name}
              onChange={(e) => update(i, { ...h, name: e.target.value })}
            />
            <input
              aria-label={`Header ${i + 1} value`}
              placeholder={secret && !value ? "Re-enter secret value" : "Value"}
              type={secret ? "password" : "text"}
              value={value}
              onChange={(e) =>
                update(i, {
                  ...h,
                  value: secret ? { secret: true, value: e.target.value } : { plain: e.target.value },
                })
              }
            />
            <Toggle
              label="Secret"
              checked={secret}
              onChange={(s) => update(i, { ...h, value: s ? { secret: true, value } : { plain: value } })}
            />
            <Toggle
              label="All hosts"
              checked={h.send_to === "all"}
              onChange={(all) => update(i, { ...h, send_to: all ? "all" : "internal" })}
            />
            <button type="button" onClick={() => onChange(headers.filter((_, j) => j !== i))}>
              Remove
            </button>
          </div>
        );
      })}
      <button
        type="button"
        onClick={() => onChange([...headers, { name: "", value: { plain: "" }, send_to: "internal" }])}
      >
        Add header
      </button>
    </div>
  );
}
