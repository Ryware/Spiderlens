// The Spelling tab's check control: start, progress and what the last check
// found (spec 027 part D).

import { SpellCheck } from "lucide-react";
import { Bar, Button, Notice } from "../components/ui";
import type { Spelling } from "./useSpelling";

export function SpellingBar({ spelling, live }: { spelling: Spelling; live: boolean }) {
  const { progress, summary, error } = spelling;
  if (progress) {
    return (
      <div className="flex items-center gap-3 text-sm" role="status">
        <span className="text-fg-2 tabular">
          Checking spelling: {progress.checked.toLocaleString()} of {progress.total ? progress.total.toLocaleString() : "…"} pages
        </span>
        <Bar value={progress.total ? progress.checked / progress.total : 0} tone="accent" label="Spelling progress" className="w-40" />
        <Button size="sm" variant="quiet" onClick={spelling.cancel}>
          Cancel
        </Button>
      </div>
    );
  }
  const missing = Object.entries(summary?.missing ?? {});
  return (
    <div className="flex flex-wrap items-center gap-3 text-sm">
      <Button size="sm" variant={summary ? "quiet" : "primary"} onClick={spelling.start} disabled={live}>
        <SpellCheck aria-hidden className="size-3.5" />
        {summary ? "Check again" : "Check spelling"}
      </Button>
      {live && <span className="text-fg-3">Spelling is checked when the crawl ends.</span>}
      {!live && summary && (
        <span className="text-fg-2" data-testid="spell-summary">
          {summary.cancelled ? "Stopped early: " : ""}
          {summary.errors.toLocaleString()} {summary.errors === 1 ? "error" : "errors"} ({summary.distinct_words.toLocaleString()} different{" "}
          {summary.distinct_words === 1 ? "word" : "words"}) in {(summary.pages - summary.no_dictionary).toLocaleString()} pages
          {summary.partly_checked > 0 && `. ${summary.partly_checked.toLocaleString()} without stored HTML were checked by title and headings only`}
          .
        </span>
      )}
      {!live && missing.length > 0 && (
        <Notice kind="warn">
          No dictionary for {missing.map(([lang, n]) => `${lang} (${n.toLocaleString()} ${n === 1 ? "page" : "pages"})`).join(", ")}. Add one in Settings › Spelling.
        </Notice>
      )}
      {error && <Notice kind="bad">{error}</Notice>}
    </div>
  );
}
