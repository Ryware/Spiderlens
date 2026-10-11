# Tasks: Design System and App Shell (017)

Each task includes its tests. [P] = can run in parallel.

## Phase 1: Foundations
- [ ] T001 Add dependencies (Base UI, Tailwind v4, cmdk, lucide, clsx) and the Tailwind Vite plugin
- [ ] T002 Token file (light, dark, meaning colors, type scale, spacing, radius, row heights, motion) and Tailwind `@theme inline` mapping
- [ ] T003 [P] Token check (SC-004) and contrast check (SC-003) as tests
- [ ] T004 [P] CSPProvider with disabled style elements, and the two scrollbar rules in CSS

## Phase 2: Shared components (FR-021, FR-022)
- [ ] T005 Button, IconButton, Kbd, Badge (meaning + label), Chip with count, Bar
- [ ] T006 Input, NumberField, Textarea, Select, Segmented, Switch
- [ ] T007 Tooltip, Menu, Sheet (dialog), Notice, EmptyState

## Phase 3: Shell (US1, US3, US5)
- [ ] T008 Theme (Light / Dark / System, persisted, follows the system live) with tests
- [ ] T009 Layout state (pane sizes, collapsed, last category) persisted, with tests
- [ ] T010 AppShell five regions, collapse order by width, resizable panes
- [ ] T011 Toolbar, StatusBar, Sidebar, DetailPanel, StartState
- [ ] T012 About dialog (version, engine, build id, copy)
- [ ] T013 `tauri.conf.json`: overlay title bar on macOS, drag region, minimum size

## Phase 4: Settings sheet (US2)
- [ ] T014 Lift state to `useCrawlSetup`
- [ ] T015 SettingsSheet with category list, counts, grouped rows, help and inline issues
- [ ] T016 ProfilePicker with all profile actions in one menu
- [ ] T017 Port every spec 002 UI test to the sheet (SC-009) and add sheet tests (US2 scenarios)

## Phase 5: Keyboard (US4)
- [ ] T018 Command registry, shortcuts (settings, palette, start), palette with recent commands
- [ ] T019 Keyboard-only test of the task list reachable today (open settings, change speed, start)

## Phase 6: Polish
- [ ] T020 Motion tokens and reduced-motion handling (US6)
- [ ] T021 Linux reference screenshots (light, dark) of shell and sheet
- [ ] T022 Remove old styles.css and dead components; full test, typecheck and build pass
