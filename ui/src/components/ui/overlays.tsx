import type { ReactElement, ReactNode } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { Menu as BaseMenu } from "@base-ui/react/menu";
import { Tooltip as BaseTooltip } from "@base-ui/react/tooltip";
import { cn } from "./cn";

export const TooltipProvider = BaseTooltip.Provider;

/** Tooltip on hover and focus. `content` null renders the child alone. */
export function Tooltip({ content, children }: { content: ReactNode; children: ReactElement<Record<string, unknown>> }) {
  if (content == null || content === "") return children;
  return (
    <BaseTooltip.Root>
      <BaseTooltip.Trigger render={children} />
      <BaseTooltip.Portal>
        <BaseTooltip.Positioner sideOffset={6} className="z-50">
          <BaseTooltip.Popup
            className={cn(
              "max-w-80 rounded-md bg-fg px-2 py-1 text-sm text-bg shadow-popover",
              "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)",
              "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
            )}
          >
            {content}
          </BaseTooltip.Popup>
        </BaseTooltip.Positioner>
      </BaseTooltip.Portal>
    </BaseTooltip.Root>
  );
}

/** Modal sheet over the window (FR-009). Escape and the scrim close it. */
export function Sheet({
  open,
  onOpenChange,
  title,
  children,
  className,
  initialFocus,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
  className?: string;
  initialFocus?: React.RefObject<HTMLElement | null>;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(o) => onOpenChange(o)}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur) data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup
          initialFocus={initialFocus}
          className={cn(
            "fixed top-12 left-1/2 z-40 flex sheet-max-h -translate-x-1/2 flex-col overflow-hidden",
            "rounded-xl bg-surface text-fg shadow-sheet outline-none",
            "transition-[opacity,translate] duration-(--dur-slow) ease-ui",
            "data-ending-style:-translate-y-3 data-ending-style:opacity-0 data-starting-style:-translate-y-3 data-starting-style:opacity-0",
            className,
          )}
        >
          <Dialog.Title className="sr-only">{title}</Dialog.Title>
          {children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export const SheetClose = Dialog.Close;

export interface MenuEntry {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
}

/** Action menu. `null` entries draw separators. */
export function Menu({ trigger, items, label }: { trigger: ReactElement<Record<string, unknown>>; items: (MenuEntry | null)[]; label: string }) {
  return (
    <BaseMenu.Root>
      <BaseMenu.Trigger render={trigger} aria-label={label} />
      <BaseMenu.Portal>
        <BaseMenu.Positioner sideOffset={4} align="start" className="z-50">
          <BaseMenu.Popup
            className={cn(
              "min-w-48 rounded-lg bg-surface p-1 text-base text-fg shadow-popover outline-none",
              "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-fast)",
              "data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
            )}
          >
            {items.map((it, i) =>
              it === null ? (
                <BaseMenu.Separator key={`sep-${i}`} className="mx-1 my-1 h-px bg-line" />
              ) : (
                <BaseMenu.Item
                  key={it.label}
                  disabled={it.disabled}
                  onClick={it.onSelect}
                  className={cn(
                    "flex h-7 items-center rounded-sm px-2 outline-none select-none",
                    "data-highlighted:bg-accent data-highlighted:text-accent-fg data-disabled:opacity-45",
                    it.destructive && "text-bad",
                  )}
                >
                  {it.label}
                </BaseMenu.Item>
              ),
            )}
          </BaseMenu.Popup>
        </BaseMenu.Positioner>
      </BaseMenu.Portal>
    </BaseMenu.Root>
  );
}

/** Small modal for one question or one input (profile names, About). */
export function Dialog_({
  open,
  onOpenChange,
  title,
  description,
  children,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(o) => onOpenChange(o)}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur) data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup
          className={cn(
            "fixed top-1/3 left-1/2 z-40 flex w-96 -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-xl bg-surface p-5 text-fg shadow-sheet outline-none",
            "transition-[opacity,scale] duration-(--dur) data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
          )}
        >
          <div className="flex flex-col gap-1">
            <Dialog.Title className="m-0 text-md font-semibold">{title}</Dialog.Title>
            {description && <Dialog.Description className="m-0 text-sm text-fg-2">{description}</Dialog.Description>}
          </div>
          {children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export { Dialog_ as Modal };
