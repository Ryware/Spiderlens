import { useRef } from "react";
import type { CrawlSetup } from "../crawl/useCrawlSetup";
import type { Transport } from "../transport";
import { Button, cn, Kbd, Sheet, SheetClose } from "../components/ui";
import { categories, CategoryBody, type CategoryId, issuesFor } from "./categories";
import { IssueList } from "./fields";
import { ProfilePicker } from "./ProfilePicker";
import { AppearanceSettings } from "./AppearanceSettings";
import type { Appearance, Level } from "../shell/appearance";

/**
 * Crawl settings as a sheet over the window (US2): profile and categories on
 * the left, the chosen category as grouped rows on the right. Behaviour and
 * validation are spec 002's, unchanged (FR-013).
 */
export function SettingsSheet({
  open,
  onOpenChange,
  setup,
  transport,
  category,
  onCategory,
  appearance,
  level,
  systemDark,
  onAppearance,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  setup: CrawlSetup;
  transport: Transport;
  category: CategoryId;
  onCategory: (c: CategoryId) => void;
  appearance: Appearance;
  level: Level;
  systemDark: boolean;
  onAppearance: (patch: Partial<Appearance>) => void;
}) {
  const navRef = useRef<HTMLButtonElement>(null);
  const mode = setup.config?.mode ?? "spider";
  const visible = categories.filter((c) => !c.listOnly || mode === "list");
  const groups = [visible.filter((c) => !c.app), visible.filter((c) => c.app)];
  const current = visible.find((c) => c.id === category) ?? visible.find((c) => c.id === "speed")!;
  // Issues on a whole category (for example "speed") rather than one setting.
  const general = setup.issues.filter((i) => current.prefixes.includes(i.path));

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Crawl settings" className="h-160 w-190 sheet-max-w" initialFocus={navRef}>
      <div className="flex min-h-0 flex-1">
        <nav aria-label="Settings categories" className="flex w-56 shrink-0 flex-col gap-3 border-r border-line bg-sidebar p-3">
          <div className="flex flex-col gap-1.5">
            <div className="px-1 text-xs font-semibold text-fg-3">Profile</div>
            <ProfilePicker transport={transport} setup={setup} />
          </div>
          <div role="tablist" aria-orientation="vertical" className="flex flex-col gap-3">
            {groups.map((group, g) => (
              <ul key={g} className="m-0 flex list-none flex-col gap-0.5 p-0" role="presentation">
                {g === 1 && <li role="presentation" className="px-2 pb-1 text-xs font-semibold text-fg-3">App</li>}
                {group.map((c) => {
                  const iss = issuesFor(setup, c);
                  const errors = iss.filter((i) => i.severity === "error").length;
                  const warnings = iss.length - errors;
                  const on = c.id === current.id;
                  return (
                    <li key={c.id} role="presentation">
                      <button
                        ref={on ? navRef : undefined}
                        type="button"
                        role="tab"
                        aria-selected={on}
                        onClick={() => onCategory(c.id)}
                        className={cn("flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-base", on ? "bg-selection text-fg" : "text-fg-2 hover:bg-surface-2")}
                      >
                        <span className="min-w-0 flex-1 truncate">{c.title}</span>
                        {errors > 0 && <span className="text-xs font-medium text-bad tabular">{errors === 1 ? "1 error" : `${errors} errors`}</span>}
                        {errors === 0 && warnings > 0 && (
                          <span className="text-xs font-medium text-warn tabular">{warnings === 1 ? "1 warning" : `${warnings} warnings`}</span>
                        )}
                      </button>
                    </li>
                  );
                })}
              </ul>
            ))}
          </div>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col" role="tabpanel" aria-label={current.title}>
          <header className="flex flex-col gap-0.5 px-6 pt-5 pb-3">
            <h2 className="m-0 text-md font-semibold">{current.title}</h2>
            <p className="m-0 text-sm text-fg-3">{current.description}</p>
          </header>
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-4">
            <IssueList issues={general} />
            {current.id === "appearance" ? (
              <AppearanceSettings appearance={appearance} level={level} systemDark={systemDark} onChange={onAppearance} />
            ) : (
              <CategoryBody id={current.id} setup={setup} transport={transport} />
            )}
          </div>
          <footer className="flex items-center gap-2 border-t border-line px-6 py-3">
            <span className="mr-auto text-sm text-fg-3">{current.app ? "Saved on this computer" : "Checked by the engine as you type"}</span>
            <SheetClose render={<Button variant="primary" />}>
              Done <Kbd keys="Escape" onAccent />
            </SheetClose>
          </footer>
        </div>
      </div>
    </Sheet>
  );
}
