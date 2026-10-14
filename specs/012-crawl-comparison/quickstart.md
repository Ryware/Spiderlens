# Quickstart: Crawl Comparison (test map)

Fixture pairs (crates/test-fixtures `compare.rs`): **site v1 / v2** (pages
added and deleted, titles changed, whitespace-only change, a title removed,
a page turned into a redirect, canonical changed, noindex added, word count
±5% and −30%, text changed with same headings, JSON-LD added, inlinks
changed, depth changed), **migration pair** (staging host + old paths →
production host + new paths, one collision, one unmatched page, one
redirect chain), **limit pair** (current crawl stopped by max URLs),
**old-format file** (no extraction).

| Scenario | Test |
|---|---|
| US1-1 New and Missing on Internal › All | `compare::tests::us1_new_missing` |
| US1-2 title removed: Added to Missing title, not New/Missing | `us1_added_removed_title` |
| US1-3 200 → 301 moves between Response codes filters | `us1_status_moves` |
| US1-4 sort by change, hide unchanged | Vitest `CompareOverview.test.tsx` |
| US1-5 group list with previous and current columns, export | `view::tests::group_export` + Playwright `compare.spec.ts` |
| US2-1..6 element changes, whitespace, thresholds, canonical + indexability, content fingerprint, settings update without rescan | `elements::tests::*` + Vitest `ChangeDetection.test.tsx` |
| US3-1..5 issue deltas | `issues::tests::*` (with 009) |
| US4-1 host mapping suggested | `matching::tests::suggest_host` + Vitest `CompareSetup.test.tsx` |
| US4-2 path mapping matches | `matching::tests::path_rules` |
| US4-3 mapping tester | `matching::tests::tester` + Vitest |
| US4-4 collision | `matching::tests::collision` |
| US4-5 migration view | `counts::tests::migration_lists` |
| US5-1..5 CLI summary, JSON, export, fail conditions, settings file | `crates/cli/tests/compare.rs` |
| US6-1 progress and cancel | `lib::tests::cancel_cleans_up` |
| US6-2 files untouched, crawl can open for writing at the same time | `lib::tests::files_untouched` (bytes + mtime, SC-004) |
| US6-3 older and newer format files | `lib::tests::formats` |
| US6-4 crawl speed while comparing | bench `compare_while_crawling` (SC-008) |
| Edge: same file, running crawl snapshot, incomplete crawl Not reached, config diffs, rendering on one side, low overlap warning, file deleted mid-run | `edge::*` |

Manual check on a real site: crawl a site twice with a change in between,
open Compare crawls from the start screen, and walk through Overview,
Change detection and an export.
