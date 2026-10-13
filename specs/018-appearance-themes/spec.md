# Feature Specification: Appearance, Brightness Levels and Accent Palettes

**Feature Branch**: `018-appearance-themes`

**Created**: 2026-10-13

**Status**: Draft, waiting for the owner's approval of the spec and of the palette set

**Input**: User description: "I want the same UI changes + Select multiple ui color palette selection (this palette can sit in settings) investigate what's needed in order to implement it and what are the color lead in 2026/7". A screen recording of the Decim admin was attached. It shows a theme control in the header: a button with a sun or moon icon opens a popover holding a vertical slider. The slider track runs from white at the top to black at the bottom, with six named stops (Light, Soft, Island, Dark, Deep, Midnight), each with its own icon. Dragging the thumb snaps to a stop and re-themes the whole app live, and the header icon follows the chosen stop. Island keeps a dark frame and sidebar around light content.

**Prototype**: https://claude.ai/artifact/5Fr3kBynymfz6LoxVL7gZ3 (clickable SpiderLens shell with the slider, the Appearance settings page and all six accents). Screenshots in [screenshots/](screenshots/).

**Numbering note**: 009 to 016 are taken by the product backlog (014 is JavaScript rendering) and 017 is the design system, so this spec takes 018. It changes spec 017 in one place: FR-016 there says the UI has exactly one accent, the teal. After this spec the UI still uses one accent at a time, but the user picks which one; teal stays the default.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pick how bright the app is, from the toolbar (Priority: P1)

A user working late, or in a bright office, wants the app darker or lighter than plain Light and Dark. A button at the right of the toolbar shows the current brightness as an icon. Clicking it opens a small popover with a vertical slider of six steps: Light, Soft, Island, Dark, Deep, Midnight. Dragging the thumb, clicking a step name, or using the arrow keys changes the whole window at once, while the popover stays open, so the user sees each step on their real crawl before letting go. The choice is remembered.

**Why this priority**: This is the change shown in the recording and the one the owner asked to copy. Everything else in this spec builds on the brightness steps being a set of token values.

**Independent Test**: Open a sample crawl, open the popover, move through all six steps with the mouse and then with the keyboard, and check that every region of the window (toolbar, sidebar, table, detail panel, status bar, sheets, popovers, command palette) changes on each step, that the toolbar icon matches the step, and that the step is still selected after a restart.

**Acceptance Scenarios**:

1. **Given** the main window, **When** the user clicks the brightness button, **Then** a popover opens under it with the six steps in order from lightest to darkest, the current step marked, and keyboard focus on the slider.
2. **Given** the popover is open, **When** the user drags the thumb, **Then** it snaps to the nearest step and the window re-themes as soon as the thumb crosses into a new step, without waiting for release.
3. **Given** the slider has focus, **When** the user presses Up, Down, Home or End, **Then** the step moves by one, or to the first or last step, and a screen reader announces the step name.
4. **Given** the user chose a step, **When** they quit and reopen the app, **Then** the window opens directly in that step, with no flash of another step while it loads.
5. **Given** the popover is open, **When** the user presses Escape or clicks outside it, **Then** it closes and focus returns to the brightness button.

---

### User Story 2 - Pick an accent palette in Settings (Priority: P1)

A user wants SpiderLens in a colour they like, or one that is easier on their eyes. In Settings there is an Appearance page, separate from the crawl profile categories. It shows the six brightness steps as small previews, a Follow system switch, and the accent palettes as named swatches: Teal, Cobalt, Lavender, Orchid, Celery and Cloud. Picking one changes the primary button, the selection, focus rings, links and the faint tint of the greys, at once. Status colours (2xx, 3xx, 4xx, 5xx, warnings) do not change.

**Why this priority**: This is the palette selection the owner asked for. It shares all its plumbing with User Story 1.

**Independent Test**: Open Settings, go to Appearance, pick each accent in turn in a light step and a dark step, and check that only accent-coloured elements change while status colours keep their exact values. Switch crawl profile and reset the profile to defaults, and check the accent and brightness stay as chosen.

**Acceptance Scenarios**:

1. **Given** the settings sheet is open, **When** the user looks at its category list, **Then** Appearance appears in its own group below the crawl profile categories and shows no warning or error count.
2. **Given** the Appearance page, **When** the user picks an accent, **Then** the window behind the sheet and the sheet itself change to that accent immediately, and the chosen swatch is marked.
3. **Given** any accent and any brightness step, **When** a crawl result shows a 200, 301, 404 or 503, **Then** those status colours are identical to their values with the default accent.
4. **Given** the user switches profile, imports a profile, resets to defaults or exports a profile, **When** they look at the app, **Then** brightness and accent are unchanged, and the exported profile file contains no appearance settings.
5. **Given** the command palette, **When** the user types "accent" or a palette name, **Then** each palette can be applied from there, as the existing Appearance commands can.

---

### User Story 3 - Follow the system, with my own light and dark steps (Priority: P2)

Today Appearance can follow the system light or dark mode. With six steps, a user who follows the system picks which step is used in light mode (Light, Soft or Island) and which in dark mode (Dark, Deep or Midnight). When macOS, Windows or the Linux desktop switches, SpiderLens switches with it.

**Why this priority**: It keeps the existing System behaviour, which is the default for new users, and is how Discord and GitHub handle several dark levels. It is P2 because a fixed step already covers most people.

**Independent Test**: Turn Follow system on, choose Soft for light and Deep for dark, then toggle the operating system appearance and check the window follows within a second each way.

**Acceptance Scenarios**:

1. **Given** a new installation, **When** the app opens, **Then** Follow system is on, mapped to Light and Dark, which looks exactly as the app does today.
2. **Given** Follow system is on, **When** the user moves the toolbar slider to a step, **Then** that step becomes the system's light or dark choice for the matching side, and Follow system stays on.
3. **Given** Follow system is on with Soft and Deep, **When** the operating system switches between light and dark, **Then** the window switches between Soft and Deep without a restart.

---

### User Story 4 - Island: a dark frame around light data (Priority: P2)

Some users like dark chrome but read tables better on a light background. In the Island step the toolbar, sidebar and status bar use the Dark step's colours and the results table and detail panel sit on a light, rounded panel inset from the frame, as in the recording.

**Why this priority**: Island is the one step that is not just a different set of greys: it needs two tone sets in one window. It is P2 because the other five steps do not depend on it.

**Independent Test**: Choose Island and check that every control inside the toolbar, sidebar and status bar (buttons, the URL field, segmented control, counts, menus they open) is legible with dark-step contrast, and everything inside the content panel with light-step contrast.

**Acceptance Scenarios**:

1. **Given** Island, **When** the user looks at the window, **Then** the toolbar, sidebar and status bar are dark, the content panel is light with rounded corners and a margin, and there is no light seam or dark seam between them.
2. **Given** Island, **When** the user opens a popover or menu from the toolbar, **Then** it uses the tone of the region it belongs to and meets the same contrast rules as that tone.

---

### Edge Cases

- The operating system asks for increased contrast: the existing rule (stronger lines, secondary text as strong as primary) applies on top of every step and accent.
- Reduced motion: re-theming never animates colour; with reduced motion off it may cross-fade for at most the existing 160 ms token duration, and never while the user is dragging.
- A stored step or accent name that this version does not know (from a newer build, or a removed palette) falls back to the default for that field only; the rest of the layout state is kept.
- Midnight uses true black content; panes must still be told apart by lines, so line tokens there are never lighter than in Deep.
- A crawl with 1 million rows is open: changing the step or accent must not re-render the rows and must not drop frames beyond what a colour change costs the browser.
- The window's native parts (macOS traffic lights and title bar, scroll bars, the Windows title bar) follow the tone of the step: light for Light and Soft, dark for Dark, Deep, Midnight and Island.

## Requirements *(mandatory)*

### Functional Requirements

**Brightness steps**

- **FR-001**: The app MUST offer six brightness steps, in this order: Light, Soft, Island, Dark, Deep, Midnight. Light and Dark MUST keep today's token values exactly.
- **FR-002**: Each step MUST be a complete set of the neutral and status colour tokens defined in spec 017 (backgrounds, surfaces, sidebar, chrome, lines, control edges, three text levels, the four meaning colours and their soft fills, scrim, shadows). No component may test which step is active to pick a colour; components keep using token names only.
- **FR-003**: Soft MUST be a lower-glare light step with a warm paper tint. Deep MUST be darker than Dark. Midnight MUST use true black for the main content background.
- **FR-004**: Island MUST apply the Dark step's tokens to the toolbar, sidebar and status bar and the Light step's tokens to the content and detail regions, and MUST show the content as an inset panel with the large radius token.

**Toolbar control**

- **FR-005**: The toolbar MUST have a brightness button, before the command palette button, whose icon reflects the current step (sun for Light, smaller sun for Soft, half-filled circle for Island, moon for Dark, filled moon for Deep, moon with a star for Midnight).
- **FR-006**: The button MUST open a popover with a vertical slider (track shading from light at the top to black at the bottom) and the six step names with their icons; the step names MUST be clickable; the slider MUST be operable by pointer drag, click and keyboard and expose its value to assistive technology as a slider with the step name as its text value.
- **FR-007**: Changing the step MUST apply to the whole window immediately, including while dragging.

**Accent palettes**

- **FR-008**: The app MUST offer these accent palettes: Teal (default), Cobalt, Lavender, Orchid, Celery and Cloud, with the values in research.md R4 unless changed at approval.
- **FR-009**: Each palette MUST define the accent tokens of spec 017 (accent, hover, label on accent, accent text, soft fill, selection) once for the light steps and once for the dark steps, and MAY tint the neutral tokens towards its hue, by no more than the chroma the current neutrals use.
- **FR-010**: The four meaning colours MUST NOT change with the palette. Every palette with visible hue MUST sit at least 25 degrees of OKLCH hue away from every meaning colour's hue, so an accent is never mistaken for a status.
- **FR-011**: Palettes MUST be listed in data in one place, so adding one is a token block plus one list entry, with no component changes.

**Settings and persistence**

- **FR-012**: The settings sheet MUST show an Appearance page in its own group, below the crawl profile categories, with: the six steps as previews, the Follow system switch with its light and dark step choices, and the palettes as named swatches. Appearance MUST NOT be part of the crawl profile and MUST NOT be touched by any profile action.
- **FR-013**: Follow system MUST be the default, mapped to Light and Dark. With Follow system on, the active step MUST follow the operating system's light or dark setting live.
- **FR-014**: Brightness, accent and the Follow system mapping MUST be remembered per machine with the rest of the window layout (spec 017 FR-015) and MUST be applied before the first paint on the next launch.
- **FR-015**: The command palette MUST offer one command per step and per accent, replacing today's three Appearance commands with the new set plus "Appearance: Follow system".
- **FR-016**: The native window appearance MUST follow the tone of the active step, as listed in the edge cases.

**Quality**

- **FR-017**: Every combination of step and palette MUST meet the spec 017 contrast rules (FR-018 there): text 4.5:1 on every surface, meaning colour text 4.5:1 on its soft fill, label on accent 4.5:1, accent text 4.5:1, focus ring and control edges 3:1. Island MUST meet them separately for its dark regions and its light regions.
- **FR-018**: The existing token rule stays: no colour may be defined outside the token file, and every step and palette block may only redefine tokens that the base block defines.

### Key Entities

- **Brightness step**: one of six named sets of neutral and meaning colour tokens, with a tone (light or dark) that sets the native window appearance.
- **Accent palette**: a name, a hue, and two accent token sets (for light tone and dark tone).
- **Appearance preference**: per machine; the chosen step, the chosen palette, whether to follow the system, and the light and dark steps used when following.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All 36 combinations of step and palette, plus Island's two regions, pass every check in the automated contrast test that today runs for Light and Dark only.
- **SC-002**: A user can go from any step to any other in at most two actions from the main window (open the popover, pick the step).
- **SC-003**: Switching step or palette with a 1 million row crawl open takes under 100 ms to the next painted frame on the reference Mac.
- **SC-004**: After a restart the first painted frame already uses the saved step and palette in 100% of launches in the automated launch check.
- **SC-005**: With the defaults (Follow system, Teal), screenshots of every reference screen match spec 017's approved screenshots exactly.
- **SC-006**: Every screen in the UI test suite renders in every step without a raw colour or a token missing in that step (no fallback to an undefined value).

## Assumptions

- The six step names come from the recording and are kept as they are; the owner can rename them at approval.
- Appearance stays per machine and outside the crawl profile, as spec 017 research R7 decided for the theme. The command-line tool has no use for it.
- No custom colour picker in this spec: users choose from a curated list, because every palette must be contrast-checked in every step. A free picker can come later on top of the same token structure.
- A separate high-contrast setting and colour-blind status colours are out of scope here; research.md R7 notes them as follow-ups.

## Dependencies

- Spec 017 (design system and shell): the token file, the token test, the settings sheet and the layout state this spec extends.
- The UI test suite work in progress: new tests extend the existing token test and settings sheet tests rather than adding new scaffolding.
