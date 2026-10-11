import { useState } from "react";
import type { PatternTest } from "../generated/PatternTest";
import type { Transport } from "../transport";
import { Badge, Button, Textarea } from "../components/ui";

export function PatternTester({ transport, include, exclude }: { transport: Transport; include: string[]; exclude: string[] }) {
  const [samples, setSamples] = useState("");
  const [rows, setRows] = useState<PatternTest[]>([]);

  const run = async () => setRows(await transport.testPatterns(include, exclude, samples.split("\n")));

  return (
    <div className="flex w-full flex-col gap-2">
      <Textarea aria-label="Sample URLs" rows={3} placeholder="Paste sample URLs, one per line" value={samples} onChange={(e) => setSamples(e.target.value)} />
      <div>
        <Button size="sm" onClick={run}>
          Test patterns
        </Button>
      </div>
      {rows.length > 0 && (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left text-xs text-fg-3">
              <th className="py-1 pr-2 font-medium">URL</th>
              <th className="py-1 pr-2 font-medium">Result</th>
              <th className="py-1 font-medium">Decided by</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} className="border-t border-line">
                <td className="max-w-60 truncate py-1 pr-2 font-code" data-selectable>
                  {r.url ?? r.input}
                </td>
                <td className="py-1 pr-2">{r.error ? <Badge tone="bad">{r.error}</Badge> : describe(r)}</td>
                <td className="py-1 font-code text-fg-2">{r.pattern ?? ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function describe(r: PatternTest) {
  switch (r.decision?.decision) {
    case "crawl":
      return <Badge tone="ok">Crawled</Badge>;
    case "excluded":
      return <Badge tone="warn">Excluded</Badge>;
    case "not_included":
      return <Badge tone="neutral">Not included (matches no include pattern)</Badge>;
    default:
      return null;
  }
}
