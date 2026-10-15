import { useCallback, useEffect, useRef, useState } from "react";
import type { SpellingSettings } from "../generated/SpellingSettings";
import type { SpellSummary } from "../generated/SpellSummary";
import type { SpellProgress, Transport } from "../transport";

export interface Spelling {
  /** The running check's progress, or null. */
  progress: SpellProgress | null;
  /** The last check of the open file, or null when it was never checked. */
  summary: SpellSummary | null;
  error: string | null;
  start(): void;
  cancel(): void;
}

/**
 * Spelling checks of the open crawl file (spec 027 part D). They run after
 * the crawl, so they never slow it: on request, or when the crawl ends with
 * "Check spelling after the crawl" on.
 */
export function useSpelling(
  transport: Transport,
  path: string | null,
  settings: SpellingSettings | null,
  crawlDone: boolean,
  onChecked: () => void,
): Spelling {
  const [progress, setProgress] = useState<SpellProgress | null>(null);
  const [summary, setSummary] = useState<SpellSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const checked = useRef(onChecked);
  checked.current = onChecked;

  useEffect(() => {
    setSummary(null);
    setError(null);
    if (!path) return;
    let off = false;
    transport.spellSummary(path).then(
      (s) => !off && setSummary(s),
      () => {},
    );
    return () => {
      off = true;
    };
  }, [transport, path]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let off = false;
    void transport.onSpellProgress((p) => {
      if (p.state === "running") return setProgress(p);
      setProgress(null);
      if (p.state === "failed") setError(p.error ?? "The spelling check failed.");
      if (p.summary) setSummary(p.summary);
      checked.current();
    }).then((u) => (off ? u() : (unlisten = u)));
    return () => {
      off = true;
      unlisten?.();
    };
  }, [transport]);

  const start = useCallback(() => {
    if (!path || !settings) return;
    setError(null);
    setProgress({ checked: 0, total: 0, state: "running", summary: null, error: null });
    transport.spellStart(path, settings).catch((e) => {
      setProgress(null);
      setError(String(e));
    });
  }, [transport, path, settings]);

  const cancel = useCallback(() => void transport.spellCancel(), [transport]);

  // Check once when a crawl ends, if asked to.
  const auto = useRef<string | null>(null);
  useEffect(() => {
    if (crawlDone && path && settings?.check && auto.current !== path) {
      auto.current = path;
      start();
    }
  }, [crawlDone, path, settings?.check, start]);

  return { progress, summary, error, start, cancel };
}
