import { useEffect, useState } from "react";
import type { ListSummary, Transport } from "../transport";

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
    <div className="list-input">
      <textarea
        aria-label="URL list"
        rows={6}
        placeholder="Paste URLs, one per line"
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="row">
        <button type="button" onClick={() => run(() => transport.parseListText(text))}>
          Load pasted URLs
        </button>
        <button type="button" onClick={upload}>
          Upload text or CSV file…
        </button>
        {progress !== null && <span role="status">Reading… {progress.toLocaleString()} lines</span>}
      </div>
      {error && <p role="alert">{error}</p>}
      {summary && (
        <div className="list-summary" data-testid="list-summary">
          <p>
            <strong>{summary.urls.toLocaleString()}</strong> URLs to crawl ·{" "}
            {summary.duplicates.toLocaleString()} duplicates removed ·{" "}
            {summary.invalid_count.toLocaleString()} invalid
          </p>
          {summary.notices.map((n) => (
            <p key={n} className="notice">
              {n}
            </p>
          ))}
          {summary.invalid_count > 0 && (
            <details>
              <summary>Show invalid lines</summary>
              <ul>
                {summary.invalid_sample.map((l) => (
                  <li key={l.line}>
                    Line {l.line}: <code>{l.text}</code> ({l.reason})
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
