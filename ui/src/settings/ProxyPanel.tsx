import { useState } from "react";
import type { ProxyKind } from "../generated/ProxyKind";
import type { ProxyMode } from "../generated/ProxyMode";
import type { ProxySettings } from "../generated/ProxySettings";
import type { Issue } from "../generated/Issue";
import type { ProxyInEffect, ProxyTestResult, Transport } from "../transport";
import { Button, Input, NumberInput, Notice, Segmented, Select, Textarea } from "../components/ui";
import { Row } from "./fields";

const defaultPorts: Record<ProxyKind, number> = { http: 8080, https: 443, socks5: 1080 };

/** "http proxy.local:8080 as ann", "the system's settings: direct". */
export function describeProxy(p: ProxyInEffect): string {
  if (p.mode === "none" || !p.host) return p.mode === "system" ? "The system has no proxy: direct connections." : "Direct connections.";
  const from = p.source === "os" ? " (system settings)" : p.source === "env" ? " (environment)" : "";
  return `${p.kind ?? "http"} ${p.host}:${p.port ?? ""}${p.user ? ` as ${p.user}` : ""}${from}`;
}

/** The proxy every request goes through (spec 025 FR-020 to FR-024). */
export function ProxyPanel({
  proxy,
  probeUrl,
  transport,
  issues,
  onChange,
}: {
  proxy: ProxySettings;
  /** The start URL; Test requests its robots.txt through the proxy. */
  probeUrl: string;
  transport: Transport;
  issues: (prefix: string) => Issue[];
  onChange: (p: ProxySettings) => void;
}) {
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<ProxyTestResult | null>(null);
  const set = (patch: Partial<ProxySettings>) => {
    setResult(null);
    onChange({ ...proxy, ...patch });
  };
  const setMode = (mode: ProxyMode) => set(mode === "manual" && proxy.port === 0 ? { mode, port: defaultPorts[proxy.kind] } : { mode });
  const test = async () => {
    setTesting(true);
    try {
      setResult(await transport.proxyTest(proxy, probeUrl));
    } catch (e) {
      setResult({ ok: false, in_effect: null, problem: "invalid", message: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
      <Row
        label="Proxy"
        help={
          proxy.mode === "system"
            ? "The operating system's proxy settings, read when the crawl starts. An automatic configuration script (PAC) set there is used too; automatic discovery without an address (WPAD) is not."
            : proxy.mode === "manual"
              ? "Every request, robots.txt and sitemaps included, goes through this proxy."
              : "Direct connections, whatever the environment says."
        }
        issues={issues("access.proxy.mode")}
      >
        {() => (
          <Segmented
            label="Proxy"
            value={proxy.mode}
            options={[
              { value: "none", label: "None" },
              { value: "system", label: "System" },
              { value: "manual", label: "Manual" },
            ]}
            onChange={setMode}
          />
        )}
      </Row>
      {proxy.mode === "manual" && (
        <>
          <Row label="Address" issues={[...issues("access.proxy.host"), ...issues("access.proxy.port")]}>
            {(id) => (
              <span className="flex items-center gap-2">
                <Select
                  aria-label="Proxy type"
                  className="w-24"
                  value={proxy.kind}
                  onChange={(e) => {
                    const kind = e.target.value as ProxyKind;
                    set({ kind, port: proxy.port === defaultPorts[proxy.kind] ? defaultPorts[kind] : proxy.port });
                  }}
                >
                  <option value="http">HTTP</option>
                  <option value="https">HTTPS</option>
                  <option value="socks5">SOCKS5</option>
                </Select>
                <Input id={id} aria-label="Proxy host" placeholder="proxy.example.com" mono className="w-48" value={proxy.host} onChange={(e) => set({ host: e.target.value })} />
                <NumberInput aria-label="Proxy port" min={1} max={65535} value={proxy.port} onChange={(v) => set({ port: v ?? 0 })} />
              </span>
            )}
          </Row>
          <Row label="User" help="Leave blank when the proxy takes no sign-in." issues={issues("access.proxy.user")}>
            {(id) => <Input id={id} className="w-48" value={proxy.user ?? ""} onChange={(e) => set({ user: e.target.value === "" ? null : e.target.value })} />}
          </Row>
          {proxy.user && (
            <Row label="Password" help="Kept for this session only, never saved." issues={issues("access.proxy.password")}>
              {(id) => (
                <Input
                  id={id}
                  type="password"
                  className="w-48"
                  placeholder="Enter password"
                  value={proxy.password.value ?? ""}
                  onChange={(e) => set({ password: { secret: true, value: e.target.value } })}
                />
              )}
            </Row>
          )}
          <Row label="Reach directly" stacked help="One host per line: localhost, *.example.com or 10.0.0.0/8." issues={issues("access.proxy.bypass")}>
            {(id) => (
              <Textarea
                id={id}
                rows={2}
                value={proxy.bypass.join("\n")}
                onChange={(e) => set({ bypass: e.target.value.split("\n").map((l) => l.trim()).filter(Boolean) })}
              />
            )}
          </Row>
        </>
      )}
      {proxy.mode !== "none" && (
        <Row label="Check the proxy" help="Requests the start URL's robots.txt through it.">
          {() => (
            <span className="flex flex-col items-end gap-1.5">
              <Button size="sm" disabled={testing || !probeUrl.trim()} onClick={test}>
                {testing ? "Testing…" : "Test"}
              </Button>
              {result &&
                (result.ok ? (
                  <Notice kind="info">Works: {result.in_effect ? describeProxy(result.in_effect) : "direct connections."}</Notice>
                ) : (
                  <Notice kind="bad" role="alert">
                    {result.message ?? "The proxy did not answer."}
                  </Notice>
                ))}
            </span>
          )}
        </Row>
      )}
    </>
  );
}
