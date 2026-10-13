# Quickstart: Results Interface

```sh
# A synthetic crawl file for scale work (no network needed)
cargo run --release -p spiderlens-results --example gen_crawl -- 1000000 /tmp/1m.spiderlens
# Timings for open, sort, search, counts and export on that file
cargo run --release -p spiderlens-results --example bench -- /tmp/1m.spiderlens
# A real crawl, then a view exported from the command line
cargo run -p spiderlens -- crawl example.com --out /tmp/example.spiderlens
cargo run -p spiderlens -- export /tmp/example.spiderlens --tab page-titles --filter duplicate
# Desktop (see docs/CONTRIBUTING.md), then ⌘O and open a .spiderlens file
pnpm dev
```

The issues fixture site (seeded titles, redirects, 404s, a robots.txt rule,
an exclude rule) is built inside `crates/results/tests/issues.rs` and
crawled for real by the tests; there is no separate binary.

## Scenario-to-test map

Rust tests live in `crates/results/tests/` unless noted; UI tests in
`ui/src/results/Results.test.tsx`. Scenarios that need spec 005's
extraction are marked "after 005" and covered when it lands (T044).

| Scenario | Test |
|---|---|
| US1-1 progress figures and states | `ui/src/crawl/CrawlRun.test.tsx` |
| US1-2 rows appear while crawling, anchor and selection kept | `live.rs` `live_refresh_keeps_anchor_and_respects_view`; UI `live_crawl_refreshes_the_view_and_keeps_the_selected_row` |
| US1-3 each tab lists exactly its URLs; counts agree | `issues.rs` `us2_filters_list_exactly_the_seeded_urls_and_counts_agree` (base tabs; content tabs after 005) |
| US1-4 reopened file identical | `live.rs` `reopen_gives_identical_views`; UI `open_crawl_file_browses_its_results` |
| US1-5, US1-6 elapsed time, failures by class | engine `crates/engine/tests/crawl.rs` `spec007_active_time_and_failures_by_class` |
| US2-1, US2-2 missing and duplicate titles | `issues.rs` `us2_filters_list_exactly_the_seeded_urls_and_counts_agree`; `norm_ws` unit tests |
| US2-3, US2-4 sort and search (Hebrew included) | `issues.rs` `us2_scope_sort_and_search`; UI `header_click_sorts_then_reverses_then_clears`, `search_filters_the_view` |
| US2-5 columns per tab | UI `not_collected_columns_are_marked`; `columnPrefs.ts` |
| US2-6 live with filter, search, sort | `live.rs` `live_refresh_keeps_anchor_and_respects_view` |
| US3-1, US3-2, US3-5 redirects, inlinks, failed URL | `issues.rs` `us3_detail_redirects_inlinks_and_failure` |
| US3-3, US3-4 images, headings, raw HTML | after 005 |
| US3-6 keyboard follows selection | UI `keyboard_moves_the_selection_and_feeds_the_detail_pane` |
| US4-1, US4-4 export equals view, cancel leaves no file | `issues.rs` `us4_export_matches_view_and_cancel_leaves_nothing`; CLI `crates/cli/tests/export.rs` `cli_csv_equals_desktop_export_for_the_same_view` |
| US4-2, US4-3 copy rows | `issues.rs` (copy), UI `selection_copies_rows_with_a_header` |
| US4 export UI | UI `export_shows_progress_then_where_the_file_went` |
| US5, SC-003..009 | `examples/bench.rs`; numbers in research.md (Mac run pending, T040) |
| Edge: formula injection, quoting | `export.rs` `quoting_and_formulas`, `fields_round_trip` (property test) |
| Edge: older file or missing data | `issues.rs` `unavailable_filters_are_refused_with_a_reason`; CLI `unknown_ids_and_unavailable_filters_have_their_own_exit_codes` |
| Scope tester (spec 003 FR-016) | CLI `scope_test_explains_each_url`; UI `scope_tester_explains_each_url_with_the_current_settings` |
