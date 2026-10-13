# Research: Accessibility and Mobile Usability Checks

**Feature**: [spec.md](./spec.md) | **Date**: 2026-10-13

This file records the technology choices the spec leaves to planning, the hooks this feature needs from specs 005, 009 and 014, and the decisions the owner is asked to confirm. Each decision has a default the spec already assumes.

## R1. Which accessibility rule engine

| Option | Licence | Fit | Verdict |
|---|---|---|---|
| **axe-core** (Deque) | MPL-2.0 | Runs as one JavaScript file inside the page; about 100 checks tagged with WCAG 2.0, 2.1, 2.2 levels A, AA, AAA and best practice; per-check impact (critical, serious, moderate, minor); a "needs review" (incomplete) result instead of guessing; widely used (Screaming Frog's accessibility audit runs axe 4.10, Lighthouse's accessibility category is axe, browser devtools extensions). Current release 4.14.0 on npm (checked 2026-10-13). | **Chosen** |
| IBM Equal Access checker engine | Apache-2.0 | Also in-page, good WCAG coverage, but different rule names and result model from what SEO users see in other tools, and a smaller user base. | Rejected: results would not match what users compare against. |
| Siteimprove Alfa | MIT | Rust-friendly licence, but runs in Node against a serialised page and needs its own layout model for contrast and sizes; weaker on rendered layout. | Rejected: does not see the real rendered layout. |
| Lighthouse | Apache-2.0 | Needs Node and launches its own browser run per page; its accessibility audits are axe underneath. | Rejected: a second page load per page and a Node dependency, against the speed priority and the "one request per page" rule of spec 014. |
| Our own rules in Rust | ours | Only possible for the static subset; the rendered checks need computed styles and accessible names from the browser. | Used for the static subset only (R4). |

**Licence**: MPL-2.0 is a file-level copyleft. Shipping the unmodified `axe.min.js` inside the MIT-licensed app is allowed; we must keep its licence notice, ship it as a separate file (embedded as a resource is fine) and make its source available (a link to the exact upstream tag satisfies this for unmodified files). We do not modify it; configuration is passed through its API. The notice goes into the About screen's licences list.

**Size**: the minified file is about 0.5 MB; embedded once in the engine crate, shared by the app and the CLI. The binary budget of spec 014 (8 MB per binary) is the check.

**Version pinning**: one axe version per SpiderLens release, recorded in the crawl file (FR-008) and in the rule catalogue (FR-019). Upgrading axe is a normal PR that regenerates the catalogue entries from axe's own rule metadata (`axe.getRules()`), bumps the catalogue version and runs the fixture suite.

## R2. Hook into the rendering stage of spec 014 (agreed with the "JS rendering" thread, 2026-10-13)

The 014 thread accepted this hook and will publish it in `contracts/render-crate.md` with its first code PR.

Spec 014's renderer (`crates/render`, plan D1) loads each page in a fresh browser context, waits for the settle rule, serialises the HTML and optionally takes a screenshot, then closes the context. Accessibility needs one step between "settled" and "closed". Proposal, generic so specs 015 and 016 can use it too:

- `RenderJob` gains `probes: Vec<PageProbe>`. A `PageProbe` is `{ name, script, args_json, budget }`. Empty by default, so a crawl without probes runs exactly the spec 014 code path (no extra CDP calls, no allocation).
- After the settle rule, after the HTML serialisation and after the screenshot (FR-009 of this spec: probes cannot change what 014 stores), the renderer runs each probe in an **isolated world** (`Page.createIsolatedWorld`), so page scripts cannot see or tamper with the probe and the probe's globals do not leak into the page. The probe's script returns a JSON string.
- Each probe has its own time budget (`Runtime.evaluate` with `awaitPromise` and a timeout), and all probes of one page share a total cap (default 5 s, set from this feature's FR-005 when it is on); a timeout returns `ProbeOutcome::TimedOut { spent }` and never fails the render. The maximum result size is capped (4 MB) and anything larger is `ProbeOutcome::TooLarge`; any other error is `ProbeOutcome::Failed(reason)`.
- `RenderOutcome` gains `probes: Vec<(name, ProbeOutcome)>`. The engine's `on_rendered` hands each outcome to the feature that registered the probe, which parses and stores it.
- `RenderJob.load_types` overrides which resource types load for that job. When accessibility or mobile checks are on, this feature adds fonts and images (FR-012); everything else about resource blocking stays as spec 014 decides.
- Each page starts in a clean browser state seeded from the crawl's cookie jar and proxy (agreed with 014 for spec 025), so checks see the same page the crawl saw.

Two probes from this feature: `a11y` (axe with the chosen tags and result limits) and `mobile_layout` (a small script of our own measuring document width against the window, computed font sizes of visible text nodes weighted by characters, and interactive element boxes with WCAG 2.5.8 spacing; axe's own `target-size` check gives the WCAG verdict, our script gives the numbers for the Mobile tab).

*Analogy*: like a middleware chain in a web framework, but with exactly one well-defined slot after the response is built: the renderer stays the owner of the page, a probe only reads.

## R3. axe configuration

- `runOnly` by tags from the WCAG target (FR-003) plus `best-practice`, but **all** tags are run and stored, so switching the target later (FR-020) does not need a new check; the target only decides what counts. The extra cost of running all tags is small, because most checks are shared across levels.
- `resultTypes: ['violations', 'incomplete']`: passes and inapplicable results are not collected (large and not needed), which keeps the result small and fast.
- `selectors: true`, `ancestry: false`, `xpath: false`, `elementRef: false`; HTML excerpts cut to 300 characters on our side; node lists capped per check (20) and per page (500) before leaving the browser.
- `iframes: true` for same-origin frames; cross-origin frames are reported "not checked" (spec edge case).
- `performanceTimer: false`; our own budget wraps the call (FR-013).
- Element positions for screenshot outlines (FR-024): one `getBoundingClientRect` per stored node after axe returns, in the same probe.

## R4. Static checks from the spec 005 parse

Spec 005 already walks the parsed document once (`crates/engine/src/extract/walk.rs`, `scraper` on `html5ever`). The static checks become one more collector in that walk, created only when the level is not "Off" (an `Option` in the extraction context, so "Off" costs one branch per page). The collector sees each element once and keeps a few counters and small evidence lists: labels by `for` id, inputs and their ids, `aria-label`/`aria-labelledby`/`title` presence, buttons and links with empty text after the walk, frame titles, `tabindex` and `accesskey` values, the viewport meta content, deprecated and plugin elements. Label association that needs the whole document (a label later in the document pointing at an earlier input) is resolved at the end of the walk from the id map.

No second parse and no stored-HTML read during the crawl (FR-028). Running static checks on an existing crawl (User Story 5) re-runs extraction from stored HTML for this collector only.

Spec 009 rules already cover missing alt, missing language, non-sequential headings and missing title (FR-007): they get WCAG tags in their catalogue entries; this feature adds only the other static checks.

## R5. Storage

New tables in one additive step of `crates/engine/src/crawl/migrate.rs`, number assigned at merge time (latest step on main plus one, no gaps, nullable columns, no hand-written ALTER), after spec 006 PR #27 and spec 014's step:

- `a11y_audits` (one row per checked page: level, state, reason, engine version, check time, counts by impact, needs review count, lowest level failed)
- `a11y_results` (page, check id, kind violation or needs review, source static or full, impact, WCAG tags as a small text list, node count)
- `a11y_nodes` (result, selector, excerpt, data JSON, x, y, width, height)
- `mobile_measures` (page, viewport content and state, window kind, measured state and reason, content width, widest selector, small text share, small target count, examples JSON)
- Meta keys: `a11y_engine` (name and version), `a11y_settings`, and `data_features` gains `accessibility_static`, `accessibility_full`, `mobile_layout`.

Results tabs (spec 007) and rules (spec 009) are SQL over these tables, as for other tabs; the WCAG target is a stored setting used in the filter conditions, like other thresholds.

## R6. Performance

| Situation | Expected cost | Check |
|---|---|---|
| Level Off | One `Option` branch per page in extraction; no tables written (tables exist empty in new files). | SC-002 benchmark, median of 7 runs. |
| Static | One extra collector in the single DOM walk; a few small inserts per page, batched with the existing 200 ms commits (spec 008). | SC-003 benchmark (at most 3%). |
| Full | axe on a typical page takes from tens of milliseconds to about a second *(estimate, measured in the plan's spike)*, run in the render slot, so it lowers rendered pages per second, not the raw fetch rate. | SC-004 on the rendered fixture site. |
| Mobile layout | One small script per page, a few milliseconds *(estimate)*. | Part of SC-004. |

## Decisions for the owner

Each has a default the spec already assumes.

1. **Default level**: (a) "Static checks" on by default, falling back to "Off" if the benchmark shows more than 3% cost *(default)*; (b) "Off" by default.
2. **Engine**: (a) axe-core, pinned per release *(default)*; (b) IBM Equal Access.
3. **WCAG target default**: (a) WCAG 2.2 AA *(default, what the European Accessibility Act points to through EN 301 549 and the most common contract level)*; (b) WCAG 2.1 AA.
4. **Fonts and images during rendering**: (a) load web fonts and images when accessibility or mobile checks are on, even though spec 014 blocks them by default *(default, measured sizes and contrast are wrong otherwise; costs more bandwidth on the site)*; (b) keep spec 014's blocking and accept less accurate results.
5. **Mobile layout on desktop-rendered crawls**: (a) not measured, no second render *(default, keeps rendering cost unchanged)*; (b) a second render at the mobile size when mobile checks are on.

## Sources

- Screaming Frog accessibility audit tutorial (engine, WCAG filters, needs review, exports): https://www.screamingfrog.co.uk/seo-spider/tutorials/how-to-perform-a-web-accessibility-audit/
- axe-core 4.5 and WCAG 2.2 support: https://www.deque.com/blog/axe-core-4-5-first-wcag-2-2-support-and-more/
- axe-core on npm (version 4.14.0, MPL-2.0, checked 2026-10-13): https://registry.npmjs.org/axe-core/latest
- Lighthouse 13 removed the `font-size` audit ("no signals that this remains an SEO concern today") and replaced `viewport` with an insight: https://developer.chrome.com/blog/lighthouse-13-0
- WCAG 2.2, success criterion 2.5.8 Target Size (Minimum): https://www.w3.org/TR/WCAG22/#target-size-minimum
