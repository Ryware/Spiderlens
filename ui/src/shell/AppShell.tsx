import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Transport } from "../transport";
import { useCrawlRun } from "../crawl/useCrawlRun";
import { useAnalysis } from "../results/useAnalysis";
import { useCrawlSetup } from "../crawl/useCrawlSetup";
import { CrawlLog } from "../crawl/CrawlLog";
import { RetryDialog, useRetryAvailability } from "../crawl/RetryFailed";
import { SettingsSheet } from "../settings/SettingsSheet";
import { IntegrationsView } from "../integrations/IntegrationsView";
import { categories, type CategoryId } from "../settings/categories";
import { cn } from "../components/ui";
import { Toolbar } from "./Toolbar";
import { reportTabs, resultTabs, Sidebar, vizTabs } from "./Sidebar";
import { VizView, vizViews } from "../viz/VizView";
import { AboutDialog, CrawlOverview, DetailPanel, StartState, StatusBar } from "./panels";
import { CommandPalette } from "./CommandPalette";
import { shortcutCommand, type Command } from "./commands";
import { matches, platform } from "./platform";
import { useAppearance, useUiState } from "./useUiState";
import { ACCENTS, chooseLevel, LEVELS } from "./appearance";
import { ResultsView } from "../results/ResultsView";
import { Inspector } from "../results/Inspector";
import { GenerateSitemapSheet } from "../results/GenerateSitemapSheet";
import { SitemapFilesView } from "../sitemaps/SitemapFiles";
import { CookiesView } from "../results/CookiesView";
import { useResults } from "../results/useResults";
import { issueOfTab, issueTab, useIssues } from "../issues/useIssues";
import { isCustomTab, useCustomRun } from "../results/useCustomRun";
import { useCrawlLibrary } from "../files/useCrawlLibrary";
import { useCrawlFile } from "../files/useCrawlFile";
import { CrawlFileDialogs } from "../files/CrawlFileDialogs";
import { CrawlInfoSheet } from "../files/CrawlInfoSheet";
import { ManageCrawls } from "../files/ManageCrawls";
import { CleanupNotice } from "../files/CleanupNotice";
import type { CrawlInfo } from "../transport";
import type { Schedule } from "../generated/Schedule";
import { useSchedules } from "../schedules/useSchedules";
import { SchedulesView } from "../schedules/SchedulesView";
import { ScheduleEditor } from "../schedules/ScheduleEditor";
import { newDraft } from "../schedules/format";

/** Below these window widths the detail panel, then the sidebar, collapse (FR-005). */
export const AUTO_COLLAPSE_DETAIL = 1100;
export const AUTO_COLLAPSE_SIDEBAR = 900;
const SIDEBAR = { min: 180, max: 320, icons: 52 };
const DETAIL = { min: 240, max: 440 };

function useWindowWidth() {
  const [w, setW] = useState(() => window.innerWidth);
  useEffect(() => {
    const on = () => setW(window.innerWidth);
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return w;
}

/** Draggable, keyboard-adjustable edge between two regions. */
function Splitter({ label, value, min, max, onChange, invert }: { label: string; value: number; min: number; max: number; onChange: (v: number) => void; invert?: boolean }) {
  const start = useRef<{ x: number; v: number } | null>(null);
  const clamp = (v: number) => Math.round(Math.max(min, Math.min(max, v)));
  return (
    <div
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      tabIndex={0}
      className="relative z-10 -mx-1 w-2 shrink-0 cursor-col-resize after:absolute after:inset-y-0 after:left-1 after:w-px after:bg-line hover:after:bg-accent focus-visible:after:bg-accent focus-visible:outline-none"
      onPointerDown={(e) => {
        start.current = { x: e.clientX, v: value };
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (!start.current) return;
        const dx = e.clientX - start.current.x;
        onChange(clamp(start.current.v + (invert ? -dx : dx)));
      }}
      onPointerUp={() => (start.current = null)}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 40 : 10;
        if (e.key === "ArrowLeft") onChange(clamp(value + (invert ? step : -step)));
        if (e.key === "ArrowRight") onChange(clamp(value + (invert ? -step : step)));
      }}
    />
  );
}

/**
 * The window (US1): toolbar, sidebar, main area, detail panel, status bar.
 * Crawl settings open as a sheet over it (US2).
 */
export function AppShell({ transport }: { transport: Transport }) {
  const setup = useCrawlSetup(transport);
  const crawl = useCrawlRun(transport);
  const [ui, updateUi] = useUiState();
  const { level, systemDark } = useAppearance(ui);
  const width = useWindowWidth();
  const [section, setSection] = useState("overview");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [sitemapOpen, setSitemapOpen] = useState(false);
  const [retryOpen, setRetryOpen] = useState(false);
  const retry = useRetryAvailability(transport, crawl);
  const [infoOpen, setInfoOpen] = useState(false);
  /** Crawl information for a file other than the open crawl (from Manage crawls). */
  const [infoPath, setInfoPath] = useState<string | null>(null);
  const [manageOpen, setManageOpen] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorDraft, setEditorDraft] = useState<Schedule | null>(null);
  const [scheduleVersion, setScheduleVersion] = useState(0);
  const urlRef = useRef<HTMLInputElement>(null);
  const inspectorRef = useRef<HTMLDivElement>(null);
  const inspectFrom = useRef<HTMLElement | null>(null);
  const inspectPending = useRef(false);
  const [inspectAsked, setInspectAsked] = useState(0);
  const results = useResults(transport, crawl.path, crawl.running);
  const issues = useIssues(transport, crawl.path, crawl.running, setup.config?.issues);
  const analysis = useAnalysis(transport, crawl.path, crawl.phase, crawl.running, setup.config?.links.auto_analyse ?? true);
  const resultTab = crawl.path ? results.tabs.find((t) => t.id === section) : undefined;
  const issueId = resultTab ? issueOfTab(resultTab.id) : null;
  const vizView = crawl.path ? vizViews.find((v) => v.id === section) : undefined;
  const cookiesOpen = crawl.path !== null && section === "cookies";
  // Kept here so its progress and outcome outlive the tab's reload when it finishes.
  const customRun = useCustomRun(transport, crawl.path);

  const onFocusId = results.setFocusId;
  // Opening the anchor text cloud from the details starts a fresh view for the selected page.
  const [anchorOpens, setAnchorOpens] = useState(0);
  const openAnchorCloud = useCallback(() => {
    setAnchorOpens((n) => n + 1);
    setSection("anchor-text");
  }, []);
  const onSession = useCallback((patch: Parameters<typeof results.update>[1]) => resultTab && results.update(resultTab.id, patch), [results, resultTab]);

  // "Show in table" from the inspector (T027): the URL is selected in the
  // current tab when it lists the URL, else in Internal or External, else
  // in the first tab that does. `n` makes a repeat of the same URL count.
  const [reveal, setReveal] = useState<{ tab: string; id: number; n: number } | null>(null);
  useEffect(() => {
    if (reveal && reveal.tab !== section) setReveal(null);
  }, [section, reveal]);
  const showInTable = useCallback(
    async (id: number) => {
      if (!crawl.path) return false;
      const listed = (await transport.urlTabs(crawl.path, id)).filter((t) => results.tabs.some((x) => x.id === t));
      const tab = resultTab && listed.includes(resultTab.id) ? resultTab.id : (listed.find((t) => t === "internal" || t === "external") ?? listed[0]);
      if (!tab) return false;
      results.update(tab, { focusId: id });
      setSection(tab);
      setReveal((r) => ({ tab, id, n: (r?.n ?? 0) + 1 }));
      return true;
    },
    [transport, crawl.path, results, resultTab],
  );

  useEffect(() => {
    document.documentElement.dataset.platform = platform;
  }, []);

  // Edited link analysis settings apply to the open crawl file too, which
  // then works out positions and Link Score again (spec 023 FR-023). The
  // file's own settings stand until the user changes them.
  const links = setup.config?.links;
  const linksKey = links ? JSON.stringify(links) : null;
  const lastLinks = useRef<string | null>(null);
  useEffect(() => {
    const before = lastLinks.current;
    lastLinks.current = linksKey;
    if (links && crawl.path && before !== null && before !== linksKey) void transport.analysisSettings(crawl.path, links).catch(() => {});
  }, [linksKey, crawl.path, transport]);

  // Focus the start URL once the window is ready (US1-2).
  const configReady = setup.config !== null;
  useEffect(() => {
    if (configReady) urlRef.current?.focus();
  }, [configReady]);

  const sidebarIcons = ui.sidebarCollapsed || width < AUTO_COLLAPSE_SIDEBAR;
  const detailHidden = ui.detailCollapsed || width < AUTO_COLLAPSE_DETAIL;

  // Enter on a row (FR-018): show the details if they were hidden, then move
  // focus into them; Escape there goes back to the row.
  const inspect = useCallback(() => {
    inspectFrom.current = document.activeElement as HTMLElement | null;
    inspectPending.current = true;
    if (ui.detailCollapsed) updateUi({ detailCollapsed: false });
    setInspectAsked((n) => n + 1);
  }, [ui.detailCollapsed, updateUi]);
  useEffect(() => {
    // A window too narrow for details keeps focus on the row.
    if (inspectPending.current) inspectorRef.current?.focus();
    inspectPending.current = false;
  }, [inspectAsked, detailHidden]);

  const openSettings = useCallback(
    (category?: CategoryId) => {
      if (category) updateUi({ settingsCategory: category });
      setSettingsOpen(true);
    },
    [updateUi],
  );

  // Not gated on setup.canStart: that reflects the last validated value, which
  // can lag behind the field. crawlInput validates what is there now.
  const starting = useRef(false);
  const startCrawl = useCallback(async () => {
    if (starting.current || crawl.active) return;
    starting.current = true;
    try {
      const input = await setup.crawlInput();
      if (!input) return;
      setSection("overview");
      crawl.start(input.config, input.startUrl, input.target);
    } finally {
      starting.current = false;
    }
  }, [setup, crawl]);

  const library = useCrawlLibrary(transport);
  const file = useCrawlFile(transport, crawl, {
    onOpened: () => setSection((s) => (s === "overview" ? "internal" : s)),
    onChanged: library.reload,
  });

  // The crawl's settings go back into the setup, and the window returns to the start screen.
  const newCrawlFrom = useCallback(
    (info: CrawlInfo) => {
      if (!info.config || !crawl.close()) return;
      const config = info.config;
      setup.set(() => config);
      setup.setLoaded(null);
      if (config.mode === "spider" && info.start_url) setup.setStartUrl(info.start_url);
      setInfoOpen(false);
      setSection("overview");
    },
    [crawl, setup],
  );
  const { resume: resumeFile, saveAs: saveFileAs, open: openFile, openPicker } = file;
  const resume = useCallback(() => void resumeFile(), [resumeFile]);
  const saveAs = useCallback(() => void saveFileAs(), [saveFileAs]);
  const recentOpen = useMemo(() => library.recent.filter((e) => e.available).slice(0, 5), [library.recent]);

  const schedules = useSchedules(transport);
  const editSchedule = useCallback((s: Schedule) => {
    setEditorDraft(structuredClone(s));
    setEditorOpen(true);
  }, []);
  // A new schedule from settings and an input (spec 021 FR-002).
  const scheduleFrom = useCallback(
    async (config: CrawlInfo["config"], startUrl: string | null, profile: string | null) => {
      if (!config) return;
      const outputDir = (await transport.scheduleDefaultOutput().catch(() => null)) ?? "";
      setSettingsOpen(false);
      setInfoOpen(false);
      setSection("schedules");
      editSchedule(newDraft({ config, startUrl, profile, outputDir }));
    },
    [transport, editSchedule],
  );
  const scheduleThisCrawl = useCallback(
    () => void scheduleFrom(setup.config, setup.startUrl.trim() || null, setup.loaded?.name ?? null),
    [scheduleFrom, setup.config, setup.startUrl, setup.loaded],
  );

  const hasInput = setup.config?.mode === "list" || setup.startUrl.trim() !== "";
  const commands: Command[] = useMemo(
    () => [
      // Not gated on setup.canStart either: Mod+Enter right after typing must
      // reach startCrawl, which checks the field as it is now (like Enter).
      { id: "start", title: "Start crawl", group: "Crawl", shortcut: "Mod+Enter", enabled: hasInput && !crawl.active, run: startCrawl },
      { id: "pause", title: "Pause crawl", group: "Crawl", enabled: crawl.phase === "running", run: crawl.pause },
      { id: "resume", title: "Resume crawl", group: "Crawl", enabled: crawl.phase === "paused", run: resume },
      { id: "retry", title: "Retry failed URLs", group: "Crawl", keywords: ["again", "errors", "transient"], enabled: retry.available, reason: retry.reason, run: () => setRetryOpen(true) },
      { id: "stop", title: "Stop crawl", group: "Crawl", keywords: ["cancel", "end"], enabled: crawl.phase === "running" || crawl.phase === "paused", run: crawl.stop },
      { id: "open", title: "Open crawl…", group: "Crawl", shortcut: "Mod+O", keywords: ["file", "load", "results"], run: () => void openPicker() },
      ...recentOpen.map((e) => ({ id: `open-recent:${e.path}`, title: `Open recent: ${e.name || e.start_url || e.path}`, group: "Crawl" as const, keywords: [e.path], run: () => void openFile(e.path) })),
      { id: "save-as", title: "Save crawl as…", group: "Crawl", shortcut: "Mod+Shift+S", keywords: ["file", "export", "move"], enabled: crawl.path !== null, run: saveAs },
      { id: "crawl-info", title: "Crawl information", group: "Crawl", keywords: ["configuration", "version", "history", "file"], enabled: crawl.path !== null, run: () => setInfoOpen(true) },
      {
        id: "generate-sitemap",
        title: "Generate XML sitemap…",
        group: "Crawl",
        keywords: ["sitemap", "xml", "export"],
        enabled: crawl.path !== null && setup.config !== null,
        run: () => setSitemapOpen(true),
      },
      { id: "manage-crawls", title: "Manage crawls…", group: "Crawl", keywords: ["disk", "space", "delete", "cleanup", "storage", "files", "trash"], run: () => setManageOpen(true) },
      {
        id: "analyse",
        title: "Work out Link Score",
        group: "Crawl",
        keywords: ["analysis", "link positions", "pagerank"],
        enabled: crawl.path !== null && !crawl.running && analysis.status !== "running",
        run: analysis.start,
      },
      { id: "schedule-crawl", title: "Schedule this crawl…", group: "Crawl", keywords: ["schedule", "recurring", "repeat", "every", "cron", "new schedule"], enabled: setup.config !== null, run: scheduleThisCrawl },
      { id: "settings", title: "Crawl settings", group: "Crawl", shortcut: "Mod+,", keywords: ["configure", "preferences"], run: () => openSettings() },
      { id: "focus-url", title: "Edit start URL", group: "Crawl", shortcut: "Mod+L", run: () => urlRef.current?.focus() },
      { id: "mode-spider", title: "Switch to Spider mode", group: "Crawl", enabled: !crawl.active, run: () => setup.set((c) => ({ ...c, mode: "spider" })) },
      { id: "mode-list", title: "Switch to List mode", group: "Crawl", enabled: !crawl.active, run: () => setup.set((c) => ({ ...c, mode: "list" })) },
      { id: "speed-polite", title: "Speed: Polite", group: "Crawl", run: () => setup.presets && setup.set((c) => ({ ...c, speed: setup.presets!.polite })) },
      { id: "speed-fast", title: "Speed: Fast (sites you control)", group: "Crawl", run: () => setup.presets && setup.set((c) => ({ ...c, speed: setup.presets!.fast })) },
      ...categories
        .filter((c) => !c.listOnly)
        .map((c) => ({
          id: `settings-${c.id}`,
          title: `Settings: ${c.title}`,
          group: "Go to" as const,
          run: () => openSettings(c.id),
        })),
      { id: "go-overview", title: "Overview", group: "Go to", run: () => setSection("overview") },
      { id: "go-integrations", title: "Integrations", group: "Go to", keywords: ["search console", "google", "external data"], run: () => setSection("integrations") },
      { id: "go-log", title: "Crawl log", group: "Go to", keywords: ["retries", "failures", "events", "history"], enabled: crawl.path !== null, run: () => setSection("log") },
      { id: "go-schedules", title: "Schedules", group: "Go to", keywords: ["scheduled crawls", "history"], run: () => setSection("schedules") },
      // Mod+1…Mod+9 open the first nine result tabs (FR-018).
      ...resultTabs.map((t, i) => ({ id: `go-${t.id}`, title: t.title, group: "Go to" as const, shortcut: i < 9 ? `Mod+${i + 1}` : undefined, enabled: crawl.path !== null, run: () => setSection(t.id) })),
      ...issues.rules.map((r) => ({
        id: `go-issue-${r.id}`,
        title: `Issue: ${r.name}`,
        group: "Go to" as const,
        keywords: ["issue", r.category_label],
        enabled: crawl.path !== null,
        run: () => setSection(issueTab(r.id)),
      })),
      ...vizTabs.map((t) => ({ id: `go-${t.id}`, title: t.title, group: "Go to" as const, keywords: ["visualisation", "visualization", "graph"], enabled: crawl.path !== null, run: () => setSection(t.id) })),
      ...reportTabs.map((t) => ({ id: `go-${t.id}`, title: t.title, group: "Go to" as const, enabled: crawl.path !== null, run: () => setSection(t.id) })),
      { id: "toggle-sidebar", title: ui.sidebarCollapsed ? "Show sidebar" : "Collapse sidebar", group: "View", shortcut: "Mod+Alt+S", run: () => updateUi({ sidebarCollapsed: !ui.sidebarCollapsed }) },
      { id: "toggle-detail", title: ui.detailCollapsed ? "Show details" : "Hide details", group: "View", shortcut: "Mod+Alt+I", run: () => updateUi({ detailCollapsed: !ui.detailCollapsed }) },
      { id: "theme-system", title: "Appearance: Follow system", group: "View", keywords: ["theme", "brightness"], run: () => updateUi({ followSystem: true }) },
      ...LEVELS.map((l) => ({
        id: `level-${l.id}`,
        title: `Brightness: ${l.name}`,
        group: "View" as const,
        keywords: ["theme", "appearance", l.tone],
        run: () => updateUi(chooseLevel(ui, l.id, systemDark)),
      })),
      ...ACCENTS.map((p) => ({ id: `accent-${p.id}`, title: `Accent: ${p.name}`, group: "View" as const, keywords: ["theme", "appearance", "palette", "colour", "color"], run: () => updateUi({ accent: p.id }) })),
      { id: "about", title: "About SpiderLens", group: "Help", keywords: ["version", "build"], run: () => setAboutOpen(true) },
    ],
    [setup, crawl, retry, hasInput, startCrawl, scheduleThisCrawl, openPicker, openFile, recentOpen, resume, analysis, saveAs, openSettings, ui, updateUi, systemDark, issues.rules],
  );

  const run = useCallback(
    (c: Command) => {
      updateUi({ recentCommands: [c.id, ...ui.recentCommands.filter((x) => x !== c.id)].slice(0, 5) });
      c.run();
    },
    [ui.recentCommands, updateUi],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (matches(e, "Mod+K")) {
        e.preventDefault();
        setPaletteOpen((o) => !o);
        return;
      }
      if (settingsOpen || paletteOpen || aboutOpen || infoOpen || retryOpen || sitemapOpen || manageOpen || editorOpen || file.prompt || file.quitting) return;
      const c = shortcutCommand(commands, e, matches);
      if (c) {
        e.preventDefault();
        c.run();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [commands, settingsOpen, paletteOpen, aboutOpen, infoOpen, retryOpen, sitemapOpen, manageOpen, editorOpen, file.prompt, file.quitting]);

  return (
    <div data-shell className="flex h-full flex-col bg-bg text-fg">
      <Toolbar
        ref={urlRef}
        setup={setup}
        run={crawl}
        onStart={startCrawl}
        onResume={resume}
        onOpenSettings={openSettings}
        onOpenPalette={() => setPaletteOpen(true)}
        appearance={ui}
        level={level}
        systemDark={systemDark}
        onAppearance={updateUi}
        retry={retry}
        onRetry={() => setRetryOpen(true)}
      />
      <div data-shell className="flex min-h-0 flex-1 bg-frame">
        <div data-region="chrome" className={cn("shrink-0 border-r border-line", sidebarIcons && "border-r")} style={{ width: sidebarIcons ? SIDEBAR.icons : ui.sidebarWidth }}>
          <Sidebar active={section} onSelect={setSection} collapsed={sidebarIcons} onAbout={() => setAboutOpen(true)} results={crawl.path !== null} counts={crawl.path ? results.counts : null} issues={issues.summary} counting={issues.counting} custom={results.tabs.filter((t) => isCustomTab(t.id))} />
        </div>
        {!sidebarIcons && <Splitter label="Resize sidebar" value={ui.sidebarWidth} min={SIDEBAR.min} max={SIDEBAR.max} onChange={(v) => updateUi({ sidebarWidth: v })} />}
        {/* In Island this is the light panel inset in the dark frame (spec 018 FR-004). */}
        <div className="island-content flex min-w-0 flex-1">
        <main className="min-w-0 flex-1 overflow-hidden bg-bg">
          {section === "schedules" ? (
            <SchedulesView
              transport={transport}
              schedules={schedules}
              version={scheduleVersion}
              onNew={scheduleThisCrawl}
              onEdit={editSchedule}
              onOpenCrawl={(p) => void file.open(p)}
            />
          ) : section === "log" && crawl.path ? (
            <CrawlLog transport={transport} path={crawl.path} live={crawl.running} />
          ) : resultTab && crawl.path ? (
            <ResultsView
              // One instance per tab: a tab switch must not carry the old
              // tab's selection into the new tab's first render.
              // Settings changes reload it: thresholds change the rows (spec 009).
              key={`${crawl.path}\n${resultTab.id}\n${issues.changed}\n${results.epoch}\n${results.features?.available.join(",") ?? ""}`}
              transport={transport}
              path={crawl.path}
              tab={resultTab}
              features={results.features}
              live={crawl.running}
              session={results.session(resultTab.id)}
              onSession={onSession}
              onFocusId={onFocusId}
              issue={issueId ? (issues.byId.get(issueId) ?? null) : null}
              onIssueSettings={() => openSettings("issues")}
              customRun={isCustomTab(resultTab.id) ? customRun : undefined}
              onInspect={inspect}
              reveal={reveal?.tab === resultTab.id ? reveal : null}
            />
          ) : section === "integrations" ? (
            <IntegrationsView transport={transport} path={crawl.path} crawlRunning={crawl.running} onDataChanged={results.reload} />
          ) : section === "sitemap-files" && crawl.path ? (
            <SitemapFilesView key={crawl.path} transport={transport} path={crawl.path} live={crawl.running} />
          ) : vizView && crawl.path ? (
            <VizView key={`${crawl.path}\n${vizView.id}\n${vizView.view === "anchors" ? anchorOpens : 0}`} transport={transport} path={crawl.path} view={vizView.view} live={crawl.running} focusId={results.focusId} onFocusId={onFocusId} />
          ) : cookiesOpen && crawl.path ? (
            <CookiesView key={crawl.path} transport={transport} path={crawl.path} features={results.features} live={crawl.running} onFocusId={onFocusId} />
          ) : crawl.phase === "idle" ? (
            <StartState setup={setup} error={crawl.error} onFocusUrl={() => urlRef.current?.focus()} onOpenSettings={() => openSettings()} library={library} onOpenRecent={(p) => void file.open(p)} onManageCrawls={() => setManageOpen(true)} />
          ) : (
            <CrawlOverview run={crawl} onInfo={() => setInfoOpen(true)} onResume={resume} onSaveAs={saveAs} />
          )}
        </main>
        {!detailHidden && (
          <>
            <Splitter label="Resize details" value={ui.detailWidth} min={DETAIL.min} max={DETAIL.max} invert onChange={(v) => updateUi({ detailWidth: v })} />
            <div
              ref={inspectorRef}
              tabIndex={-1}
              className="shrink-0 border-l border-line outline-none"
              style={{ width: ui.detailWidth }}
              onKeyDown={(e) => {
                if (e.key === "Escape" && !e.defaultPrevented && inspectFrom.current?.isConnected) {
                  e.preventDefault();
                  inspectFrom.current.focus();
                }
              }}
            >
              {resultTab || vizView || cookiesOpen ? (
                <Inspector
                  transport={transport}
                  path={crawl.path}
                  id={results.focusId}
                  issuesVersion={issues.changed}
                  rulesVersion={issues.summary?.rules_version}
                  onShowInTable={showInTable}
                  onAnchorCloud={openAnchorCloud}
                  epoch={results.epoch}
                  rules={setup.config?.custom}
                  aiChecks={results.features?.available.includes("ai_checks") ?? false}
                  onIssue={(id) => setSection(issueTab(id))}
                  onOpenUrl={(url) => {
                    results.update("internal", { search: url, filter: "all" });
                    setSection("internal");
                  }}
                />
              ) : (
                <DetailPanel />
              )}

            </div>
          </>
        )}
        </div>
      </div>
      <StatusBar setup={setup} run={crawl} analysis={analysis} />

      <SettingsSheet
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        setup={setup}
        run={crawl}
        transport={transport}
        category={ui.settingsCategory as CategoryId}
        onCategory={(c) => updateUi({ settingsCategory: c })}
        appearance={ui}
        level={level}
        systemDark={systemDark}
        onAppearance={updateUi}
        crawlPath={crawl.path}
        crawlRunning={crawl.running}
        onSchedule={scheduleThisCrawl}
      />
      <ScheduleEditor
        open={editorOpen}
        onOpenChange={setEditorOpen}
        transport={transport}
        initial={editorDraft}
        onSaved={() => {
          setScheduleVersion((v) => v + 1);
          void schedules.reload();
        }}

      />
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} commands={commands} recent={ui.recentCommands} onRun={run} />
      {crawl.path && setup.config && (
        <GenerateSitemapSheet
          open={sitemapOpen}
          onOpenChange={setSitemapOpen}
          transport={transport}
          path={crawl.path}
          running={crawl.active}
          options={setup.config.sitemap_generate}
          onOptions={(o) => setup.set((c) => ({ ...c, sitemap_generate: o }))}
        />
      )}
      <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} transport={transport} />
      <RetryDialog open={retryOpen} onOpenChange={setRetryOpen} transport={transport} path={crawl.path} onConfirm={() => void crawl.retryFailed()} />
      <CrawlInfoSheet
        open={infoOpen}
        onOpenChange={(o) => {
          setInfoOpen(o);
          if (!o) setInfoPath(null);
        }}
        transport={transport}
        path={infoPath ?? crawl.path}
        onNewCrawl={newCrawlFrom}
        onScheduleAgain={(i) => void scheduleFrom(i.config, i.start_url, null)}
      />
      <ManageCrawls
        open={manageOpen}
        onOpenChange={(o) => {
          setManageOpen(o);
          if (!o) library.reload();
        }}
        transport={transport}
        onOpenCrawl={(p) => void file.open(p)}
        onShowInfo={(p) => {
          setInfoPath(p);
          setInfoOpen(true);
        }}
        onSaveAs={(p) => void saveFileAs(p).then(() => library.reload())}
      />
      <CleanupNotice transport={transport} onManage={() => setManageOpen(true)} onSettings={() => openSettings("storage")} />
      <CrawlFileDialogs file={file} />
    </div>
  );
}
