# Research: Appearance, Brightness Levels and Accent Palettes (spec 018)

**Date**: 2026-10-13. Prototype: https://claude.ai/artifact/5Fr3kBynymfz6LoxVL7gZ3

## R1. What the recording shows

The recording (7 s, Decim admin at admin.decim.dev) shows one control. A sun button left of the user menu opens a popover. Inside it a vertical slider has a track shaded from white to black and six named stops: Light, Soft, Island, Dark, Deep, Midnight, each with an icon. Dragging the thumb snaps to stops and re-themes the page live during the drag. The header icon changes with the stop (sun, half sun and moon, moon). Measured from the frames, relative to each other: Soft is a slightly warmer, slightly dimmer light; Island keeps the window frame and sidebar dark around a light content panel; Dark, Deep and Midnight step down to black.

## R2. How it fits the token system

**Decision**: two independent axes on the root element, both as data attributes. `data-level` selects one of six neutral token blocks; `data-accent` selects one accent block, in a light-tone and a dark-tone variant. A level block sets a `data-tone` value (light or dark) that the accent blocks key on. Island sets tone light on the root and tone dark on the toolbar, sidebar and status bar, so the same accent block applies there with dark values because custom properties cascade into the subtree.

**Why**: components already use only token names (spec 017 FR-014), so no component code changes to get new colours. Hand-written blocks keep the token file readable and keep the existing test's parser working.

**Alternatives rejected**:
- Computing accents at runtime from a hue (`oklch(from ...)` or `calc` on L, C, H variables). Less code, but values could not be contrast-tested from the file, and relative colour syntax support in the Linux WebKitGTK build used by Tauri is not certain.
- One block per combination (36 blocks). This is what the prototype does for speed; in the app it would be 36 places to edit.

**Size of the token file**: today 2 blocks. After: 6 level blocks of about 30 tokens and 6 palettes with 2 variants of 6 tokens, about 300 lines in total.

## R3. Brightness steps (proposed values, OKLCH, hue 188 shown for teal)

| Token | Light | Soft | Dark | Deep | Midnight |
|---|---|---|---|---|---|
| bg | 0.985 0.004 | 0.945 0.009 h80 | 0.175 0.007 | 0.13 0.006 | 0 0 0 |
| surface | 1 0 0 | 0.965 0.009 h80 | 0.205 0.007 | 0.155 0.006 | 0 0 0 |
| surface-2 | 0.962 0.004 | 0.925 0.009 h80 | 0.245 0.007 | 0.195 0.006 | 0.16 0.003 |
| sidebar | 0.955 0.004 | 0.925 0.009 h80 | 0.19 0.007 | 0.14 0.006 | 0 0 0 |
| line | 0.89 | 0.855 | 0.30 | 0.25 | 0.24 |
| control-border | 0.62 | 0.58 | 0.56 | 0.53 | 0.52 |
| fg / fg-2 / fg-3 | 0.22 / 0.42 / 0.50 | 0.24 / 0.40 / 0.465 | 0.95 / 0.78 / 0.68 | 0.93 / 0.76 / 0.665 | 0.92 / 0.75 / 0.66 |

Light and Dark are today's values. Soft's meaning colours sit 0.045 lower in lightness than Light's so they still reach 4.5:1 on the warmer, darker paper. Island is Light plus Dark, so it needs no values of its own. Midnight separates panes with lines only, as X's Lights out and Discord's Onyx do.

## R4. Accent palettes (proposed)

| Palette | Hue | Light-tone accent | Dark-tone accent | Backed by |
|---|---|---|---|---|
| Teal (default) | 188 | 0.50 0.095 | 0.80 0.10 | WGSN + Coloro 2026 Transformative Teal; Behr 2026 Hidden Gem; Pantone SS27 Waterfall; the app icon |
| Cobalt | 266 | 0.50 0.16 | 0.76 0.11 | WGSN + Coloro 2027 Luminous Blue; Dulux 2026 blues; Pantone SS27 Princess Blue |
| Lavender | 295 | 0.50 0.15 | 0.78 0.11 | Pantone SS27 Lavender Oil |
| Orchid | 345 | 0.52 0.16 | 0.78 0.12 | WGSN S/S27 Pop Pink; Pantone SS27 Rosy Future |
| Celery | 122 | 0.50 0.11 | 0.86 0.14 | Sherwin-Williams and HGTV Home 2027 Celery; Behr 2027 Grounded; Pantone SS27 Daiquiri Green |
| Cloud | none | 0.30 0.01 (graphite) | 0.93 0.012 (off-white) | Pantone 2026 Cloud Dancer; Fluent 2 and Linear keep the accent restrained |

Hover is 0.05 darker (light tone) or lighter (dark tone). Accent text is 0.05 darker (light) or 0.02 lighter with 8% less chroma (dark). The label on the accent is near white on light tones and 0.20 lightness on dark tones. Soft fill and selection are the accent at 12% and 10% (light) or 16% and 13% (dark), as today.

**Contrast**: every palette in every step passes all spec 017 rules (checked with the same OKLCH to sRGB conversion the token test uses). Weakest accent ratios: 5.5:1 (Teal, light steps) to 14.7:1 (Cloud, dark steps). Teal's light value is very slightly outside sRGB, as it is today; browsers clip it and the test already accepts it.

**Left out on purpose**:
- Orange (WGSN Energy Orange, Dutch Boy Deep Rooted): too close to the warning amber.
- Steel or slate blue (Valspar Cottage Door, Glidden Artifact 2027): too close to the 3xx redirect blue.
- Clay and terracotta (Glidden 2026, Graham & Brown 2027): too close to the error red.
- Brown (Benjamin Moore 2026 Silhouette): reads as disabled on dark steps.

Celery is the closest to a status colour (33 degrees from the 2xx green). It stays in because the 2027 picks lean green, but the owner may prefer to drop it.

## R5. Persistence

**Decision**: keep appearance in the per-machine UI state in local storage (spec 017 R7), moving the stored record from version 1 to version 2: `theme` becomes `level` (`"system"` or a step), plus `systemLight`, `systemDark` and `accent`. A stored version 1 record maps `light`, `dark` and `system` across, so nobody loses their choice.

**Why not the Rust config crate**: the config crate holds crawl profiles, which are shared with the command-line tool and exported to files. Appearance means nothing to the engine or the CLI, and putting it in a profile would make switching profiles change the colours. Rust changes are therefore small: none in config or engine.

**First paint**: today the theme is applied from a React effect, which can show one frame of the default. The bootstrap reads the stored record and sets the data attributes synchronously in `main.tsx` before React renders. The Tauri window's background colour is set from the same record so the native window does not flash before the page loads.

## R6. Native window and Rust side

- Native appearance: the UI calls the Tauri window API to set the window theme to light or dark from the step's tone. This needs the `core:window:allow-set-theme` permission in `app/src-tauri/capabilities/default.json`. That is the only Rust-side change.
- Following the system: `prefers-color-scheme` already reports the OS setting in the webview; `useTheme` already listens to it.

## R7. Trends that shaped the set (2026 and 2027)

- **Colours of the year 2026**: Pantone Cloud Dancer (an off-white); WGSN and Coloro Transformative Teal; Benjamin Moore Silhouette (charcoal brown); Sherwin-Williams Universal Khaki; Behr Hidden Gem (smoky jade); Valspar Warm Eucalyptus; Graham & Brown Divine Damson; Glidden Warm Mahogany; Dulux blues.
- **2027 so far**: WGSN and Coloro Luminous Blue (announced April 2025) with S/S27 key colours Energy Orange, Pop Pink, Meadowland Green and Clay; Sherwin-Williams and HGTV Home Celery; Behr Grounded (olive); Valspar Cottage Door and Glidden Artifact (steely and smoky blues); Dutch Boy Deep Rooted (earthy orange); Graham & Brown Rose Earth. Pantone 2027 is not announced yet (normally early December); its SS27 runway report lists Princess Blue, Waterfall, Daiquiri Green, Dragon Fire, Lavender Oil and Rosy Future. Benjamin Moore 2027 is not announced yet.
- **Direction**: teal leads 2026; 2027 leans to luminous and steely blues, herbal and olive greens, and warm pinks and clays.
- **Operating systems and design systems**: macOS and iOS 26 let users tint icons and folders with any colour and, since 26.1, choose Clear or Tinted Liquid Glass. Material 3 Expressive adds higher-chroma dynamic colour and three contrast levels. Fluent 2 keeps the brand colour to buttons and selected states and lets users personalise it. GitHub Primer has light, dark, dark dimmed, high contrast and colour-blind themes. Linear generates its themes from base, accent and contrast in LCH and has reduced how much the accent tints the greys.
- **Several dark levels**: X has Dim and Lights out (true black, for OLED). Discord has Light, Ash, Dark and Onyx (true black), and lets users choose which dark theme follows the OS. GitHub has Dark and Dark dimmed with separate day and night choices. The six-step ladder fits this pattern; Follow system with a choice of light and dark step follows Discord and GitHub.
- **Accessibility**: WCAG 2.2 remains the baseline (4.5:1 text, 3:1 components). APCA was taken out of the WCAG 3 drafts and WCAG 3's contrast method is not decided, so the token test keeps WCAG 2 ratios. Follow-ups worth a later spec: a High contrast option (7:1, like Primer) and a colour-blind status set based on Okabe-Ito.

## Sources

1. Pantone 2026 Cloud Dancer: https://www.vistaprint.com/hub/pantone-color-of-the-year
2. WGSN and Coloro 2026 Transformative Teal: https://mr-mag.com/wgsn-and-coloro-announce-colour-of-the-year-2026-transformative-teal-and-the-key-colours-for-a-w-26-27
3. WGSN and Coloro 2027 Luminous Blue and S/S27 key colours: https://www.wgsn.com/en/wgsn/press/press-releases/wgsn-and-coloro-reveal-colour-year-2027-luminous-blue-and-s-s-27-key
4. Benjamin Moore 2026 Silhouette: https://www.businesswire.com/news/home/20251016154282/en/Benjamin-Moore-Names-Silhouette-AF-655-as-its-Color-of-the-Year-2026
5. Colours of the year 2026 round-up: https://www.homesandgardens.com/interior-design/colors-of-the-year-2026
6. 2027 colours of the year round-up: https://www.younghouselove.com/2027-color-of-the-year/
7. Behr 2027 Grounded: https://www.homesandgardens.com/decor/paint/behr-color-of-the-year-2027
8. Pantone NYFW SS27 top 10: https://fashionunited.com/news/fashion/pantone-nyfw-spring-summer-2027-color-trend-report-the-top-10-colors/2026092174743
9. macOS 26 customisation: https://9to5mac.com/2025/08/07/macos-26-tahoe-customization-options/
10. iOS and macOS 26.1 Clear or Tinted: https://www.macworld.com/article/2947565/ios-26-1-beta-4-does-the-unthinkable-you-can-control-how-glassy-liquid-glass-should-be.html
11. Material 3 Expressive colour: https://www.androidauthority.com/material-3-expressive-color-3560841
12. Fluent 2 colour: https://fluent2.microsoft.design/color
13. GitHub Primer colour: https://primer.style/foundations/color/overview
14. GitHub dark dimmed: https://github.blog/changelog/2021-03-16-dimmed-theme-beta-for-github-com/
15. Linear UI redesign: https://linear.app/now/how-we-redesigned-the-linear-ui
16. Discord Ash, Dark, Onyx: https://discord.com/blog/improving-mobile-with-squircles-styles-and-spacing
17. Twitter Lights out: https://www.techradar.com/news/twitter-apps-dark-mode-gets-even-darker-with-an-oled-ready-update
18. WCAG 3 contrast status, April 2026: https://adrianroselli.com/2026/04/wcag3-contrast-as-of-april-2026.html
19. Radix Colors scale: https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale
20. Adobe Leonardo: https://github.com/adobe/leonardo
