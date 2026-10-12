# Research: Design System and App Shell (017)

Date: 2026-10-11. Background, sources and mockups: "SpiderLens Design Direction"
(https://claude.ai/artifact/57ZpfN5U4ZC36Rb3HLFX7K), approved by the owner on 2026-10-11.

## R1. Component layer: copy-in components on Base UI

**Decision**: shadcn/ui-style components (component source lives in `ui/src/components/ui`,
owned by us) built on `@base-ui/react` 1.9 headless primitives.

**Rationale**: we control every pixel and every state, which FR-021/FR-022 require. Base UI
is MUI's maintained headless layer (stable since 1.0.0 on 2025-12-11, 1.9.0 on 2026-10-09),
and shadcn supports it as a primitive layer since Feb 2026. Radix's release pace slowed after
its acquisition.

**Alternatives**: Radix primitives (fine, slower cadence); a styled kit such as MUI or Mantine
(fights the custom look, heavier); hand-rolled primitives (accessibility cost too high).

**Note**: we hand-write the shadcn-pattern components instead of running the shadcn CLI, so
the build has no registry dependency. Same structure, same ownership model.

## R2. Styling: Tailwind CSS v4 with OKLCH tokens

**Decision**: Tailwind v4 via `@tailwindcss/vite`; tokens declared once as CSS variables in
`ui/src/styles/tokens.css` and exposed to Tailwind with `@theme inline`. Colors in OKLCH.
Light values on `:root`, dark values under `[data-theme="dark"]` (FR-015); the app resolves
System to Light or Dark at runtime and sets the attribute.

**Rationale**: one token file is the contract (FR-014, SC-004). OKLCH keeps perceived
lightness steady across hues, so the four meaning colors pass contrast together (FR-018).
Tailwind compiles to a static CSS file, which satisfies the app's CSP `style-src 'self'`.

**CSP detail**: Base UI injects a `<style>` tag for scrollbar hiding in ScrollArea and
aligned Select. We wrap the app in `CSPProvider disableStyleElements` and ship those two
rules in our stylesheet, so the CSP stays unchanged.

## R3. Crawl grid: DOM virtualization (spike result)

**Question**: can a DOM-virtualized table (TanStack Virtual, rows fetched in pages over an
async boundary) handle spec 007's one-million-row target in the engines Tauri uses?

**Method**: `spike/` renders N rows of 7 columns at 28 px, fetching 200-row pages with 4 ms
simulated IPC latency. `__measure()` jumps to the last row, then scrolls continuously for
3 s at 6,000 px/s and records frame times. Run on Linux in headless Chromium (the engine
inside WebView2 on Windows) and in WebKitGTK 2.52 under Xvfb with software rendering (the
WebKit engine family used on macOS and Linux).

| Engine | Rows | Height wanted | Height given | Last row reachable | p50 frame | p95 frame | Frames > 33 ms | DOM rows |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Chromium | 1,000,000 | 28,000,000 | 28,000,000 | yes | 16.7 ms | 16.7 ms | 0 / 182 | 49 |
| Chromium | 5,000,000 | 140,000,000 | 33,554,428 | no (stops at 1,198,382) | 16.7 ms | 16.7 ms | 0 / 182 | 49 |
| WebKitGTK | 1,000,000 | 28,000,000 | 28,000,000 | yes | 16 ms | 34 ms | 9 / 159 | 49 |
| WebKitGTK | 5,000,000 | 140,000,000 | 33,554,428 | no (stops at 1,198,382) | 16 ms | 31 ms | 5 / 172 | 50 |

**Findings**:
1. One million rows works in both engines: the last row is reachable and only about 50 rows
   exist in the DOM regardless of total size. WebKitGTK's p95 here is with software GL in a
   virtual display; real hardware is expected to be smoother, to be confirmed on macOS.
2. Both engines cap a scrollable element at 33,554,428 px (2^25). At 28 px rows that is
   **1,198,372 rows**. Crawls above that need *scaled scrolling*: the scroller's physical
   height is capped and scroll position maps proportionally to a row index. TanStack Virtual
   does not do this; spec 007's plan must add it (a small custom range calculator over the
   same column model). Canvas grids such as Glide also rely on a native scroller, so this is
   not a reason to switch.

**Decision**: TanStack Table v9 (column model: order, visibility, sizing, pinning, persisted
per tab) + TanStack Virtual for rows, data paged from the engine. Sorting, filtering and
search stay in the engine (spec 007 SC-004). Scaled scrolling above about 1.19M rows is
recorded as a requirement for 007's plan. Glide Data Grid stays the fallback but is not needed.

## R4. Window chrome per platform

**Decision**: macOS: `titleBarStyle: "Overlay"` and `hiddenTitle: true`; our toolbar takes
the title bar row with a left inset for the window buttons, and the toolbar is a drag region
(`data-tauri-drag-region`). Windows and Linux: system decorations stay on (standard caption
buttons, FR-026); the toolbar is the first row of the client area.

**Materials (FR-027, a MAY)**: deferred to a follow-up task. The `window-vibrancy` crate
(vibrancy or Liquid Glass on macOS 26+, Mica on Windows 11) needs a transparent window and,
on macOS, `macOSPrivateApi`, which affects App Store eligibility and every content surface's
background. The shell already paints the sidebar from its own token, so adding the material
later is a background swap behind one class.

## R5. Fonts

**Decision**: the platform interface font through the system font stack (SF Pro on macOS,
Segoe UI Variable on Windows, the desktop default on Linux), and the platform monospaced
stack (`ui-monospace`, SF Mono, Cascadia Mono, Menlo, Consolas) for URLs, headers and
source. No web fonts, so the CSP needs no font host and nothing is downloaded. Tabular digits
via `font-variant-numeric: tabular-nums` on numeric cells and the status bar.

## R6. Command palette

**Decision**: `cmdk` 1.1 inside a Base UI Dialog. Commands are plain data (`id`, `title`,
`group`, `shortcut`, `enabled`, `run`) in one registry that the palette and keyboard
shortcuts read (Key Entity "Command").

## R7. Theme and layout persistence

**Decision**: theme preference and pane state in `localStorage` under one versioned key.
They are per-machine UI state, not crawl configuration, so they do not belong in the
engine's profile store. If the storage read fails, the defaults apply.
