# Feature Specification: Design System and App Shell

**Feature Branch**: `017-design-system-shell`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "The current settings screen is bad design. Make SpiderLens look and feel like Headroom: a native desktop tool, not a web form. Approved direction (2026-10-11): a three-pane window with toolbar, sidebar, content, inspector and status bar; one small set of design tokens for color, type, spacing and density; light and dark themes; native window chrome per platform; keyboard-first with a command palette; crawl settings shown as a focused sheet with categories and grouped rows. The UI layer is replaced; the engine and its boundary stay as they are."

**Numbering note**: 009 is reserved for the issue rules engine in the product backlog, so this spec takes the next free number. It must be implemented **before** spec 007 (Results interface), which builds its screens on this shell.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A window that reads as a desktop tool (Priority: P1)

When an SEO specialist opens SpiderLens, they see a desktop application window, not a page. The crawl controls (mode, start URL, start, pause, stop, progress) sit in a toolbar at the top. A sidebar on the left lists where they can go (an overview and the result tabs). The middle area shows the current view. A detail panel on the right shows information about whatever is selected. A status bar at the bottom shows the live crawl figures. The window uses its full size at any width. The app version and build moved to an About window and are no longer the main content.

**Why this priority**: Every later screen (results, reports, AI visibility) lives inside this frame. Without it each screen invents its own layout, which is how the current screen ended up as a centred column of unrelated controls.

**Independent Test**: Launch the app with no crawl and with a sample crawl file open. Check that the five regions are present and in place, that the sidebar and detail panel can be resized, collapsed and restored, and that the layout comes back the same after a restart.

**Acceptance Scenarios**:

1. **Given** the app is launched for the first time, **When** the window opens, **Then** it shows a toolbar, a sidebar, a main area, a detail panel and a status bar, and the main area fills all space not used by the other regions at every window size from the minimum up to full screen.
2. **Given** no crawl has been started or opened, **When** the user looks at the main area, **Then** it shows a start state that invites them to enter a URL or open a crawl file, with the start URL field focused, and not an empty table.
3. **Given** the user resizes or collapses the sidebar or the detail panel, **When** they quit and reopen the app, **Then** the sizes and collapsed state are as they left them.
4. **Given** the window is made narrower than the space needed for all regions, **When** it crosses that width, **Then** the detail panel collapses first, then the sidebar collapses to icons, and no region overlaps another or clips its text.
5. **Given** the user wants the version and build, **When** they open About from the app menu, **Then** the product version, engine version and build id are shown there and can be copied.

---

### User Story 2 - Crawl settings as a focused sheet (Priority: P1)

The user opens crawl settings from the toolbar or with the standard settings shortcut. Settings appear as a sheet over the window. On the left is the profile picker and the list of categories (Limits, Scope, Identity, Speed, robots.txt, Network and retries). On the right the chosen category shows its settings as grouped rows: a label on the left, the control on the right, and one line of help underneath where it adds something. Problems found by the engine appear in the row they belong to, and each category in the list shows how many warnings or errors it holds. All behaviour from spec 002 stays the same; only how it is presented changes.

**Why this priority**: This is the screen the owner rejected. It is small, it is the first thing a user configures, and it exercises every basic control, so it proves the design system on real content.

**Independent Test**: Open the sheet, visit every category, change one value in each, and confirm the same validation results, profile behaviour and saved configuration as the spec 002 tests, with the problem counts per category matching the engine's validation output.

**Acceptance Scenarios**:

1. **Given** the main window, **When** the user presses the settings shortcut or the Configure button, **Then** the settings sheet opens on the category used last (Speed on first use) and keyboard focus moves into it.
2. **Given** a value that the engine reports as an error, **When** the user changes it, **Then** within the spec 002 validation delay the row shows the message in place, the category shows an error count, and the Start control in the toolbar is disabled with a tooltip naming the first problem.
3. **Given** the Speed category, **When** the user picks Polite, Fast or Custom, **Then** the preset is shown as one segmented choice and the rows below reflect the preset's values; editing any of them switches the choice to Custom.
4. **Given** saved profiles exist, **When** the user opens the profile picker, **Then** they can switch profile, save the current settings as a new profile, import a profile file and reset to defaults from that one menu, with no other profile controls elsewhere.
5. **Given** the sheet is open, **When** the user presses Escape or Done, **Then** the sheet closes and the toolbar shows the active profile name.

---

### User Story 3 - One consistent visual language in light and dark (Priority: P1)

Every screen uses the same small set of colors, type sizes, spacing steps, corner radii and row heights. The app follows the system light or dark appearance and the user can override it. There is one accent color, taken from the SpiderLens icon, used for the primary action, selection and focus only. Meaning (success, information, warning, error) has its own four colors, always paired with a text label or symbol so color is never the only signal. Numbers that line up in columns use fixed-width digits. URLs and code use a monospaced face.

**Why this priority**: Consistency is what makes Headroom feel finished. Defining it once is cheaper than fixing each screen later, and spec 007 needs these conventions for status badges, filter counts and inline bars.

**Independent Test**: Render a reference screen containing every shared component in both themes. Check contrast ratios automatically, check that no screen uses a color, size or spacing value outside the defined set, and compare screenshots against approved references.

**Acceptance Scenarios**:

1. **Given** the operating system is in dark mode and the user has not overridden it, **When** the app starts, **Then** it uses the dark theme, and switching the system to light switches the app without a restart.
2. **Given** the user chooses Light, Dark or System in preferences, **When** they restart the app, **Then** the choice is kept.
3. **Given** any status shown in the app (for example an HTTP status, indexability or a validation problem), **When** it is displayed, **Then** it uses one of the four meaning colors together with a text label, and the same status looks the same everywhere it appears.
4. **Given** a table or list of figures, **When** values of different lengths stack, **Then** their digits align in columns.

---

### User Story 4 - Keyboard first (Priority: P2)

An experienced user can run the whole app without the mouse. A command palette opens with the standard shortcut (Cmd+K on macOS, Ctrl+K elsewhere) and finds any action or destination by typing: switch tab, apply a filter, start, pause or stop the crawl, open settings, export. Common actions also have direct shortcuts, shown next to them in menus and in the palette. Focus is always visible.

**Why this priority**: Power users of crawlers live in the keyboard, and the palette is also where AI-agent features will plug in later. It is not needed to make the first screens usable, so it comes after the shell and settings.

**Independent Test**: With a sample crawl open, complete a scripted task list (open settings, change speed, start a crawl, switch to Page titles, apply Over 60 characters, select a row, open its detail panel, export) using only the keyboard.

**Acceptance Scenarios**:

1. **Given** any screen without a modal open, **When** the user presses the palette shortcut, **Then** the palette opens with the search field focused and lists recent commands first.
2. **Given** the palette is open, **When** the user types part of a command or tab name, **Then** matching entries appear in under 100 ms, each with its shortcut if it has one, and Enter runs the highlighted entry.
3. **Given** keyboard navigation, **When** focus moves to any interactive element, **Then** a visible focus indicator is drawn that meets contrast requirements in both themes.

---

### User Story 5 - Feels native on each platform (Priority: P2)

On macOS the toolbar sits in the title bar next to the window buttons, the sidebar uses the system's translucent material, and the system font is used. On Windows 11 the window uses the system backdrop material and the standard caption buttons, with the Windows system font. On Linux the app uses a solid, opaque look that works under any desktop environment. The results table and other content are always opaque so they stay readable and fast.

**Why this priority**: Native chrome is a large part of why Headroom feels right, but the app is fully usable without it, so it follows the core work.

**Independent Test**: Run the reference screen on macOS (current and previous major version), Windows 10, Windows 11 and one Linux desktop, and compare each against the platform reference screenshots.

**Acceptance Scenarios**:

1. **Given** macOS, **When** the main window opens, **Then** the window buttons sit inside the toolbar row, the window can be dragged by the toolbar's empty space, and double-clicking that space zooms the window as in other Mac apps.
2. **Given** a platform or version without a translucent material, **When** the window opens, **Then** the sidebar uses the solid sidebar color from the design tokens and nothing else changes.
3. **Given** the user has turned on the system's reduce-transparency setting, **When** the app runs, **Then** no translucent material is used.

---

### User Story 6 - Quiet motion and clear feedback (Priority: P3)

Motion is used only to explain change: live counters roll to their new value, the settings sheet and short notices slide in and out, panels resize smoothly. Table rows never animate. Every action gives feedback within 100 ms, and anything slower shows progress. If the system asks for reduced motion, the app changes state instantly.

**Why this priority**: Polish that matters once the screens exist.

**Independent Test**: Record the shell during a sample crawl with normal and reduced motion settings and verify durations and that rows do not animate.

**Acceptance Scenarios**:

1. **Given** a running crawl, **When** the status bar figures change, **Then** they update at least once per second with a transition of at most 200 ms, and with reduced motion they change without any transition.
2. **Given** the user starts an export or another action longer than 1 second, **When** it runs, **Then** progress is shown and the rest of the window stays usable.

### Edge Cases

- **Very small windows**: below the minimum size the window cannot be resized further; the minimum keeps the toolbar's primary controls, the main area and the status bar visible.
- **Large text and zoom**: at 200% interface zoom or the largest system text size, no text is clipped; regions scroll or collapse instead.
- **Long values**: URLs, titles and profile names longer than their space are shortened with an ellipsis (URLs in the middle, keeping the end of the path) and shown in full on hover and in the detail panel.
- **Right-to-left text**: page titles and other crawled text in right-to-left scripts such as Hebrew and Arabic display in the correct direction inside cells and the detail panel, without flipping the app's own layout.
- **High contrast**: with the operating system's high-contrast or increased-contrast setting on, borders and focus indicators become stronger and the meaning colors still pass contrast checks.
- **Theme change mid-crawl**: switching appearance during a crawl does not interrupt it, reset scroll positions or lose the selection.
- **Missing fonts**: if a preferred font is not available, the platform's default interface or monospaced font is used and layouts still fit.

## Requirements *(mandatory)*

### Functional Requirements

**Shell**

- **FR-001**: The main window MUST consist of a toolbar, sidebar, main area, detail panel and status bar, in those positions, on every platform.
- **FR-002**: The toolbar MUST hold the crawl mode choice (Spider or List), the start URL or list summary, the crawl progress, the Start, Pause/Resume and Stop controls, and the entry to settings; exactly one control at a time is styled as the primary action.
- **FR-003**: The sidebar MUST list an overview and the result tabs defined by spec 007, each with its live count, plus grouped entries for later milestones shown as unavailable with the milestone they arrive in.
- **FR-004**: Users MUST be able to resize and collapse the sidebar and the detail panel; sizes and states MUST persist across restarts.
- **FR-005**: The window MUST collapse regions in a defined order as it narrows (detail panel, then sidebar to icons) and MUST never overlap regions or clip text.
- **FR-006**: The status bar MUST show the crawl state and the live figures from spec 007 (crawled, queued, errors, URLs per second, elapsed time) and the active speed preset.
- **FR-007**: Product version, engine version and build id MUST move to an About window reachable from the app menu.
- **FR-008**: When no crawl is open, the main area MUST show a start state with the start URL field focused and the option to open a crawl file.

**Settings sheet**

- **FR-009**: Crawl settings MUST open as a sheet over the main window from the toolbar and from the platform's standard settings shortcut.
- **FR-010**: The sheet MUST show the profile picker and the six setting categories of spec 002 as a list, and the selected category's settings as grouped label-and-control rows with optional one-line help.
- **FR-011**: Validation results from the engine MUST be shown in the row they refer to, and each category MUST show its count of errors and warnings.
- **FR-012**: All profile actions (switch, save as new, import, reset to defaults) MUST be reachable from the profile picker.
- **FR-013**: The settings sheet MUST NOT change any behaviour, default or validation rule defined in spec 002.

**Design tokens**

- **FR-014**: All colors, font sizes and weights, spacing steps, corner radii, row heights and motion durations MUST come from one shared set of named design tokens; screens MUST NOT use values outside that set.
- **FR-015**: The token set MUST define a light and a dark theme from the same names; the app MUST follow the system appearance by default and allow Light, Dark or System as a persisted preference.
- **FR-016**: The palette MUST have exactly one accent color, derived from the SpiderLens icon, used only for the primary action, selection and focus.
- **FR-017**: The palette MUST define four meaning colors (success, information, warning, error) plus a neutral, and every use of them MUST be paired with a text label or symbol.
- **FR-018**: Text MUST meet WCAG 2.2 AA contrast (4.5:1 for body text, 3:1 for large text and interface component boundaries) in both themes.
- **FR-019**: The type scale MUST use the platform's interface font for text and a monospaced font for URLs, headers and source code, with fixed-width digits wherever figures align.
- **FR-020**: Dense views MUST use a default row height between 28 and 32 points with a 4-point spacing grid, so a 1080-pixel-tall screen shows at least 25 table rows.

**Shared components**

- **FR-021**: The design system MUST provide shared components used by every screen: button (primary, secondary, quiet, destructive), text and number field, select, segmented choice, switch, checkbox, status badge, filter chip with count, inline proportion bar, tooltip, menu, sheet, notice, empty state and the table conventions (header, sort indicator, selection, truncation).
- **FR-022**: Each shared component MUST have a defined appearance for its default, hover, pressed, focused, disabled and error states in both themes.

**Keyboard and accessibility**

- **FR-023**: A command palette MUST open with Cmd+K on macOS and Ctrl+K on Windows and Linux, and MUST list every navigation destination and every action available from menus or the toolbar.
- **FR-024**: Every action MUST be reachable by keyboard, and shortcuts MUST be shown next to their actions in menus and in the palette.
- **FR-025**: Focus MUST always be visible, and every interactive element MUST expose its name, role and state to screen readers.

**Platform**

- **FR-026**: On macOS the toolbar MUST share the title bar row with the window buttons; on Windows the standard caption buttons MUST be used; on Linux the system title bar or a solid equivalent MUST be used.
- **FR-027**: Translucent system materials MAY be used only behind the sidebar and toolbar, MUST fall back to solid token colors where unavailable or when reduce-transparency is on, and MUST never be used behind content.

**Motion**

- **FR-028**: Animated transitions MUST last no more than 250 ms, MUST be limited to counters, sheets, notices and panel resizing, and MUST be turned off when the system asks for reduced motion.

### Key Entities

- **Design token**: a named value (color per theme, size, spacing step, radius, row height, duration) that screens refer to by name.
- **Theme**: the set of token values for Light or Dark.
- **Layout state**: the user's sidebar and detail panel sizes and collapsed states, the last settings category and the theme preference, kept across restarts.
- **Command**: an action or destination with a name, optional shortcut and availability rule, listed in the palette and menus.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time user starts a crawl of a given URL with default settings within 15 seconds of the window opening, without help, in 4 of 5 moderated trials.
- **SC-002**: A first-time user finds and changes the per-site request limit within 30 seconds of being asked, in 4 of 5 trials.
- **SC-003**: 100% of text and interface boundaries pass the contrast thresholds in FR-018 in both themes, as checked by an automated audit of the reference screen.
- **SC-004**: An automated check finds zero color, font size, spacing or radius values in the UI that are not design tokens.
- **SC-005**: The scripted keyboard-only task list in User Story 4 is completed with zero mouse use.
- **SC-006**: The main window shows its first complete frame within 1 second of launch on the reference machine.
- **SC-007**: Every action gives visible feedback within 100 ms on the reference machine.
- **SC-008**: Approved reference screenshots exist for the main window and settings sheet in both themes on macOS, Windows and Linux, and visual comparison detects unintended changes in CI.
- **SC-009**: All spec 002 acceptance tests still pass against the redesigned settings sheet.

## Assumptions

- The direction approved by the owner on 2026-10-11 (published as "SpiderLens Design Direction") is the visual reference: dark-first in screenshots, teal accent from the icon, Headroom's window structure.
- Dark and light are equally supported; "dark-first" means reference screenshots and design reviews start with dark.
- The settings move from a full screen into a sheet; there is no separate settings window.
- Result tabs, filters, columns and the detail panel's contents are defined by spec 007; this spec defines only the frame they appear in and their visual conventions.
- Entries for later milestones (for example AI crawlers and llms.txt) appear in the sidebar as unavailable so the structure is stable; they can be hidden by a later decision.
- The reference machine is the one defined in specs 005, 007 and 008.
- Moderated trials for SC-001 and SC-002 can be run informally by the owner with people who have not used SpiderLens.
- Choice of component, styling and table technology belongs in the plan, not here.

## Dependencies

- Spec 001 (foundation) for the desktop shell and build pipeline.
- Spec 002 (crawl configuration) for every setting, validation rule and profile behaviour shown in the sheet.
- Spec 007 (results interface) depends on this spec for its frame and visual conventions.
