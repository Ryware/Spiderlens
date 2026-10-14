import { Menu as BaseMenu } from "@base-ui/react/menu";
import { Check, Columns3 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { TabDef } from "../generated/TabDef";
import { Button, cn } from "../components/ui";
import { formatShortcut } from "../shell/platform";
import { needReason } from "./format";

/**
 * Show, hide and reorder a tab's columns (FR-014). Order follows the tab
 * unless dragged or moved here with Alt+Up/Down. Shown columns are listed
 * in their order on screen, hidden ones after them.
 */
export function ColumnMenu({ tab, visible, available, onChange, onReset }: { tab: TabDef; visible: string[]; available: (need: TabDef["columns"][number]["need"]) => boolean; onChange: (ids: string[]) => void; onReset: () => void }) {
  const on = new Set(visible);
  const [said, setSaid] = useState("");
  const items = useRef(new Map<string, HTMLElement>());
  const moved = useRef<string | null>(null);
  const listed = [...visible.flatMap((id) => tab.columns.filter((c) => c.id === id)), ...tab.columns.filter((c) => !on.has(c.id))];

  // The moved item keeps focus; telling the menu it has focus again makes
  // the menu's arrow keys continue from its new place.
  useEffect(() => {
    const el = moved.current && items.current.get(moved.current);
    moved.current = null;
    if (el) el.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
  }, [visible]);

  const move = (id: string, by: -1 | 1) => {
    const from = visible.indexOf(id);
    const to = from + by;
    const label = tab.columns.find((c) => c.id === id)?.label ?? id;
    if (to < 0 || to >= visible.length) {
      setSaid(`${label} is already ${by < 0 ? "first" : "last"}`);
      return;
    }
    const order = [...visible];
    order.splice(from, 1);
    order.splice(to, 0, id);
    moved.current = id;
    onChange(order);
    setSaid(`${label} moved to position ${to + 1} of ${order.length}`);
  };
  const toggle = (id: string, show: boolean) => {
    moved.current = id;
    if (show) {
      // Keep the tab's own order unless the user has dragged columns around.
      const rank = (x: string) => tab.columns.findIndex((c) => c.id === x);
      const inTabOrder = visible.every((v, i) => i === 0 || rank(visible[i - 1]) < rank(v));
      onChange(inTabOrder ? [...visible, id].sort((a, b) => rank(a) - rank(b)) : [...visible, id]);
    } else if (visible.length > 1) onChange(visible.filter((v) => v !== id));
  };
  return (
    <>
      <BaseMenu.Root onOpenChange={() => setSaid("")}>
        <BaseMenu.Trigger
          render={
            <Button size="sm" variant="quiet" aria-label="Columns">
              <Columns3 aria-hidden className="size-3.5" />
              Columns
            </Button>
          }
        />
        <BaseMenu.Portal>
          <BaseMenu.Positioner sideOffset={4} align="end" className="z-50">
            <BaseMenu.Popup className="max-h-96 min-w-56 overflow-y-auto rounded-lg bg-surface p-1 text-base text-fg shadow-popover outline-none">
              {listed.map((c) => (
                <BaseMenu.CheckboxItem
                  key={c.id}
                  ref={(el) => void (el ? items.current.set(c.id, el) : items.current.delete(c.id))}
                  checked={on.has(c.id)}
                  closeOnClick={false}
                  onCheckedChange={(v) => toggle(c.id, v)}
                  aria-keyshortcuts={on.has(c.id) ? "Alt+ArrowUp Alt+ArrowDown" : undefined}
                  onKeyDown={(e) => {
                    if (!e.altKey || e.ctrlKey || e.metaKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown") || !on.has(c.id)) return;
                    // Not the menu's own arrow keys.
                    e.preventDefault();
                    e.stopPropagation();
                    move(c.id, e.key === "ArrowUp" ? -1 : 1);
                  }}
                  className="flex h-7 items-center gap-2 rounded-sm px-2 outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-fg"
                >
                  <span className="flex size-3.5 items-center justify-center">
                    <BaseMenu.CheckboxItemIndicator>
                      <Check aria-hidden className="size-3.5" />
                    </BaseMenu.CheckboxItemIndicator>
                  </span>
                  <span className="flex-1">{c.label}</span>
                  {!available(c.need) && (
                    <span title={needReason[c.need]} className={cn("text-xs opacity-70")}>
                      Not collected
                    </span>
                  )}
                </BaseMenu.CheckboxItem>
              ))}
              <BaseMenu.Separator className="mx-1 my-1 h-px bg-line" />
              <BaseMenu.Item onClick={onReset} className="flex h-7 items-center rounded-sm px-2 pl-7.5 outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-fg">
                Reset columns
              </BaseMenu.Item>
              <div aria-hidden className="px-2 pt-1 pb-0.5 pl-7.5 text-xs text-fg-3">
                {formatShortcut("Alt+↑")} {formatShortcut("Alt+↓")} moves a column
              </div>
            </BaseMenu.Popup>
          </BaseMenu.Positioner>
        </BaseMenu.Portal>
      </BaseMenu.Root>
      <span role="status" className="sr-only">
        {said}
      </span>
    </>
  );
}
