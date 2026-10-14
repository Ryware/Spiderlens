# Desktop Commands Contract: Content Checks (spec 027)

Tauri commands; types exported to `ui/src/generated` with ts-rs. Errors use the existing `CommandError` shape.

## Part A

| Command | Input | Output |
|---|---|---|
| `serp_measure` | `{ title?: string, description?: string }` | `SnippetMeasure` pair (used by tests and CLI parity; the UI measures locally with `serpWidths.json`) |
| `serp_proposal_get` | `{ urlId }` | `{ title?, description?, updatedAt? }` |
| `serp_proposal_set` | `{ urlId, title?, description? }` | `()`; empty strings clear |
| `serp_proposals_export` | `{ path }` | `{ rows }` |
| `serp_list_open` | `{ path }` or `{ text }` | `{ viewId, rows, rejected: [{ line, reason }] }` |

Tabs `page-titles` and `meta-description` gain columns `title_px`, `title_trunc_desktop`, `title_trunc_mobile`, `desc_px`, `desc_trunc_desktop`, `desc_trunc_mobile` and filters `title_px_over_desktop`, `title_px_over_mobile`, `title_px_below`, `desc_px_over_desktop`, `desc_px_over_mobile` (shared with the rules, 009 FR-005).

## Part B

| Command | Input | Output |
|---|---|---|
| `segments_validate` | `Segment` | `{ ok } | { clause, message }` |
| `segments_set` | `{ segments: Segment[] }` | `{ counts: [{ segId, urls }], noSegment }` (applies to the profile and the open crawl) |
| `segments_counts` | `{}` | as above |
| `segment_of` | `{ url }` | `{ segId? }` (scope tester) |

Existing `view_query`, `counts`, `issues_summary` and `export_tab` accept an optional `segment: number | "none"`.

## Part C

Uses the existing crawl configuration commands for the new settings; the `pdfs` tab is a normal `TabDef`. `detail` gains a `pdf` section.

## Part D

| Command | Input | Output |
|---|---|---|
| `spell_dictionaries` | `{}` | `{ installed: Dictionary[], available: ManifestEntry[] }` |
| `spell_dictionary_add` | `{ lang }` | progress events `spell://download`, then `Dictionary` |
| `spell_dictionary_add_file` | `{ aff, dic }` | `Dictionary` |
| `spell_dictionary_remove` | `{ lang }` | `()` |
| `spell_run` | `{ grammar?: boolean }` | progress events `spell://progress { checked, total }` |
| `spell_cancel` | `{}` | `()` |
| `spell_ignore` | `{ word, scope: "once" | "site", urlId? }` | `()` |

`detail` gains a `spelling` section (findings in context).

## Part E

`detail` gains an `amp` section; the `amp` tab is a normal `TabDef`.
