import { useEffect, useRef, useState } from "react";
import { Command as Cmdk } from "cmdk";
import { Dialog } from "@base-ui/react/dialog";
import { Search } from "lucide-react";
import { cn, Kbd } from "../components/ui";
import type { Command } from "./commands";

/** ⌘K / Ctrl+K palette: every destination and action, recent ones first (US4). */
export function CommandPalette({
  open,
  onOpenChange,
  commands,
  recent,
  onRun,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  commands: Command[];
  recent: string[];
  onRun: (c: Command) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  // Every opening starts empty: cleared as the palette closes, and again as it
  // opens in case it reopens during the closing animation.
  const [search, setSearch] = useState("");
  useEffect(() => {
    if (open) setSearch("");
  }, [open]);
  const setOpen = (o: boolean) => {
    if (!o) setSearch("");
    onOpenChange(o);
  };
  const available = commands.filter((c) => c.enabled !== false || c.reason);
  const recentCommands = recent.map((id) => available.find((c) => c.id === id)).filter((c): c is Command => !!c);
  const groups = ["Crawl", "Go to", "View", "Help"] as const;

  const item = (c: Command, prefix = "") => (
    <Cmdk.Item
      key={prefix + c.id}
      value={prefix + c.id + " " + c.title}
      keywords={c.keywords}
      disabled={c.enabled === false}
      onSelect={() => {
        setOpen(false);
        onRun(c);
      }}
      className="flex h-8 items-center gap-2 rounded-md px-2.5 text-base text-fg data-[disabled=true]:text-fg-3 data-[selected=true]:bg-selection"
    >
      <span className="truncate">{c.title}</span>
      <span className="min-w-0 flex-1 truncate text-sm text-fg-3">{c.enabled === false ? c.reason : null}</span>
      {c.shortcut && <Kbd keys={c.shortcut} className="border-current text-current opacity-70" />}
    </Cmdk.Item>
  );

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-40 bg-scrim transition-opacity duration-(--dur-fast) data-ending-style:opacity-0 data-starting-style:opacity-0" />
        {/* Focus the input through the dialog, not autoFocus: the dialog then
            knows what was focused before and returns focus there on close. */}
        <Dialog.Popup
          initialFocus={input}
          aria-label="Command palette"
          className={cn(
            "fixed top-20 left-1/2 z-40 w-140 sheet-max-w -translate-x-1/2 overflow-hidden rounded-xl bg-surface text-fg shadow-sheet outline-none",
            "transition-[opacity,scale] duration-(--dur-fast) data-ending-style:scale-98 data-ending-style:opacity-0 data-starting-style:scale-98 data-starting-style:opacity-0",
          )}
        >
          <Dialog.Title className="sr-only">Command palette</Dialog.Title>
          <Cmdk label="Commands" loop>
            <div className="flex items-center gap-2 border-b border-line px-3">
              <Search aria-hidden className="size-4 text-fg-3" />
              <Cmdk.Input ref={input} value={search} onValueChange={setSearch} placeholder="Type a command or a tab name" className="h-11 flex-1 bg-transparent text-md text-fg outline-none placeholder:text-fg-3" />
            </div>
            <Cmdk.List className="max-h-90 overflow-y-auto p-1.5">
              <Cmdk.Empty className="px-3 py-6 text-center text-sm text-fg-3">No matching command</Cmdk.Empty>
              {recentCommands.length > 0 && (
                <Cmdk.Group heading="Recent" className="[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-fg-3">
                  {recentCommands.map((c) => item(c, "recent:"))}
                </Cmdk.Group>
              )}
              {groups.map((g) => {
                const cs = available.filter((c) => c.group === g);
                return cs.length ? (
                  <Cmdk.Group key={g} heading={g} className="[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-fg-3">
                    {cs.map((c) => item(c))}
                  </Cmdk.Group>
                ) : null;
              })}
            </Cmdk.List>
          </Cmdk>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
