# Tasks: Design System and App Shell (017)

Each task includes its tests. [P] = can run in parallel.

## Phase 1: Foundations
- [x] T001 Add dependencies (Base UI, Tailwind v4, cmdk, lucide, clsx) and the Tailwind Vite plugin
- [x] T002 Token file (light, dark, meaning colors, type scale, spacing, radius, row heights, motion) and Tailwind `@theme inline` mapping
- [x] T003 [P] Token check (SC-004) and contrast check (SC-003) as tests
- [x] T004 [P] CSPProvider with disabled style elements, and the two scrollbar rules in CSS

## Phase 2: Shared components (FR-021, FR-022)
- [x] T005 Button, IconButton, Kbd, Badge (meaning + label), Chip with count, Bar
- [x] T006 Input, NumberField, Textarea, Select, Segmented, Switch
- [x] T007 Tooltip, Menu, Sheet (dialog), Notice, EmptyState

## Phase 3: Shell (US1, US3, US5)
- [x] T008 Theme (Light / Dark / System, persisted, follows the system live) with tests
- [x] T009 Layout state (pane sizes, collapsed, last category) persisted, with tests
- [x] T010 AppShell five regions, collapse order by width, resizable panes
- [x] T011 Toolbar, StatusBar, Sidebar, DetailPanel, StartState
- [x] T012 About dialog (version, engine, build id, copy)
- [x] T013 `tauri.conf.json`: overlay title bar on macOS, drag region, minimum size (to verify on a Mac: not buildable in this Linux container)

## Phase 4: Settings sheet (US2)
- [x] T014 Lift state to `useCrawlSetup`
- [x] T015 SettingsSheet with category list, counts, grouped rows, help and inline issues
- [x] T016 ProfilePicker with all profile actions in one menu
- [x] T017 Port every spec 002 UI test to the sheet (SC-009) and add sheet tests (US2 scenarios)

## Phase 5: Keyboard (US4)
- [x] T018 Command registry, shortcuts (settings, palette, start), palette with recent commands
- [x] T019 Keyboard-only test of the task list reachable today (open settings, change speed, start)

## Phase 6: Polish
- [x] T020 Motion tokens and reduced-motion handling (US6)
- [x] T021 Linux reference screenshots (light, dark) of shell, sheet and palette in `screenshots/` (Chromium, 1280x800 @2x)
- [x] T022 Remove old styles.css and dead components; full test, typecheck and build pass
