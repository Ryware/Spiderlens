# Data Model: Accessibility and Mobile Usability Checks

All tables arrive in one additive step in `crates/engine/src/crawl/migrate.rs` (spec 006 mechanism). **The step number is taken at merge time: the latest step on main plus one, no gaps.** Columns are nullable unless they are part of a key; nothing in existing tables changes. Conventions are those of spec 005's data model; `url_id` always equals `urls.id`. Tables are created in every new file and stay empty when the level is Off.

## Settings: `CrawlConfig.accessibility` (`crates/config/src/model.rs`)

| Field | Type | Default | Rule |
|---|---|---|---|
| `level` | `off` \| `static` \| `full` | `static` | FR-001. `full` with rendering off runs static and gives a notice (FR-002) |
| `wcag.version` | `2.0` \| `2.1` \| `2.2` | `2.2` | FR-003 |
| `wcag.level` | `a` \| `aa` \| `aaa` | `aa` | FR-003 |
| `best_practice` | bool | `true` | FR-003 |
| `mobile` | bool | `true` | FR-004 |
| `budget_secs` | integer | `5` | 1 to 30 (FR-005) |
| `small_text_px` | integer | `12` | 6 to 24 (FR-016) |
| `small_text_share` | integer (%) | `40` | 1 to 100 (FR-016) |
| `tap_target_px` | integer | `24` | 16 to 64 (FR-016) |
| `overflow_px` | integer | `5` | 0 to 100 (FR-016) |

Validation codes: `accessibility.budget_secs.range` and so on (spec 002 style). Older configurations without the section load with these defaults.

## Meta keys

| Key | Value |
|---|---|
| `a11y_engine` | JSON `{"name": "axe-core", "version": "4.14.0"}` when Full ran; absent otherwise |
| `a11y_settings` | JSON of the `accessibility` settings used for the last run (also inside the stored config; kept separately because FR-031 can run checks later with other settings) |
| `data_features` | gains `accessibility_static`, `accessibility_full`, `mobile_layout` as applicable |

## `a11y_audits`: one row per checked internal HTML page

| Column | Type | Notes |
|---|---|---|
| `url_id` | INTEGER PK | |
| `static_state` | TEXT | `checked`, or NULL when static checks did not run |
| `full_state` | TEXT | `checked`, `timed_out`, `not_checked`, or NULL when Full was not selected |
| `full_reason` | TEXT | for `not_checked`: `not_rendered`, `render_failed`, `render_timed_out`, `bot_challenge`, `probe_failed`, `too_large` |
| `check_ms` | INTEGER | probe time spent (Full) |
| `critical`, `serious`, `moderate`, `minor` | INTEGER | violation counts by impact, all tags, static and full together after de-duplication |
| `needs_review` | INTEGER | incomplete results |
| `checks_failed` | INTEGER | distinct check ids with a violation |
| `elements` | INTEGER | failing elements, totals (not capped) |
| `lowest_level` | TEXT | lowest WCAG level with a violation (`2.0-a` ... `2.2-aaa`, `best_practice`), all tags |

## `a11y_results`: one row per page, check and kind

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | |
| `url_id` | INTEGER NOT NULL | index `(url_id)`, index `(check_id, kind)` |
| `check_id` | TEXT NOT NULL | axe rule id (`color-contrast`) or `static.<id>` |
| `kind` | TEXT NOT NULL | `violation` \| `needs_review` |
| `source` | TEXT NOT NULL | `static` \| `full` |
| `impact` | TEXT | `critical`, `serious`, `moderate`, `minor` |
| `wcag` | TEXT | space-separated tags as axe writes them (`wcag2aa wcag143`), or `best-practice` |
| `nodes_total` | INTEGER | failing elements before the cap |

When both sources report the same concept on one page, only the `full` row is stored and the static one is dropped (spec edge case); the mapping is the `same_as` field of each static check.

## `a11y_nodes`: capped element detail

| Column | Type | Notes |
|---|---|---|
| `result_id` | INTEGER NOT NULL | → `a11y_results.id`, index |
| `pos` | INTEGER NOT NULL | 0.. in the order the check reported them |
| `selector` | TEXT | CSS selector |
| `excerpt` | TEXT | at most 300 characters |
| `data` | TEXT | JSON, check-specific (contrast: `fg`, `bg`, `ratio`, `expected`, `font_px`, `bold`) |
| `x`, `y`, `w`, `h` | REAL | CSS px in the rendered page, Full only |

Caps: 20 per result, 500 per page (FR-010).

## `mobile_measures`: one row per page with mobile checks

| Column | Type | Notes |
|---|---|---|
| `url_id` | INTEGER PK | |
| `viewport` | TEXT | the viewport meta content as written, NULL when missing |
| `viewport_state` | TEXT | `ok`, `missing`, `not_device_width`, `zoom_disabled` |
| `plugins` | INTEGER | `object`, `embed`, `applet` count |
| `window` | TEXT | `mobile`, `desktop`, NULL when not rendered |
| `measured` | TEXT | `yes`, or the reason: `desktop_window`, `not_rendered`, `timed_out` |
| `content_px` | REAL | document width |
| `window_px` | REAL | window width |
| `widest` | TEXT | selector of the widest element |
| `small_text_share` | REAL | % of visible characters below `small_text_px` |
| `small_targets` | INTEGER | targets failing WCAG 2.5.8 |
| `examples` | TEXT | JSON list of at most 20 `{selector, w, h}` |

## Engine types (not stored as such)

- `PageInput.a11y: bool` and `PageData.a11y: Option<A11yStatic>` (`crates/engine/src/extract/a11y.rs`).
- `A11yStatic { findings: Vec<StaticFinding>, viewport: Option<Viewport> }`; `StaticFinding { check: StaticCheck, count: u32, nodes: Vec<A11yNode> }`; `A11yNode { selector: String, excerpt: String }`; `Viewport { content: String, device_width: bool, zoom_disabled: bool }`.
- `StaticCheck` ids, each with its WCAG tags, impact and `same_as` axe id:

| id | WCAG | Impact | same_as |
|---|---|---|---|
| `form_label` | 1.3.1, 4.1.2 A | critical | `label` |
| `button_name` | 4.1.2 A | critical | `button-name` |
| `link_name` | 2.4.4, 4.1.2 A | serious | `link-name` |
| `frame_title` | 4.1.2 A | serious | `frame-title` |
| `lang_invalid` | 3.1.1 A | serious | `html-lang-valid` |
| `element_lang_invalid` | 3.1.2 AA | serious | `valid-lang` |
| `zoom_disabled` | 1.4.4 AA | critical | `meta-viewport` |
| `viewport_missing` | best practice (mobile) | minor | none |
| `viewport_width` | best practice (mobile) | minor | none |
| `tabindex_positive` | best practice | serious | `tabindex` |
| `accesskey_duplicate` | best practice | serious | `accesskeys` |
| `meta_refresh_delay` | 2.2.1 A | critical | `meta-refresh` |
| `deprecated_element` | 2.2.2 A | serious | `blink`, `marquee` |
| `plugin_element` | best practice (mobile) | minor | none |
| `area_alt` | 1.1.1, 2.4.4 A | critical | `area-alt` |
| `input_image_alt` | 1.1.1 A | critical | `input-image-alt` |
