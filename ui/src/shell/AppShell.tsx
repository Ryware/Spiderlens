import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Transport } from "../transport";
import { useCrawlSetup } from "../crawl/useCrawlSetup";
import { SettingsSheet } from "../settings/SettingsSheet";
import type { CategoryId } from "../settings/categories";
import { cn } from "../components/ui";
import { Toolbar } from "./Toolbar";
import { resultTabs, Sidebar } from "./Sidebar";
import { AboutDialog, DetailPanel, StartState, StatusBar } from "./panels";
import { CommandPalette } from "./CommandPalette";
import { shortcutCommand, type Command } from "./commands";
import { matches, platform } from "./platform";
import { useTheme, useUiState } from "./useUiState";

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
  const [ui, updateUi] = useUiState();
  useTheme(ui.theme);
  const width = useWindowWidth();
  const [section, setSection] = useState("overview");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const urlRef = useRef<HTMLInputElement>(null);

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

  const commands: Command[] = useMemo(
    () => [
      { id: "start", title: "Start crawl", group: "Crawl", shortcut: "Mod+Enter", enabled: setup.canStart, run: setup.start },
      { id: "settings", title: "Crawl settings", group: "Crawl", shortcut: "Mod+,", keywords: ["configure", "preferences"], run: () => openSettings() },
      { id: "focus-url", title: "Edit start URL", group: "Crawl", shortcut: "Mod+L", run: () => urlRef.current?.focus() },
      { id: "mode-spider", title: "Switch to Spider mode", group: "Crawl", run: () => setup.set((c) => ({ ...c, mode: "spider" })) },
      { id: "mode-list", title: "Switch to List mode", group: "Crawl", run: () => setup.set((c) => ({ ...c, mode: "list" })) },
      { id: "speed-polite", title: "Speed: Polite", group: "Crawl", run: () => setup.presets && setup.set((c) => ({ ...c, speed: setup.presets!.polite })) },
      { id: "speed-fast", title: "Speed: Fast (sites you control)", group: "Crawl", run: () => setup.presets && setup.set((c) => ({ ...c, speed: setup.presets!.fast })) },
      ...["limits", "scope", "identity", "speed", "robots", "network"].map((id) => ({
        id: `settings-${id}`,
        title: `Settings: ${id === "robots" ? "robots.txt" : id === "network" ? "Network and retries" : id[0].toUpperCase() + id.slice(1)}`,
        group: "Go to" as const,
        run: () => openSettings(id as CategoryId),
      })),
      { id: "go-overview", title: "Overview", group: "Go to", run: () => setSection("overview") },
      ...resultTabs.map((t) => ({ id: `go-${t.id}`, title: t.title, group: "Go to" as const, enabled: false, run: () => setSection(t.id) })),
      { id: "toggle-sidebar", title: ui.sidebarCollapsed ? "Show sidebar" : "Collapse sidebar", group: "View", shortcut: "Mod+Alt+S", run: () => updateUi({ sidebarCollapsed: !ui.sidebarCollapsed }) },
      { id: "toggle-detail", title: ui.detailCollapsed ? "Show details" : "Hide details", group: "View", shortcut: "Mod+Alt+I", run: () => updateUi({ detailCollapsed: !ui.detailCollapsed }) },
      { id: "theme-system", title: "Appearance: System", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "system" }) },
      { id: "theme-light", title: "Appearance: Light", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "light" }) },
      { id: "theme-dark", title: "Appearance: Dark", group: "View", keywords: ["theme"], run: () => updateUi({ theme: "dark" }) },
      { id: "about", title: "About SpiderLens", group: "Help", keywords: ["version", "build"], run: () => setAboutOpen(true) },
    ],
    [setup, openSettings, ui.sidebarCollapsed, ui.detailCollapsed, updateUi],
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
      if (settingsOpen || paletteOpen || aboutOpen) return;
      const c = shortcutCommand(commands, e, matches);
      if (c) {
        e.preventDefault();
        c.run();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [commands, settingsOpen, paletteOpen, aboutOpen]);

  return (
    <div className="flex h-full flex-col bg-bg text-fg">
      <Toolbar ref={urlRef} setup={setup} onOpenSettings={openSettings} onOpenPalette={() => setPaletteOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <div className={cn("shrink-0 border-r border-line", sidebarIcons && "border-r")} style={{ width: sidebarIcons ? SIDEBAR.icons : ui.sidebarWidth }}>
          <Sidebar active={section} onSelect={setSection} collapsed={sidebarIcons} onAbout={() => setAboutOpen(true)} />
        </div>
        {!sidebarIcons && <Splitter label="Resize sidebar" value={ui.sidebarWidth} min={SIDEBAR.min} max={SIDEBAR.max} onChange={(v) => updateUi({ sidebarWidth: v })} />}
        <main className="min-w-0 flex-1 overflow-hidden bg-bg">
          <StartState setup={setup} onFocusUrl={() => urlRef.current?.focus()} onOpenSettings={() => openSettings()} />
        </main>
        {!detailHidden && (
          <>
            <Splitter label="Resize details" value={ui.detailWidth} min={DETAIL.min} max={DETAIL.max} invert onChange={(v) => updateUi({ detailWidth: v })} />
            <div className="shrink-0 border-l border-line" style={{ width: ui.detailWidth }}>
              <DetailPanel />
            </div>
          </>
        )}
      </div>
      <StatusBar setup={setup} />

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
    </div>
  );
}
