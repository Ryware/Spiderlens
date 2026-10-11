// In-memory Transport for UI tests. Validation here is a small stand-in; the
// real rules are tested in Rust (spiderlens-config).

import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Issue } from "../generated/Issue";
import type { SpeedSettings } from "../generated/SpeedSettings";
import type { LoadedProfile, Presets, Transport } from "../transport";

const polite: SpeedSettings = { max_concurrent: 8, max_per_host: 4, max_urls_per_sec: 5, adaptive: true };
const fast: SpeedSettings = { max_concurrent: 64, max_per_host: 32, max_urls_per_sec: null, adaptive: true };

export function defaultConfig(): CrawlConfig {
  return {
    mode: "spider",
    limits: { max_urls: null, max_depth: null },
    scope: {
      include: [],
      exclude: [],
      subdomains: "this_host",
      check_external: true,
      follow_nofollow: false,
      query_params: "keep",
    },
    identity: { user_agent: { preset: "spiderlens", custom_string: null, robots_token: null }, headers: [] },
    speed: { ...polite, preset: "polite" },
    robots: { mode: "respect", honour_crawl_delay: true },
    network: { connect_timeout_secs: 10, read_timeout_secs: 30, max_redirects: 10, max_body_mb: 50 },
    retry: { attempts: 3, max_wait_secs: 60, retry_5xx: true, retry_429: true },
  };
}

const same = (a: SpeedSettings, b: SpeedSettings) =>
  a.max_concurrent === b.max_concurrent &&
  a.max_per_host === b.max_per_host &&
  a.max_urls_per_sec === b.max_urls_per_sec &&
  a.adaptive === b.adaptive;

const presets: Presets = {
  user_agents: [
    { preset: "spiderlens", label: "SpiderLens", user_agent: "SpiderLens/0.1.0", robots_token: "SpiderLens" },
    { preset: "googlebot_smartphone", label: "Googlebot Smartphone", user_agent: "Googlebot", robots_token: "Googlebot" },
    { preset: "custom", label: "Custom", user_agent: null, robots_token: null },
  ],
  polite,
  fast,
};

export function fakeTransport(): Transport & { saved: Map<string, LoadedProfile> } {
  const saved = new Map<string, LoadedProfile>();
  let listCount = 0;
  return {
    saved,
    buildInfo: async () => ({
      product: "SpiderLens",
      product_version: "0.1.0",
      engine_version: "0.1.0",
      build_id: "abc1234.7",
      commit: "abc1234def",
    }),
    defaults: async () => defaultConfig(),
    presets: async () => presets,
    validate: async (config, startUrl) => {
      const issues: Issue[] = [];
      let normalized: string | null = null;
      if (config.mode === "spider" && startUrl !== null) {
        if (/^[a-z0-9.-]+(\/.*)?$/i.test(startUrl.replace(/^https?:\/\//, "")) && !startUrl.includes("!")) {
          normalized = startUrl.includes("://") ? startUrl : `https://${startUrl}/`;
        } else {
          issues.push({ path: "input.start_url", severity: "error", code: "start_url_invalid", message: "The start URL is not usable." });
        }
      }
      if (config.mode === "list" && listCount === 0) {
        issues.push({ path: "input.urls", severity: "error", code: "list_empty", message: "The list has no valid web addresses." });
      }
      if (config.identity.user_agent.preset !== "spiderlens") {
        issues.push({ path: "identity.user_agent.preset", severity: "warning", code: "ua_not_spiderlens", message: "The crawler will no longer identify itself as SpiderLens." });
      }
      if (config.robots.mode !== "respect") {
        issues.push({ path: "robots.mode", severity: "warning", code: "robots_ignored", message: "Pages disallowed by robots.txt will be crawled." });
      }
      const s = config.speed;
      if (s.max_concurrent > 8 || s.max_per_host > 4 || s.max_urls_per_sec === null || s.max_urls_per_sec > 5) {
        issues.push({ path: "speed", severity: "warning", code: "faster_than_polite", message: "Faster settings are intended for sites you control." });
      }
      return {
        issues,
        speed_preset: same(s, polite) ? "polite" : same(s, fast) ? "fast" : "custom",
        normalized_start_url: normalized,
      };
    },
    testPatterns: async (_i, _e, samples) =>
      samples.filter(Boolean).map((input) => ({ input, url: input, decision: { decision: "crawl", include: null }, pattern: null, error: null })),
    parseListText: async (text) => {
      const lines = text.split("\n").filter((l) => l.trim());
      const unique = new Set(lines.filter((l) => !l.includes("!")));
      listCount = unique.size;
      return {
        urls: unique.size,
        duplicates: lines.filter((l) => !l.includes("!")).length - unique.size,
        invalid_count: lines.filter((l) => l.includes("!")).length,
        invalid_sample: lines
          .map((text, i) => ({ line: i + 1, text, reason: "not a valid web address" }))
          .filter((l) => l.text.includes("!")),
        notices: [],
        first_urls: [...unique].slice(0, 20),
      };
    },
    parseListFile: async () => {
      throw new Error("not in tests");
    },
    onListProgress: async () => () => {},
    pickListFile: async () => null,
    pickProfileFile: async () => null,
    pickExportPath: async () => null,
    profiles: async () => [...saved.keys()].map((name) => ({ name, is_default: false, error: null })),
    loadProfile: async (name) => structuredClone(saved.get(name)!),
    saveProfile: async (name, config, startUrl, overwrite) => {
      if (saved.has(name) && !overwrite) return { result: "name_taken" };
      saved.set(name, { name, config: structuredClone(config), start_url: startUrl, ignored: [] });
      return { result: "saved" };
    },
    renameProfile: async () => {},
    duplicateProfile: async () => {},
    deleteProfile: async (name) => {
      saved.delete(name);
    },
    setDefaultProfile: async () => {},
    exportProfile: async () => {},
    importProfile: async () => {
      throw new Error("not in tests");
    },
  };
}
