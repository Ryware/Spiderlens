import { forwardRef } from "react";
import { Command as CommandIcon, ListPlus, Play, SlidersHorizontal, TriangleAlert } from "lucide-react";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import { Button, cn, Input, Kbd, Segmented, Tooltip } from "../components/ui";
import { platform } from "./platform";

/**
 * The crawl lives here (FR-002): mode, start URL or list, Start, settings and
 * the palette. Exactly one primary control: Start.
 */
export const Toolbar = forwardRef<HTMLInputElement, {
  setup: CrawlSetup;
  onOpenSettings: (category?: "list" | "speed") => void;
  onOpenPalette: () => void;
}>(function Toolbar({ setup, onOpenSettings, onOpenPalette }, urlRef) {
  const { config, list } = setup;
  const mode = config?.mode ?? "spider";
  const firstError = setup.errors[0]?.message;
  const urlError = setup.errors.find((e) => e.path === "input.start_url");
  const warnings = setup.warnings.length;

  return (
    <header
      data-tauri-drag-region
      className={cn("flex h-toolbar shrink-0 items-center gap-2 border-b border-line bg-chrome px-3", platform === "mac" && "pl-mac-inset")}
    >
      <Segmented
        label="Crawl mode"
        value={mode}
        options={[
          { value: "spider", label: "Spider" },
          { value: "list", label: "List" },
        ]}
        onChange={(m) => setup.set((c) => ({ ...c, mode: m }))}
      />
      {mode === "spider" ? (
        <Input
          ref={urlRef}
          aria-label="Start URL"
          aria-invalid={setup.startUrl !== "" && !!urlError}
          placeholder="Enter a website, for example example.com"
          mono
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          className="h-control min-w-0 flex-1 bg-surface-2"
          value={setup.startUrl}
          onChange={(e) => setup.setStartUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && setup.start()}
        />
      ) : (
        <button
          type="button"
          onClick={() => onOpenSettings("list")}
          className="flex h-control min-w-0 flex-1 items-center gap-2 rounded-md bg-surface-2 px-2 text-left text-base text-fg-2 hover:bg-surface-3"
        >
          <ListPlus aria-hidden className="size-4 text-fg-3" />
          <span className="truncate tabular">{list ? `${list.urls.toLocaleString()} URLs loaded` : "Add a list of URLs…"}</span>
        </button>
      )}
      {warnings > 0 && (
        <Tooltip content={setup.warnings.map((w) => w.message).join(" ")}>
          <Button variant="quiet" onClick={() => onOpenSettings()} aria-label={`${warnings} less respectful settings`} className="text-warn">
            <TriangleAlert className="size-4" />
            <span className="tabular">{warnings}</span>
          </Button>
        </Tooltip>
      )}
      <Tooltip content={!setup.canStart && firstError ? firstError : null}>
        <span className="inline-flex">
          <Button variant="primary" disabled={!setup.canStart} onClick={setup.start}>
            <Play aria-hidden className="size-3.5 fill-current" /> Start
          </Button>
        </span>
      </Tooltip>
      <Button onClick={() => onOpenSettings()} aria-label="Configure">
        <SlidersHorizontal aria-hidden className="size-3.5" /> Configure <Kbd keys="Mod+," />
      </Button>
      <Tooltip content="Commands">
        <Button variant="quiet" icon onClick={onOpenPalette} aria-label="Open command palette">
          <CommandIcon className="size-4" />
        </Button>
      </Tooltip>
    </header>
  );
});
