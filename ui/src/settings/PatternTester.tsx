import { useState } from "react";
import type { PatternTest } from "../generated/PatternTest";
import type { Transport } from "../transport";

export function PatternTester({
  transport,
  include,
  exclude,
}: {
  transport: Transport;
  include: string[];
  exclude: string[];
}) {
  const [samples, setSamples] = useState("");
  const [rows, setRows] = useState<PatternTest[]>([]);

  const run = async () => setRows(await transport.testPatterns(include, exclude, samples.split("\n")));

  return (
    <div className="tester">
      <textarea
        aria-label="Sample URLs"
        rows={3}
        placeholder="Paste sample URLs, one per line"
        value={samples}
        onChange={(e) => setSamples(e.target.value)}
      />
      <button type="button" onClick={run}>
        Test patterns
      </button>
      {rows.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>URL</th>
              <th>Result</th>
              <th>Decided by</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td>{r.url ?? r.input}</td>
                <td>{r.error ?? describe(r)}</td>
                <td>
                  <code>{r.pattern ?? ""}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function describe(r: PatternTest): string {
  switch (r.decision?.decision) {
    case "crawl":
      return "Crawled";
    case "excluded":
      return "Excluded";
    case "not_included":
      return "Not included (matches no include pattern)";
    default:
      return "";
  }
}
