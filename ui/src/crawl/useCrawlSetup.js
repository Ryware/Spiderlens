import { useCallback, useEffect, useRef, useState } from "react";
/** How long to wait after typing before asking the engine to validate. */
export const VALIDATE_DEBOUNCE_MS = 200;
/**
 * Everything needed to configure and start a crawl, shared by the toolbar and
 * the settings sheet (plan decision 1). All validation happens in Rust; this
 * only keeps state and shows what comes back.
 */
export function useCrawlSetup(transport) {
    const [config, setConfig] = useState(null);
    const [presets, setPresets] = useState(null);
    const [startUrl, setStartUrlState] = useState("");
    const [list, setList] = useState(null);
    const [validation, setValidation] = useState(null);
    const [loaded, setLoaded] = useState(null);
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
        if (!config)
            return;
        const t = setTimeout(() => {
            const n = ++seq.current;
            transport.validate(config, config.mode === "spider" ? startUrl : null).then((v) => {
                if (n === seq.current)
                    setValidation(v);
            });
        }, VALIDATE_DEBOUNCE_MS);
        return () => clearTimeout(t);
    }, [transport, config, startUrl, list]);
    const issues = validation?.issues ?? [];
    const errors = issues.filter((i) => i.severity === "error");
    const warnings = issues.filter((i) => i.severity === "warning");
    const set = useCallback((patch) => {
        setConfig((c) => (c ? patch(c) : c));
    }, []);
    const setStartUrl = useCallback((url) => {
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
    const crawlInput = async () => {
        const { config, startUrl } = latest.current;
        if (!config)
            return null;
        const spider = config.mode === "spider";
        const n = ++seq.current;
        const v = await transport.validate(config, spider ? startUrl : null);
        if (n !== seq.current)
            return null;
        setValidation(v);
        if (v.issues.some((i) => i.severity === "error"))
            return null;
        if (!spider)
            return { config, startUrl: null, target: `${list?.urls.toLocaleString() ?? 0} URLs` };
        const url = v.normalized_start_url ?? startUrl;
        if (url !== startUrl)
            setStartUrlState(url);
        let target = url;
        try {
            target = new URL(url).host;
        }
        catch {
            // Not a URL the browser parses; the engine has validated it anyway.
        }
        return { config, startUrl: url, target };
    };
    const applyProfile = (l, url) => {
        setLoaded(l);
        setConfig(l.config);
        if (url)
            setStartUrlState(url);
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
