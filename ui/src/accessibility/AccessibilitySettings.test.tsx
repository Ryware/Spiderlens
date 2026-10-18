// Spec 026: the Accessibility settings page writes every field, and warns when
// Full checks are chosen but rendering is off.
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import type { CrawlConfig } from "../generated/CrawlConfig";
import { defaultConfig } from "../testing/fakeTransport";
import { AccessibilitySettings } from "./AccessibilitySettings";

function Harness({ initial, onOpenRendering, seen }: { initial: CrawlConfig; onOpenRendering?: () => void; seen: { config: CrawlConfig } }) {
  const [config, setConfig] = useState(initial);
  seen.config = config;
  return <AccessibilitySettings config={config} set={(f) => setConfig(f)} at={() => []} onOpenRendering={onOpenRendering} />;
}

const mount = (over: (c: CrawlConfig) => CrawlConfig = (c) => c, onOpenRendering?: () => void) => {
  const seen = { config: over(defaultConfig()) };
  render(<Harness initial={seen.config} onOpenRendering={onOpenRendering} seen={seen} />);
  return seen;
};

test("level_buttons_write_the_level", () => {
  const seen = mount();
  fireEvent.click(screen.getByRole("radio", { name: "Off" }));
  expect(seen.config.accessibility.level).toBe("off");
  fireEvent.click(screen.getByRole("radio", { name: "Full checks" }));
  expect(seen.config.accessibility.level).toBe("full");
  fireEvent.click(screen.getByRole("radio", { name: "Static checks" }));
  expect(seen.config.accessibility.level).toBe("static");
});

test("wcag_version_and_level_write_the_target", () => {
  const seen = mount();
  fireEvent.change(screen.getByLabelText("WCAG version"), { target: { value: "2.1" } });
  expect(seen.config.accessibility.wcag).toEqual({ version: "2.1", level: "aa" });
  fireEvent.change(screen.getByLabelText("WCAG level"), { target: { value: "aaa" } });
  expect(seen.config.accessibility.wcag).toEqual({ version: "2.1", level: "aaa" });
});

test("switches_write_best_practice_and_mobile", () => {
  const seen = mount();
  fireEvent.click(screen.getByRole("switch", { name: "Best practice" }));
  expect(seen.config.accessibility.best_practice).toBe(false);
  fireEvent.click(screen.getByRole("switch", { name: "Mobile checks" }));
  expect(seen.config.accessibility.mobile).toBe(false);
});

test("number_fields_write_whole_numbers", () => {
  const seen = mount();
  const cases: [string, keyof CrawlConfig["accessibility"], string, number][] = [
    ["Small text", "small_text_px", "14", 14],
    ["Small text share", "small_text_share", "50", 50],
    ["Tap target size", "tap_target_px", "44", 44],
    ["Overflow allowance", "overflow_px", "10", 10],
    ["Time per page", "budget_secs", "12", 12],
  ];
  for (const [label, key, text, value] of cases) {
    fireEvent.change(screen.getByLabelText(label), { target: { value: text } });
    expect(seen.config.accessibility[key]).toBe(value);
  }
});

test("full_without_rendering_shows_a_notice_whose_button_switches_category", () => {
  const open = vi.fn();
  mount((c) => ({ ...c, accessibility: { ...c.accessibility, level: "full" }, rendering: { ...c.rendering, mode: "raw" } }), open);
  expect(screen.getByText(/Full checks need JavaScript rendering/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Open Rendering settings" }));
  expect(open).toHaveBeenCalledTimes(1);
});

test("no_notice_for_static", () => {
  mount();
  expect(screen.queryByText(/Full checks need JavaScript rendering/)).toBeNull();
});

test("no_notice_when_full_and_rendering_is_on", () => {
  mount((c) => ({ ...c, accessibility: { ...c.accessibility, level: "full" }, rendering: { ...c.rendering, mode: "javascript" } }));
  expect(screen.queryByText(/Full checks need JavaScript rendering/)).toBeNull();
});
