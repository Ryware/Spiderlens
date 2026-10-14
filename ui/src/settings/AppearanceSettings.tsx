import { ACCENTS, type Appearance, chooseLevel, type Level, LEVELS, toneOf } from "../shell/appearance";
import { materialAvailable } from "../shell/platform";
import { AccentSwatch } from "../shell/BrightnessPopover";
import { cn, Select, Switch } from "../components/ui";
import { Group, Row } from "./fields";

/**
 * Settings › Appearance (spec 018 US2, FR-012): the six steps as previews,
 * following the system, and the palettes. Saved per machine with the window
 * layout; never part of a crawl profile.
 */
export function AppearanceSettings({
  appearance,
  level,
  systemDark,
  onChange,
  material = materialAvailable(),
}: {
  appearance: Appearance;
  level: Level;
  systemDark: boolean;
  onChange: (patch: Partial<Appearance>) => void;
  /** Whether this window can show the system material at all. */
  material?: boolean;
}) {
  const side = (tone: "light" | "dark") => LEVELS.filter((l) => l.tone === tone);
  return (
    <>
      <Group>
        <Row label="Brightness" stacked help="The same six steps as the brightness button in the toolbar.">
          {(_, l) => (
            <div role="radiogroup" aria-labelledby={l} className="grid grid-cols-3 gap-2">
              {LEVELS.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  role="radio"
                  aria-checked={s.id === level}
                  onClick={() => onChange(chooseLevel(appearance, s.id, systemDark))}
                  className={cn(
                    "flex flex-col overflow-hidden rounded-md border text-sm",
                    s.id === level ? "border-accent text-fg ring-1 ring-accent" : "border-line text-fg-2 hover:border-line-strong",
                  )}
                >
                  <span aria-hidden data-preview={s.id} className="flex h-9">
                    <span className="w-1/3 bg-preview-side" />
                    <span className="flex-1 bg-preview-main" />
                  </span>
                  <span className="py-1">{s.name}</span>
                </button>
              ))}
            </div>
          )}
        </Row>
        <Row label="Follow system" help="Switch between a light and a dark step when the system appearance changes.">
          {(_, l) => <Switch labelledBy={l} checked={appearance.followSystem} onChange={(v) => onChange({ followSystem: v, level })} />}
        </Row>
        {appearance.followSystem && (
          <>
            <Row label="When the system is light">
              {(id) => (
                <Select id={id} className="w-40" value={appearance.systemLight} onChange={(e) => onChange({ systemLight: e.target.value as Level })}>
                  {side("light").map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </Select>
              )}
            </Row>
            <Row label="When the system is dark">
              {(id) => (
                <Select id={id} className="w-40" value={appearance.systemDark} onChange={(e) => onChange({ systemDark: e.target.value as Level })}>
                  {side("dark").map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </Select>
              )}
            </Row>
          </>
        )}
        {material && (
          <Row label="Translucent sidebar" help="Let the desktop show softly through the sidebar and toolbar. Off, or with Reduce transparency on in System Settings, they are solid.">
            {(_, l) => <Switch labelledBy={l} checked={appearance.translucent !== false} onChange={(v) => onChange({ translucent: v })} />}
          </Row>
        )}
      </Group>
      <Group>
        <Row label="Accent" stacked help={`Primary button, selection and focus. Colours for status codes and warnings stay the same in every palette. Shown in ${toneOf(level)} tone.`}>
          {(_, l) => (
            <div role="radiogroup" aria-labelledby={l} className="grid grid-cols-3 gap-2">
              {ACCENTS.map((a) => (
                <AccentSwatch key={a.id} id={a.id} name={a.name} withName on={appearance.accent === a.id} onPick={() => onChange({ accent: a.id })} />
              ))}
            </div>
          )}
        </Row>
      </Group>
    </>
  );
}
