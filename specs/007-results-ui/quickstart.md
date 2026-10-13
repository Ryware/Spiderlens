# Quickstart: Results Interface

```sh
# Synthetic crawl files for scale work (no network needed)
cargo run -p spiderlens-test-fixtures --bin gen-crawl -- --urls 1000000 --out /tmp/1m.spiderlens
# Issues fixture site, crawled for real
cargo run -p spiderlens-test-fixtures --bin issues-site -- --port 8081 &
cargo run -p spiderlens -- crawl http://127.0.0.1:8081/ --out /tmp/issues.spiderlens
cargo run -p spiderlens -- export /tmp/issues.spiderlens --tab page-titles --filter duplicate
# Desktop
pnpm dev     # then ⌘O and open /tmp/1m.spiderlens
```

## Scenario-to-test map

Rust tests live in `crates/results/tests/` unless noted; UI tests in
`ui/src/results/*.test.tsx`; perf in `crates/results/benches` and
`scripts/perf-results.*` (T040). "After 005" rows are written now with
`#[ignore = "needs spec 005 extraction"]` and enabled when 005 lands.

| Scenario | Test |
|---|---|
| US1-1 progress figures and states | `CrawlRun.test.tsx` (exists, extended for stopping/active_ms) |
| US1-2 rows appear ≤ 2 s, anchor and selection kept | `live_refresh_keeps_anchor` (Rust), `Grid.anchor.test.tsx` |
| US1-3 each tab lists exactly its URLs; counts agree | `tabs_list_expected_urls` on the issues site (base tabs now; content tabs after 005) |
| US1-4 reopened file identical | `reopen_gives_identical_views` |
| US1-5 elapsed across pause and reopen | engine `active_ms_accumulates_across_resume` |
| US1-6 completeness summary | engine `summary_breaks_failures_down_by_error_class` |
| US2-1 Missing titles = 3 | `filter_page_titles_missing` |
| US2-2 Duplicate "Home" | `filter_page_titles_duplicate`, `norm_ws` unit tests |
| US2-3 sort asc/desc, indicator | `sort_numeric_natural_empty_last`, `Grid.sort.test.tsx` |
| US2-4 search ignores case | `search_unicode_case_insensitive` |
| US2-5 columns persist per tab | `ColumnMenu.test.tsx` |
| US2-6 live with filter, search, sort | `live_refresh_respects_view` |
| US3-1 redirect hops | `detail_redirect_chain_and_loop` |
| US3-2 inlinks of a 404 | `detail_inlinks` (anchor text and rel after 005) |
| US3-3 outlinks, images, headings | `detail_outlinks` now; images, headings after 005 |
| US3-4 raw HTML | after 005 |
| US3-5 failed URL detail | `detail_failure` (attempts after 004) |
| US3-6 keyboard follows selection | `Inspector.keyboard.test.tsx` |
| US4-1 export = view | `export_matches_view` (CLI and desktop paths) |
| US4-2/3 copy rows and cell | `copy_rows_tsv`, `Grid.copy.test.tsx` |
| US4-4 cancel leaves no file | `export_cancel_removes_part_file` |
| US4-5 export during crawl is a snapshot | `export_is_snapshot_during_writes` |
| US5-1..3, SC-003..009 | perf harness T040, numbers recorded in research.md |
| Edge: formula injection | `csv_neutralises_formulas` (property test) |
| Edge: copy > 10,000 rows | `Grid.copy.test.tsx` |
| Edge: older file / missing data | `unavailable_columns_and_filters` |
| Edge: empty states | `ResultsView.test.tsx` |
