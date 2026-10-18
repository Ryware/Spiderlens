import type { CrawlConfig } from "../generated/CrawlConfig";
import type { A11yLevel } from "../generated/A11yLevel";
import type { AccessibilitySettings as Settings } from "../generated/AccessibilitySettings";
import type { WcagLevel } from "../generated/WcagLevel";
import type { WcagVersion } from "../generated/WcagVersion";
import type { Issue } from "../generated/Issue";
import { Button, Notice, NumberInput, Segmented, Select, Switch } from "../components/ui";
import { Group, Row } from "../settings/fields";

type NumberKey = "budget_secs" | "small_text_px" | "small_text_share" | "tap_target_px" | "overflow_px";

/** Settings > Accessibility (spec 026). */
export function AccessibilitySettings({
  config,
  set,
  at,
  onOpenRendering,
}: {
  config: CrawlConfig;
  set: (f: (c: CrawlConfig) => CrawlConfig) => void;
  at: (path: string) => Issue[];
  /** Opens the Rendering settings category. */
  onOpenRendering?: () => void;
}) {
  const a = config.accessibility;
  const patch = (p: Partial<Settings>) => set((c) => ({ ...c, accessibility: { ...c.accessibility, ...p } }));
  const num = (key: NumberKey, label: string, help: string, unit: string) => (
    <Row label={label} help={help} issues={at(`accessibility.${key}`)}>
      {(i) => (
        <span className="flex items-center gap-2">
          <NumberInput id={i} value={a[key]} onChange={(v) => patch({ [key]: v === null || Number.isNaN(v) ? a[key] : Math.round(v) })} />
          <span className="text-xs text-fg-3">{unit}</span>
        </span>
      )}
    </Row>
  );
  return (
    <>
      <Group>
        <Row label="Accessibility checks" help="Static checks read the page's HTML and add no requests. Full checks run an accessibility engine in the rendered page." issues={at("accessibility.level")}>
          {() => (
            <Segmented<A11yLevel>
              label="Accessibility checks"
              value={a.level}
              options={[
                { value: "off", label: "Off" },
                { value: "static", label: "Static checks" },
                { value: "full", label: "Full checks" },
              ]}
              onChange={(level) => patch({ level })}
            />
          )}
        </Row>
        {a.level === "full" && config.rendering.mode !== "javascript" && (
          <Notice kind="warn">
            <span className="flex flex-wrap items-center gap-2">
              Full checks need JavaScript rendering. Static checks run until rendering is on.
              {onOpenRendering && <Button onClick={onOpenRendering}>Open Rendering settings</Button>}
            </span>
          </Notice>
        )}
        <Row label="WCAG version" help="The standard checks are measured against." issues={at("accessibility.wcag")}>
          {(i) => (
            <Select id={i} className="w-32" value={a.wcag.version} onChange={(e) => patch({ wcag: { ...a.wcag, version: e.target.value as WcagVersion } })}>
              <option value="2.0">WCAG 2.0</option>
              <option value="2.1">WCAG 2.1</option>
              <option value="2.2">WCAG 2.2</option>
            </Select>
          )}
        </Row>
        <Row label="WCAG level" help="Level AA is what most laws and policies ask for." issues={at("accessibility.wcag")}>
          {(i) => (
            <Select id={i} className="w-32" value={a.wcag.level} onChange={(e) => patch({ wcag: { ...a.wcag, level: e.target.value as WcagLevel } })}>
              <option value="a">A</option>
              <option value="aa">AA</option>
              <option value="aaa">AAA</option>
            </Select>
          )}
        </Row>
        <Row label="Best practice" help="Also report advice that is not part of WCAG.">
          {(_, l) => <Switch labelledBy={l} checked={a.best_practice} onChange={(best_practice) => patch({ best_practice })} />}
        </Row>
      </Group>
      <Group title="Mobile">
        <Row label="Mobile checks" help="Checks the viewport tag, text size and tap targets. Results are in the Mobile tab.">
          {(_, l) => <Switch labelledBy={l} checked={a.mobile} onChange={(mobile) => patch({ mobile })} />}
        </Row>
        {num("small_text_px", "Small text", "Text smaller than this counts as small.", "px")}
        {num("small_text_share", "Small text share", "A page is flagged when this share of its text is small.", "%")}
        {num("tap_target_px", "Tap target size", "Links and buttons smaller than this are hard to tap.", "px")}
        {num("overflow_px", "Overflow allowance", "Content wider than the window by more than this counts as overflow.", "px")}
      </Group>
      <Group title="Time">
        {num("budget_secs", "Time per page", "Full checks stop after this long on one page (1 to 30 seconds).", "seconds")}
      </Group>
    </>
  );
}
