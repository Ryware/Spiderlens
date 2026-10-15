// The Content section (spec 020): the analysis state, then either the
// Content tab's pages or the clusters.

import { useState, type ReactNode } from "react";
import type { ContentSettings } from "../generated/ContentSettings";
import type { Transport } from "../transport";
import { Segmented } from "../components/ui";
import { AnalysisBar } from "./AnalysisBar";
import { ClustersPanel } from "./ClustersPanel";
import type { OpenCompare } from "./compare";
import type { NearDup } from "./useNearDup";

export type ContentMode = "pages" | "clusters";

export function ContentView({
  transport,
  path,
  live,
  nd,
  settings,
  onCompare,
  pages,
}: {
  transport: Transport;
  path: string;
  live: boolean;
  nd: NearDup;
  settings: ContentSettings | null;
  onCompare: OpenCompare;
  /** The Content tab's results table. */
  pages: ReactNode;
}) {
  const [mode, setMode] = useState<ContentMode>("pages");
  const analysed = nd.status?.state === "done" || nd.status?.state === "stale";
  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex items-center border-b border-line bg-bg">
        <div className="min-w-0 flex-1">
          <AnalysisBar nd={nd} live={live} settings={settings} />
        </div>
        <div className="shrink-0 px-3">
          <Segmented<ContentMode>
            size="sm"
            label="Content view"
            value={analysed ? mode : "pages"}
            onChange={setMode}
            options={[
              { value: "pages", label: "Pages" },
              { value: "clusters", label: "Clusters", disabled: !analysed },
            ]}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1">{analysed && mode === "clusters" ? <ClustersPanel transport={transport} path={path} generation={nd.generation} onCompare={onCompare} /> : pages}</div>
    </div>
  );
}
