# Implementation Plan: Design System and App Shell

**Branch**: `017-ui-shell` | **Date**: 2026-10-11 | **Spec**: [spec.md](spec.md)

## Summary

Replace the UI layer's presentation with a design system (tokens and shared components) and a
desktop app shell (toolbar, sidebar, main area, detail panel, status bar), and move crawl
settings into a sheet. The engine, `transport.ts`, the generated types and all validation stay
exactly as they are (FR-013, constitution III).

## Technical Context

**Language/Version**: TypeScript 5.6, React 18.3 (UI); Tauri 2 config only on the Rust side
**Primary Dependencies**: `@base-ui/react` 1.9, Tailwind CSS 4.3 (`@tailwindcss/vite`),
`cmdk` 1.1, `lucide-react`, `clsx`. `@tanstack/react-table` 9 and `@tanstack/react-virtual` 3
are chosen for the 007 grid (research R3) and are added by 007, not here.
**Storage**: `localStorage` for theme and layout state (R7)
**Testing**: Vitest + Testing Library (existing); a token check (SC-004) and a contrast
check (SC-003) as Vitest tests over the token file and sources
**Target Platform**: Tauri 2 webviews (WKWebView, WebView2, WebKitGTK)
**Performance Goals**: first frame < 1 s (SC-006); feedback < 100 ms (SC-007)
**Constraints**: CSP `style-src 'self'` unchanged (R2); no network fonts (R5)

## Constitution Check

| Principle | Check |
| --- | --- |
| I. Spec before code | Spec 017 approved 2026-10-11. |
| II. Local-first | No new network use; system fonts only. |
| III. Engine independent | No engine or command changes; `Transport` unchanged. |
| IV. Respectful crawling | Spec 002 warnings stay visible in the sheet, in category counts and in the toolbar. |
| V. Correct data | Presentation only. |
| VI. Scales by design | Grid foundation proven at 1M rows (R3); scaled scrolling noted for 007. |
| VII. Test-first | Spec 002 UI tests are ported, not dropped (SC-009); new shell, theme, palette and token tests. |
| VIII. Simplicity | Each new dependency replaces hand-rolled code: Base UI (accessible primitives), Tailwind (token utilities), cmdk (palette search), lucide (icons). |

## Project Structure

```text
ui/src/
├── styles/
│   ├── tokens.css          # the only place color, type, spacing, radius, motion are defined
│   └── app.css             # Tailwind entry, @theme inline mapping, base layer
├── components/ui/          # shared components (FR-021)
├── shell/
│   ├── AppShell.tsx        # five regions, collapse order, pane persistence
│   ├── Toolbar.tsx         # mode, start URL, Start, settings entry, palette entry
│   ├── Sidebar.tsx         # overview + result tabs (counts arrive with 007)
│   ├── DetailPanel.tsx
│   ├── StatusBar.tsx
│   ├── StartState.tsx
│   ├── AboutDialog.tsx
│   ├── commands.ts         # command registry (palette, shortcuts)
│   ├── CommandPalette.tsx
│   ├── theme.ts            # Light / Dark / System
│   └── layout.ts           # persisted layout state
├── settings/
│   ├── SettingsSheet.tsx   # category list + grouped rows; same state and validation as before
│   ├── categories.tsx      # one component per category
│   ├── ProfilePicker.tsx   # replaces ProfileBar; all actions in one menu
│   ├── ListInputPanel.tsx  # restyled
│   ├── PatternTester.tsx   # restyled
│   └── fields.tsx          # Row, help and issue display built on components/ui
└── crawl/useCrawlSetup.ts  # start URL, mode, config, validation and profile state
```

## Key design decisions

1. **State lifts out of the screen.** Today `SettingsScreen` owns config, validation and start
   URL. The toolbar (mode, URL, Start) and the sheet (settings) both need it now, so it moves
   to one hook, `useCrawlSetup`, used by `AppShell`. The debounced `transport.validate` call and
   its 200 ms delay are unchanged.
2. **Issues are grouped by category.** `issuesAt(path)` already exists; category counts are
   `issuesAt(issues, prefix)` over each category's prefixes, so the counts always equal the
   engine's validation output (acceptance US2-2).
3. **Speed preset becomes a segmented control** (Polite / Fast / Custom). Custom shows
   whenever the engine reports the values match neither preset, as it already does.
4. **Tokens are the only source of values.** A test scans `ui/src` for raw colors (`#hex`,
   `rgb(`, `hsl(`, `oklch(` outside `tokens.css`) and arbitrary Tailwind values
   (`-[12px]`-style) and fails if it finds any (SC-004).
5. **Window chrome**: `tauri.conf.json` gains `titleBarStyle: "Overlay"` and
   `hiddenTitle: true` (honoured on macOS, ignored elsewhere) and a minimum size of
   900 x 560. A `data-platform` attribute set at startup gives the macOS toolbar its inset
   for the window buttons.

## Out of scope here (tracked)

- Translucent materials (FR-027, a MAY): follow-up after measuring the transparent-window cost (R4).
- Results grid, tabs and filters: spec 007, on the foundations in R3.
- Reference screenshots on all three platforms in CI (SC-008): this work adds Linux
  references; the cross-platform screenshot job comes with 007's visual tests.
- Moderated trials (SC-001, SC-002): run by the owner.
