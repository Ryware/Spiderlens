import { forwardRef } from "react";
import { Command as CommandIcon, ListPlus, Pause, Play, RotateCcw, SlidersHorizontal, Square, TriangleAlert } from "lucide-react";
import type { CrawlRun } from "../crawl/useCrawlRun";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { RetryAvailability } from "../crawl/RetryFailed";
import { Button, cn, Input, Kbd, Segmented, Tooltip } from "../components/ui";
import { platform } from "./platform";
import type { Appearance, Level } from "./appearance";
import { BrightnessPopover } from "./BrightnessPopover";

/**
 * The crawl lives here (FR-002): mode, start URL or list, Start, settings and
 * the palette. Exactly one primary control: Start, or Pause while crawling,
 * or Resume while paused. Mode and URL are locked while a crawl is open.
 * "Retry failed URLs" sits next to them once a crawl file is open.
 */
export const Toolbar = forwardRef<HTMLInputElement, {
  setup: CrawlSetup;
  run: CrawlRun;
  onStart: () => void;
  /** Resume goes through the crawl file checks (upgrade, secrets, read-only). */
  onResume: () => void;
  onOpenSettings: (category?: "list" | "speed") => void;
  onOpenPalette: () => void;
  appearance: Appearance;
  level: Level;
  systemDark: boolean;
  onAppearance: (patch: Partial<Appearance>) => void;
  retry: RetryAvailability;
  onRetry: () => void;
}>(function Toolbar({ setup, run, onStart, onResume, onOpenSettings, onOpenPalette, appearance, level, systemDark, onAppearance, retry, onRetry }, urlRef) {
  const { config, list } = setup;
  const locked = run.active;
  const mode = config?.mode ?? "spider";
  const firstError = setup.errors[0]?.message;
  const urlError = setup.errors.find((e) => e.path === "input.start_url");
  const warnings = setup.warnings.length;

  return (
    <header
      data-tauri-drag-region
      data-region="chrome"
      data-material-region
      className={cn("flex h-toolbar shrink-0 items-center gap-2 border-b border-line bg-chrome px-3", platform === "mac" && "pl-mac-inset")}
    >
      <Segmented
        label="Crawl mode"
        value={mode}
        options={[
          { value: "spider", label: "Spider", disabled: locked },
          { value: "list", label: "List", disabled: locked },
        ]}
        onChange={(m) => setup.set((c) => ({ ...c, mode: m }))}
      />
      {mode === "spider" ? (
        <Input
          ref={urlRef}
          aria-label="Start URL"
          aria-invalid={setup.startUrl !== "" && !!urlError}
          // A native title, not <Tooltip>: wrapping the field when an error
          // appears would remount it and drop focus while typing.
          title={setup.startUrl !== "" ? urlError?.message : undefined}
          placeholder="Enter a website, for example example.com"
          mono
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          className="h-control min-w-0 flex-1 bg-surface-2"
          readOnly={locked}
          value={setup.startUrl}
          onChange={(e) => setup.setStartUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !locked && onStart()}
        />
      ) : (
        <button
          type="button"
          disabled={locked}
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
      {run.path && (
        <Tooltip content={retry.available ? null : retry.reason}>
          <span className="inline-flex">
            <Button variant="quiet" disabled={!retry.available} onClick={onRetry} aria-label="Retry failed URLs">
              <RotateCcw aria-hidden className="size-3.5" /> Retry failed
              {retry.available && <span className="text-fg-3 tabular">{retry.total.toLocaleString()}</span>}
            </Button>
          </span>
        </Tooltip>
      )}
      <CrawlControls setup={setup} run={run} onStart={onStart} onResume={onResume} firstError={firstError} />
      <Button onClick={() => onOpenSettings()} aria-label="Configure">
        <SlidersHorizontal aria-hidden className="size-3.5" /> Configure <Kbd keys="Mod+," />
      </Button>
      <BrightnessPopover appearance={appearance} level={level} systemDark={systemDark} onChange={onAppearance} />
      <Tooltip content="Commands">
        <Button variant="quiet" icon onClick={onOpenPalette} aria-label="Open command palette">
          <CommandIcon className="size-4" />
        </Button>
      </Tooltip>
    </header>
  );
});

function CrawlControls({ setup, run, onStart, onResume, firstError }: { setup: CrawlSetup; run: CrawlRun; onStart: () => void; onResume: () => void; firstError?: string }) {
  const { phase } = run;
  if (phase === "running" || phase === "starting" || phase === "pausing" || phase === "stopping") {
    const settling = phase !== "running";
    return (
      <>
        <Button variant="primary" disabled={settling} onClick={run.pause}>
          <Pause aria-hidden className="size-3.5 fill-current" /> {phase === "pausing" ? "Pausing…" : "Pause"}
        </Button>
        <Button disabled={settling} onClick={run.stop}>
          <Square aria-hidden className="size-3 fill-current" /> {phase === "stopping" ? "Stopping…" : "Stop"}
        </Button>
      </>
    );
  }
  if (phase === "paused") {
    return (
      <>
        <Button variant="primary" onClick={onResume}>
          <Play aria-hidden className="size-3.5 fill-current" /> Resume
        </Button>
        <Button onClick={run.stop}>
          <Square aria-hidden className="size-3 fill-current" /> Stop
        </Button>
      </>
    );
  }
  return (
    <Tooltip content={!setup.canStart && firstError ? firstError : null}>
      <span className="inline-flex">
        <Button variant="primary" disabled={!setup.canStart} onClick={onStart}>
          <Play aria-hidden className="size-3.5 fill-current" /> Start
        </Button>
      </span>
    </Tooltip>
  );
}
