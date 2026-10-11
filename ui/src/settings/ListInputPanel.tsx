import { useEffect, useState } from "react";
import { FileUp } from "lucide-react";
import type { ListSummary, Transport } from "../transport";
import { Button, Notice, Textarea } from "../components/ui";

export function ListInputPanel({
  transport,
  summary,
  onParsed,
}: {
  transport: Transport;
  summary: ListSummary | null;
  onParsed: (s: ListSummary) => void;
}) {
  const [text, setText] = useState("");
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let stop: (() => void) | undefined;
    transport.onListProgress((n) => setProgress(n)).then((s) => (stop = s));
    return () => stop?.();
  }, [transport]);

  const run = async (work: () => Promise<ListSummary>) => {
    setError(null);
    setProgress(0);
    try {
      onParsed(await work());
    } catch (e) {
      setError(String(e));
    } finally {
      setProgress(null);
    }
  };

  const upload = async () => {
    const path = await transport.pickListFile();
    if (path) await run(() => transport.parseListFile(path));
  };

  return (
    <div className="flex w-full flex-col gap-2">
      <Textarea aria-label="URL list" rows={6} placeholder="Paste URLs, one per line" value={text} onChange={(e) => setText(e.target.value)} />
      <div className="flex items-center gap-2">
        <Button size="sm" variant="primary" onClick={() => run(() => transport.parseListText(text))}>
          Load pasted URLs
        </Button>
        <Button size="sm" onClick={upload}>
          <FileUp className="size-3.5" /> Upload text or CSV file…
        </Button>
        {progress !== null && (
          <span role="status" className="text-sm text-fg-3 tabular">
            Reading… {progress.toLocaleString()} lines
          </span>
        )}
      </div>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      {summary && (
        <div className="flex flex-col gap-1.5 rounded-md bg-surface-2 px-3 py-2" data-testid="list-summary">
          <p className="m-0 text-sm text-fg-2 tabular">
            <strong className="text-fg">{summary.urls.toLocaleString()}</strong> URLs to crawl · {summary.duplicates.toLocaleString()} duplicates removed ·{" "}
            {summary.invalid_count.toLocaleString()} invalid
          </p>
          {summary.notices.map((n) => (
            <Notice key={n}>{n}</Notice>
          ))}
          {summary.invalid_count > 0 && (
            <details className="text-sm">
              <summary className="cursor-default text-fg-2">Show invalid lines</summary>
              <ul className="mt-1 mb-0 pl-4">
                {summary.invalid_sample.map((l) => (
                  <li key={l.line} className="text-fg-2">
                    Line {l.line}: <code className="font-code text-fg">{l.text}</code> ({l.reason})
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
    </div>
  );
}
