import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Transport } from "../transport";
import { useCrawlRun } from "../crawl/useCrawlRun";
import { useCrawlSetup } from "../crawl/useCrawlSetup";
import { SettingsSheet } from "../settings/SettingsSheet";
import { categories, type CategoryId } from "../settings/categories";
import { cn } from "../components/ui";
import { Toolbar } from "./Toolbar";
import { resultTabs, Sidebar } from "./Sidebar";
import { AboutDialog, CrawlOverview, DetailPanel, StartState, StatusBar } from "./panels";
import { CommandPalette } from "./CommandPalette";
import { shortcutCommand, type Command } from "./commands";
import { matches, platform } from "./platform";
import { useTheme, useUiState } from "./useUiState";
import { ResultsView } from "../results/ResultsView";
import { Inspector } from "../results/Inspector";
import { useResults } from "../results/useResults";
import { useCrawlLibrary } from "../files/useCrawlLibrary";
import { useCrawlFile } from "../files/useCrawlFile";
import { CrawlFileDialogs } from "../files/CrawlFileDialogs";
import { CrawlInfoSheet } from "../files/CrawlInfoSheet";
import type { CrawlInfo } from "../transport";

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
  useTheme(ui.theme);
  const width = useWindowWidth();
  const [section, setSection] = useState("overview");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [infoOpen, setInfoOpen] = useState(false);
  const urlRef = useRef<HTMLInputElement>(null);
  const results = useResults(transport, crawl.path, crawl.running);
  const resultTab = crawl.path ? results.tabs.find((t) => t.id === section) : undefined;
  const onFocusId = results.setFocusId;
  const onSession = useCallback((patch: Parameters<typeof results.update>[1]) => resultTab && results.update(resultTab.id, patch), [results, resultTab]);

  useEffect(() => {
    document.documentElement.dataset.platform = platform;
  }, []);

  // Focus the start URL once the window is ready (US1-2).
  const configReady = setup.config !== null;
  useEffect(() => {
    if (configReady) urlRef.current?.focus();
  }, [configReady]);

  const sidebarIcons = ui.sidebarCollapsed || width < AUTO_COLLAPSE_SIDEBAR;
  const detailHidden = ui.detailCollapsed || width < AUTO_COLLAPSE_DETAIL;

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

  const commands: Command[] = useMemo(
    () => [
      { id: "start", title: "Start crawl", group: "Crawl", shortcut: "Mod+Enter", enabled: setup.canStart && !crawl.active, run: startCrawl },
      { id: "pause", title: "Pause crawl", group: "Crawl", enabled: crawl.phase === "running", run: crawl.pause },
      { id: "resume", title: "Resume crawl", group: "Crawl", enabled: crawl.phase === "paused", run: resume },
      { id: "stop", title: "Stop crawl", group: "Crawl", keywords: ["cancel", "end"], enabled: crawl.phase === "running" || crawl.phase === "paused", run: crawl.stop },
      { id: "open", title: "Open crawl…", group: "Crawl", shortcut: "Mod+O", keywords: ["file", "load", "results"], run: () => void openPicker() },
      ...recentOpen.map((e) => ({ id: `open-recent:${e.path}`, title: `Open recent: ${e.name || e.start_url || e.path}`, group: "Crawl" as const, keywords: [e.path], run: () => void openFile(e.path) })),
      { id: "save-as", title: "Save crawl as…", group: "Crawl", shortcut: "Mod+Shift+S", keywords: ["file", "export", "move"], enabled: crawl.path !== null, run: saveAs },
      { id: "crawl-info", title: "Crawl information", group: "Crawl", keywords: ["configuration", "version", "history", "file"], enabled: crawl.path !== null, run: () => setInfoOpen(true) },
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
      ...resultTabs.map((t) => ({ id: `go-${t.id}`, title: t.title, group: "Go to" as const, enabled: crawl.path !== null, run: () => setSection(t.id) })),
      { id: "toggle-sidebar", title: ui.sidebarCollapsed ? "Show sidebar" : "Collapse sidebar", group: "View", shortcut: "Mod+Alt+S", run: () => updateUi({ sidebarCollapsed: !ui.sidebarCollapsed }) },
      { id: "toggle-detail", title: ui.detailCollapsed ? "Show details" : "Hide details", group: "View", shortcut: "Mod+Alt+I", run: () => updateUi({ detailCollapsed: !ui.detailCollapsed }) },
      { id: "theme-system", title: "Appearance: System", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "system" }) },
      { id: "theme-light", title: "Appearance: Light", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "light" }) },
      { id: "theme-dark", title: "Appearance: Dark", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "dark" }) },
      { id: "about", title: "About SpiderLens", group: "Help", keywords: ["version", "build"], run: () => setAboutOpen(true) },
    ],
    [setup, crawl, startCrawl, openPicker, openFile, recentOpen, resume, saveAs, openSettings, ui.sidebarCollapsed, ui.detailCollapsed, updateUi],
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
      if (settingsOpen || paletteOpen || aboutOpen || infoOpen || file.prompt || file.quitting) return;
      const c = shortcutCommand(commands, e, matches);
      if (c) {
        e.preventDefault();
        c.run();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [commands, settingsOpen, paletteOpen, aboutOpen, infoOpen, file.prompt, file.quitting]);

  return (
    <div className="flex h-full flex-col bg-bg text-fg">
      <Toolbar ref={urlRef} setup={setup} run={crawl} onStart={startCrawl} onResume={resume} onOpenSettings={openSettings} onOpenPalette={() => setPaletteOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <div className={cn("shrink-0 border-r border-line", sidebarIcons && "border-r")} style={{ width: sidebarIcons ? SIDEBAR.icons : ui.sidebarWidth }}>
          <Sidebar active={section} onSelect={setSection} collapsed={sidebarIcons} onAbout={() => setAboutOpen(true)} results={crawl.path !== null} />
        </div>
        {!sidebarIcons && <Splitter label="Resize sidebar" value={ui.sidebarWidth} min={SIDEBAR.min} max={SIDEBAR.max} onChange={(v) => updateUi({ sidebarWidth: v })} />}
        <main className="min-w-0 flex-1 overflow-hidden bg-bg">
          {resultTab && crawl.path ? (
            <ResultsView
              key={crawl.path}
              transport={transport}
              path={crawl.path}
              tab={resultTab}
              features={results.features}
              live={crawl.running}
              session={results.session(resultTab.id)}
              onSession={onSession}
              onFocusId={onFocusId}
            />
          ) : crawl.phase === "idle" ? (
            <StartState setup={setup} onFocusUrl={() => urlRef.current?.focus()} onOpenSettings={() => openSettings()} library={library} onOpenRecent={(p) => void file.open(p)} />
          ) : (
            <CrawlOverview run={crawl} onInfo={() => setInfoOpen(true)} onResume={resume} onSaveAs={saveAs} />
          )}
        </main>
        {!detailHidden && (
          <>
            <Splitter label="Resize details" value={ui.detailWidth} min={DETAIL.min} max={DETAIL.max} invert onChange={(v) => updateUi({ detailWidth: v })} />
            <div className="shrink-0 border-l border-line" style={{ width: ui.detailWidth }}>
              {resultTab ? <Inspector transport={transport} path={crawl.path} id={results.focusId} /> : <DetailPanel />}
            </div>
          </>
        )}
      </div>
      <StatusBar setup={setup} run={crawl} />

      <SettingsSheet
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        setup={setup}
        transport={transport}
        category={ui.settingsCategory as CategoryId}
        onCategory={(c) => updateUi({ settingsCategory: c })}
      />
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} commands={commands} recent={ui.recentCommands} onRun={run} />
      <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} transport={transport} />
      <CrawlInfoSheet open={infoOpen} onOpenChange={setInfoOpen} transport={transport} path={crawl.path} onNewCrawl={newCrawlFrom} />
      <CrawlFileDialogs file={file} />
    </div>
  );
}
