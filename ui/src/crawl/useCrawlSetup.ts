import { useCallback, useEffect, useRef, useState } from "react";
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
  // Only the newest validate reply may land: a slow reply for "r" must not
  // overwrite the one for "ryware.dev".
  const seq = useRef(0);
  // Start reads the field as it is now, not as of the last render.
  const latest = useRef({ config, startUrl });
  latest.current = { config, startUrl };

  useEffect(() => {
    transport.defaults(false).then(setConfig);
    transport.presets().then(setPresets);
  }, [transport]);

  useEffect(() => {
    if (!config) return;
    const t = setTimeout(() => {
      const n = ++seq.current;
      transport.validate(config, config.mode === "spider" ? startUrl : null).then((v) => {
        if (n === seq.current) setValidation(v);
      });
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
   * What Start sends to the engine, and a short name for it, or null when the
   * input has errors (they show at the field). Validates what is in the field
   * right now: the debounced result may be for an older value, such as "r"
   * when Enter follows "ryware.dev" quickly. The start URL is shown as it will
   * be crawled (example.com becomes https://example.com/).
   */
  const crawlInput = async (): Promise<{ config: CrawlConfig; startUrl: string | null; target: string } | null> => {
    const { config, startUrl } = latest.current;
    if (!config) return null;
    const spider = config.mode === "spider";
    const n = ++seq.current;
    const v = await transport.validate(config, spider ? startUrl : null);
    if (n !== seq.current) return null;
    setValidation(v);
    if (v.issues.some((i) => i.severity === "error")) return null;
    if (!spider) return { config, startUrl: null, target: `${list?.urls.toLocaleString() ?? 0} URLs` };
    const url = v.normalized_start_url ?? startUrl;
    if (url !== startUrl) setStartUrlState(url);
    let target = url;
    try {
      target = new URL(url).host;
    } catch {
      // Not a URL the browser parses; the engine has validated it anyway.
    }
    return { config, startUrl: url, target };
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
