import { useCallback, useEffect, useState } from "react";
import { Check, Copy, FolderPlus, X } from "lucide-react";
import type { ActivityEntry } from "../generated/ActivityEntry";
import type { AssistantSettings } from "../generated/AssistantSettings";
import type { AssistantsView, McpSelfTest, Transport } from "../transport";
import { Badge, Button, Notice, Segmented, Switch } from "../components/ui";
import { formatBytes } from "../results/format";
import { Group, Row } from "./fields";

const ACTIVITY = 20;

/**
 * Settings › AI assistants (spec 016 US1, FR-020 to FR-022, FR-028): whether
 * assistants may read crawls through SpiderLens's local MCP server, which
 * folders, how to add SpiderLens to an assistant, and what they did. Saved on
 * this computer; the server reads the same file on every request.
 */
export function AssistantsSettings({ transport }: { transport: Transport }) {
  const [view, setView] = useState<AssistantsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);

  const loadActivity = useCallback(() => {
    transport.assistantsActivity(ACTIVITY).then(setActivity, () => setActivity([]));
  }, [transport]);

  useEffect(() => {
    transport.assistantsGet().then(setView, (e) => setError(String(e)));
    loadActivity();
  }, [transport, loadActivity]);

  if (!view) return error ? <Notice kind="bad">{error}</Notice> : <div aria-busy className="text-sm text-fg-3" />;

  const save = (patch: Partial<AssistantSettings>) => {
    const next = { ...view.settings, ...patch };
    setError(null);
    setView({ ...view, settings: next });
    transport.assistantsSet(next).then(
      (settings) => setView((v) => v && { ...v, settings }),
      (e) => {
        setError(String(e));
        setView((v) => v && { ...v, settings: view.settings });
      },
    );
  };
  const s = view.settings;
  const addFolder = async () => {
    const f = await transport.pickFolder();
    if (f && !s.allowed_folders.includes(f)) save({ allowed_folders: [...s.allowed_folders, f] });
  };

  return (
    <>
      <Group>
        <Row
          label="Allow AI assistants to read my crawls"
          help="Assistants you connect can read your recent crawls and the crawl files in the folders below. They cannot change files. Starting crawls is a separate switch below."
        >
          {(_, l) => <Switch labelledBy={l} checked={s.allow_read} onChange={(v) => save({ allow_read: v })} />}
        </Row>
        <Row label="Folders" stacked help="Crawl files directly inside these folders can be read too.">
          {() => (
            <div className="flex flex-col gap-1.5">
              {s.allowed_folders.length > 0 && (
                <ul aria-label="Allowed folders" className="m-0 flex list-none flex-col divide-y divide-line rounded-md border border-line p-0">
                  {s.allowed_folders.map((f) => (
                    <li key={f} className="flex items-center gap-2 px-2.5 py-1 text-sm">
                      <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg-2" title={f}>
                        {f}
                      </span>
                      <Button size="sm" variant="quiet" icon aria-label={`Remove ${f}`} onClick={() => save({ allowed_folders: s.allowed_folders.filter((x) => x !== f) })}>
                        <X className="size-3.5" />
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
              <Button size="sm" className="self-start" onClick={() => void addFolder()}>
                <FolderPlus className="size-3.5" /> Add folder…
              </Button>
            </div>
          )}
        </Row>
      </Group>
      <Group title="Crawling">
        <Row
          label="Allow AI assistants to start crawls"
          help="An assistant can then start, watch, pause and stop crawls with the profiles you allow below. Those crawls send requests to websites from this computer, under your name and address. Assistants cannot use profiles that ignore robots.txt or change a profile's settings. Crawls they start are marked “Started by an AI assistant”, and if the assistant disconnects they are paused."
        >
          {(_, l) => <Switch labelledBy={l} checked={s.allow_crawl} onChange={(v) => save({ allow_crawl: v })} />}
        </Row>
        <Row label="Profiles assistants may use" stacked help="Only saved profiles that respect robots.txt can be allowed.">
          {() =>
            view.profiles.length === 0 ? (
              <p className="m-0 text-sm text-fg-3">No saved profiles yet. Save a profile first, then allow it here.</p>
            ) : (
              <ul aria-label="Allowed profiles" className="m-0 flex list-none flex-col divide-y divide-line rounded-md border border-line p-0">
                {view.profiles.map((p) => {
                  const on = s.allowed_profiles.includes(p.name);
                  return (
                    <li key={p.name} className="flex items-center gap-2 px-2.5 py-1.5 text-sm">
                      <span className="min-w-0 flex-1 truncate text-fg">{p.name}</span>
                      {p.reason && <span className="shrink-0 text-xs text-fg-3">{p.reason}</span>}
                      <Switch
                        label={`Allow ${p.name}`}
                        checked={on && p.respects_robots}
                        disabled={!s.allow_crawl || !p.respects_robots}
                        onChange={(v) => save({ allowed_profiles: v ? [...s.allowed_profiles, p.name] : s.allowed_profiles.filter((x) => x !== p.name) })}
                      />
                    </li>
                  );
                })}
              </ul>
            )
          }
        </Row>
      </Group>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
      <Notice kind="warn">
        Pages you crawl can contain text written to mislead an AI. SpiderLens hands it to assistants marked as website content, but an assistant may still act on it.
      </Notice>

      <Connect view={view} transport={transport} />

      <Group title="Recent activity">
        {activity.length === 0 ? (
          <div className="px-3.5 py-2.5 text-sm text-fg-3">No assistant has used SpiderLens yet.</div>
        ) : (
          <>
            <ul aria-label="Recent assistant activity" className="m-0 flex list-none flex-col divide-y divide-line p-0">
              {activity.map((a, i) => (
                <li key={`${a.at}-${i}`} className="flex items-baseline gap-2 px-3.5 py-1.5 text-sm">
                  <time className="w-28 shrink-0 text-xs text-fg-3 tabular" dateTime={a.at}>
                    {new Date(a.at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}
                  </time>
                  <span className="min-w-0 flex-1 truncate text-fg-2">{a.client ?? "Unknown assistant"}</span>
                  <span className="shrink-0 font-mono text-xs text-fg">{a.tool}</span>
                  {a.refused ? (
                    <Badge tone="warn">Refused</Badge>
                  ) : a.error ? (
                    <Badge tone="bad">Error</Badge>
                  ) : (
                    <span className="w-14 shrink-0 text-right text-xs text-fg-3 tabular">{formatBytes(a.bytes)}</span>
                  )}
                </li>
              ))}
            </ul>
            <div className="px-3.5 py-2">
              <Button size="sm" variant="quiet" onClick={() => void transport.assistantsActivityClear().then(loadActivity)}>
                Clear activity
              </Button>
            </div>
          </>
        )}
      </Group>
    </>
  );
}

/** The configuration snippet for one assistant, with Copy and Test. */
function Connect({ view, transport }: { view: AssistantsView; transport: Transport }) {
  const [client, setClient] = useState(view.snippets[0]?.client ?? "");
  const [copied, setCopied] = useState(false);
  const [test, setTest] = useState<{ ok: McpSelfTest } | { error: string } | "running" | null>(null);
  const snippet = view.snippets.find((x) => x.client === client) ?? view.snippets[0];
  if (!snippet) return null;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(snippet.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };
  const run = () => {
    setTest("running");
    transport.assistantsTest().then(
      (ok) => setTest({ ok }),
      (e) => setTest({ error: String(e) }),
    );
  };
  return (
    <Group title="Connect an assistant">
      <Row label="Assistant" stacked help={snippet.hint}>
        {() => (
          <div className="flex flex-col gap-2">
            <Segmented
              label="Assistant"
              value={snippet.client}
              onChange={(c) => (setClient(c), setCopied(false))}
              options={view.snippets.map((x) => ({ value: x.client, label: x.label }))}
            />
            <div className="relative">
              <pre aria-label={`${snippet.label} configuration`} data-selectable className="m-0 max-h-40 overflow-auto rounded-md border border-line bg-surface-2 p-2.5 pr-20 font-mono text-xs whitespace-pre-wrap break-all text-fg">
                {snippet.text}
              </pre>
              <Button size="sm" className="absolute top-1.5 right-1.5" onClick={() => void copy()}>
                {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />} {copied ? "Copied" : "Copy"}
              </Button>
            </div>
          </div>
        )}
      </Row>
      <Row label="Check the server" help="Starts SpiderLens's MCP server the way an assistant does and lists what it offers.">
        {() => (
          <Button size="sm" disabled={test === "running"} onClick={run}>
            {test === "running" ? "Checking…" : "Test"}
          </Button>
        )}
      </Row>
      {test && test !== "running" && (
        <div className="px-3.5 py-2.5" role="status">
          {"ok" in test ? (
            <Notice kind="info">
              The server works: {test.ok.tools.length} tools, protocol {test.ok.protocol}.{" "}
              {test.ok.read_access ? "Read access is on." : "Read access is off, so assistants are told to turn it on here."}{" "}
              {test.ok.crawl_access ? "Starting crawls is on." : "Starting crawls is off."}
            </Notice>
          ) : (
            <Notice kind="bad">{test.error}</Notice>
          )}
        </div>
      )}
    </Group>
  );
}
