import { Menu as BaseMenu } from "@base-ui/react/menu";
import { Check, Columns3 } from "lucide-react";
import type { TabDef } from "../generated/TabDef";
import { Button, cn } from "../components/ui";
import { needReason } from "./format";

/** Show and hide a tab's columns (FR-014). Order follows the tab unless dragged. */
export function ColumnMenu({ tab, visible, available, onChange, onReset }: { tab: TabDef; visible: string[]; available: (need: TabDef["columns"][number]["need"]) => boolean; onChange: (ids: string[]) => void; onReset: () => void }) {
  const on = new Set(visible);
  const toggle = (id: string, show: boolean) => {
    if (show) {
      // Keep the tab's own order unless the user has dragged columns around.
      const rank = (x: string) => tab.columns.findIndex((c) => c.id === x);
      const inTabOrder = visible.every((v, i) => i === 0 || rank(visible[i - 1]) < rank(v));
      onChange(inTabOrder ? [...visible, id].sort((a, b) => rank(a) - rank(b)) : [...visible, id]);
    } else if (visible.length > 1) onChange(visible.filter((v) => v !== id));
  };
  return (
    <BaseMenu.Root>
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
            {tab.columns.map((c) => (
              <BaseMenu.CheckboxItem
                key={c.id}
                checked={on.has(c.id)}
                closeOnClick={false}
                onCheckedChange={(v) => toggle(c.id, v)}
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
          </BaseMenu.Popup>
        </BaseMenu.Positioner>
      </BaseMenu.Portal>
    </BaseMenu.Root>
  );
}
