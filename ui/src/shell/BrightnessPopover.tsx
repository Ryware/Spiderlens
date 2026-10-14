import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { Popover } from "@base-ui/react/popover";
import { Button, cn, Switch, Tooltip } from "../components/ui";
import { ACCENTS, type Appearance, chooseLevel, type Level, LEVELS, levelName } from "./appearance";
import { LevelIcon } from "./levelIcons";

/**
 * The brightness control from the toolbar (spec 018 US1): a vertical slider
 * over the six steps. It re-themes on every step it crosses, while dragging
 * (FR-007), and also takes clicks on the names and the arrow keys.
 */
export function BrightnessPopover({
  appearance,
  level,
  systemDark,
  onChange,
}: {
  appearance: Appearance;
  level: Level;
  systemDark: boolean;
  onChange: (patch: Partial<Appearance>) => void;
}) {
  const track = useRef<HTMLDivElement>(null);
  const index = LEVELS.findIndex((l) => l.id === level);
  const pick = (i: number) => {
    const next = LEVELS[Math.max(0, Math.min(LEVELS.length - 1, i))].id;
    if (next !== level) onChange(chooseLevel(appearance, next, systemDark));
  };
  const fromPointer = (e: PointerEvent) => {
    const r = track.current?.getBoundingClientRect();
    if (!r || r.height === 0) return;
    pick(Math.floor(((e.clientY - r.top) / r.height) * LEVELS.length));
  };
  const onKey = (e: KeyboardEvent) => {
    const moves: Record<string, number> = { ArrowDown: index + 1, ArrowRight: index + 1, ArrowUp: index - 1, ArrowLeft: index - 1, Home: 0, End: LEVELS.length - 1 };
    if (e.key in moves) {
      e.preventDefault();
      pick(moves[e.key]);
    }
  };

  return (
    <Popover.Root>
      <Tooltip content="Brightness">
        <Popover.Trigger render={<Button variant="quiet" icon aria-label={`Brightness: ${levelName(level)}`} />}>
          <LevelIcon level={level} />
        </Popover.Trigger>
      </Tooltip>
      <Popover.Portal>
        <Popover.Positioner sideOffset={6} align="end" className="z-50">
          <Popover.Popup
            initialFocus={track}
            // Island: the popover belongs to the dark toolbar (US4-2).
            data-region="chrome"
            className={cn(
              "flex w-56 flex-col gap-2 rounded-lg bg-surface p-3 text-fg shadow-popover outline-none",
              "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)",
              "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
            )}
          >
            <Popover.Title className="sr-only">Brightness</Popover.Title>
            <div className="flex gap-2">
              <div
                ref={track}
                role="slider"
                tabIndex={0}
                aria-label="Brightness"
                aria-orientation="vertical"
                aria-valuemin={0}
                aria-valuemax={LEVELS.length - 1}
                aria-valuenow={index}
                aria-valuetext={levelName(level)}
                onKeyDown={onKey}
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture?.(e.pointerId);
                  fromPointer(e);
                }}
                onPointerMove={(e) => e.currentTarget.hasPointerCapture?.(e.pointerId) && fromPointer(e)}
                className="relative w-5 shrink-0 cursor-pointer touch-none rounded-full"
              >
                <span aria-hidden className="absolute inset-y-3 left-2 w-1 rounded-full bg-ladder ring-1 ring-line-strong" />
                <span
                  aria-hidden
                  className="absolute left-0.5 size-4 rounded-full border-2 border-fg bg-surface shadow-raised transition-[top] duration-(--dur-fast)"
                  style={{ top: `calc(${index} * var(--control) + (var(--control) - 4 * var(--unit)) / 2)` }}
                />
              </div>
              <ul className="m-0 flex min-w-0 flex-1 list-none flex-col p-0">
                {LEVELS.map((l, i) => (
                  <li key={l.id}>
                    <button
                      type="button"
                      tabIndex={-1}
                      aria-pressed={i === index}
                      onClick={() => pick(i)}
                      className={cn(
                        "flex h-control w-full items-center gap-2 rounded-md px-2 text-left text-base",
                        i === index ? "font-semibold text-fg" : "text-fg-3 hover:bg-surface-2 hover:text-fg-2",
                      )}
                    >
                      <LevelIcon level={l.id} />
                      {l.name}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
            <div className="flex items-center justify-between border-t border-line pt-2 text-sm text-fg-2">
              <span id="follow-system-label">Follow system</span>
              <Switch labelledBy="follow-system-label" checked={appearance.followSystem} onChange={(v) => onChange({ followSystem: v, level })} />
            </div>
            <div className="flex flex-wrap gap-1.5 border-t border-line pt-2" role="radiogroup" aria-label="Accent">
              {ACCENTS.map((a) => (
                <AccentSwatch key={a.id} id={a.id} name={a.name} on={appearance.accent === a.id} onPick={() => onChange({ accent: a.id })} />
              ))}
            </div>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

/**
 * A round swatch drawn in the palette's own accent: the swatch element sets
 * data-accent, so the token file gives it that palette's colour in the
 * current tone.
 */
export function AccentSwatch({ id, name, on, onPick, withName }: { id: string; name: string; on: boolean; onPick: () => void; withName?: boolean }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={on}
      aria-label={withName ? undefined : name}
      title={withName ? undefined : name}
      onClick={onPick}
      className={cn(
        "flex items-center gap-2 rounded-md text-base",
        withName ? cn("h-control border px-2 text-left", on ? "border-accent text-fg ring-1 ring-accent" : "border-line text-fg-2 hover:bg-surface-2") : "p-0.5",
      )}
    >
      <span data-swatch={id} className={cn("block size-5 rounded-full bg-swatch", !withName && on && "ring-2 ring-fg ring-offset-2 ring-offset-surface")} />
      {withName && name}
    </button>
  );
}
