import { useEffect, useState } from "react";
import type { BuildInfo, Transport } from "./transport";
import { SettingsScreen } from "./settings/SettingsScreen";

export function App({ transport }: { transport: Transport }) {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    transport.buildInfo().then(setInfo, (e) => setError(String(e)));
  }, [transport]);

  return (
    <main>
      <header className="identity">
      <h1>SpiderLens</h1>
      {error && <p role="alert">Could not read build information: {error}</p>}
      {info && (
        <dl>
          <dt>Version</dt>
          <dd data-testid="product-version">{info.product_version}</dd>
          <dt>Engine</dt>
          <dd data-testid="engine-version">{info.engine_version}</dd>
          <dt>Build</dt>
          <dd data-testid="build-id">{info.build_id}</dd>
        </dl>
      )}
      </header>
      <SettingsScreen transport={transport} />
    </main>
  );
}
