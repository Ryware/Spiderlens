import { useCallback, useEffect, useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import type { Issue } from "../generated/Issue";
import type { ListSummary, Presets, Transport, Validation } from "../transport";

/** How long to wait after typing before asking the engine to validate. */
export const VALIDATE_DEBOUNCE_MS = 200;

export interface Loaded {
  name: string;
  config: CrawlConfig;
}

/**
 * Everything needed to configure and start a crawl, shared by the toolbar and
 * the settings sheet (plan decision 1). All validation happens in Rust; this
 * only keeps state and shows what comes back.
 */
export function useCrawlSetup(transport: Transport) {
  const [config, setConfig] = useState<CrawlConfig | null>(null);
  const [presets, setPresets] = useState<Presets | null>(null);
  const [startUrl, setStartUrlState] = useState("");
  const [list, setList] = useState<ListSummary | null>(null);
  const [validation, setValidation] = useState<Validation | null>(null);
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    transport.defaults(false).then(setConfig);
    transport.presets().then(setPresets);
  }, [transport]);

  useEffect(() => {
    if (!config) return;
    const t = setTimeout(() => {
      transport.validate(config, config.mode === "spider" ? startUrl : null).then(setValidation);
    }, VALIDATE_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [transport, config, startUrl, list]);

  const issues: Issue[] = validation?.issues ?? [];
  const errors = issues.filter((i) => i.severity === "error");
  const warnings = issues.filter((i) => i.severity === "warning");

  const set = useCallback((patch: (c: CrawlConfig) => CrawlConfig) => {
    setConfig((c) => (c ? patch(c) : c));
  }, []);

  const setStartUrl = useCallback((url: string) => {
    setStartUrlState(url);
  }, []);

  const canStart = !!config && !!validation && errors.length === 0;

  /**
   * What Start sends to the engine, and a short name for it. The start URL
   * is shown as it will be crawled (example.com becomes https://example.com/).
   */
  const crawlInput = (): { startUrl: string | null; target: string } => {
    if (config?.mode === "list") return { startUrl: null, target: `${list?.urls.toLocaleString() ?? 0} URLs` };
    const url = validation?.normalized_start_url ?? startUrl;
    if (url !== startUrl) setStartUrlState(url);
    let target = url;
    try {
      target = new URL(url).host;
    } catch {
      // Not a URL the browser parses; the engine has validated it anyway.
    }
    return { startUrl: url, target };
  };

  const applyProfile = (l: Loaded, url: string | null) => {
    setLoaded(l);
    setConfig(l.config);
    if (url) setStartUrlState(url);
  };

  const resetToDefaults = async () => {
    setLoaded(null);
    setConfig(await transport.defaults(true));
  };

  return {
    config,
    presets,
    startUrl,
    setStartUrl,
    list,
    setList,
    validation,
    issues,
    errors,
    warnings,
    speedPreset: validation?.speed_preset ?? "polite",
    set,
    canStart,
    crawlInput,
    loaded,
    setLoaded,
    applyProfile,
    resetToDefaults,
  };
}

export type CrawlSetup = ReturnType<typeof useCrawlSetup>;
