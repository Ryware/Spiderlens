# Quickstart: Crawl Frontier and Scope

```sh
cargo run -p spiderlens -- crawl example.com --max-urls 500 --out example.spiderlens
cargo run -p spiderlens -- report example.spiderlens                 # summary
cargo run -p spiderlens -- report example.spiderlens --not-crawled   # every skipped URL and why
cargo run -p spiderlens -- report example.spiderlens --reason blocked_by_robots --json
cargo run -p spiderlens -- crawl --resume example.spiderlens         # after Ctrl-C (exit 5)
cargo run -p spiderlens -- scope test --start https://example.com/ --fetch-robots \
  https://example.com/private/x https://blog.example.com/
```

## Scenario-to-test map

Engine tests are in `crates/engine/tests/crawl.rs` unless a module is named;
CLI tests are in `crates/cli/tests/crawl.rs`.

| Scenario | Test |
| --- | --- |
| US1-1 spellings fetched once | `us1_1_spellings_dedupe_and_external_not_followed`, `config::url_norm` tests |
| US1-2 shortest depth | `us1_2_depth_is_shortest_click_distance`, `guard_2000_pages_once_each_at_shortest_depth` |
| US1-3 breadth first | `us1_3_breadth_first_order` |
| US1-4 external checked, not followed | `us1_1_spellings_dedupe_and_external_not_followed` |
| US1-5/6 www and subdomains | `us1_5_6_www_internal_subdomains_by_setting` |
| US2-1/2 robots group and rule shown | `us2_1_2_robots_group_and_rule_shown`, `robots` unit tests |
| US2-3 crawl-delay | `us2_3_crawl_delay_spaces_requests_to_one_host` |
| US2-4/5 robots 404 and 5xx | `us2_4_5_robots_missing_allows_and_server_error_blocks` |
| US2-6 ignore but report | `us2_6_ignore_but_report_crawls_and_records_rule` |
| US3-1, 4, 6, 7, 8 | `us3_rules_limits_and_params` |
| US3-2 include spares start URL | `us3_2_include_spares_start_url` |
| US3-3 exclude beats include | `crawl::scope::tests::exclude_wins_and_names_pattern_include_spares_seeds` |
| US3-5 URL limit | `us3_5_url_limit` |
| US4-1/2 one reason each, counts add up | `Done::assert_invariant` in every engine test, `us4_every_reason_once_and_stop_resume` |
| US4-3 scope tester | CLI `scope_test_explains_each_url` |
| US4-4 stop and resume | `us4_4_stop_then_resume_crawls_each_url_once` |
| US4-5 CLI output and file | CLI `crawl_summary_matches_schema_and_counts_reasons`, `report_lists_not_crawled_urls_with_reasons` |
| US5-1..4 nofollow | `us5_nofollow_handling` |
| US6-1..4 sitemaps | `us6_sitemaps_from_robots`, `crawl::sitemap` unit tests |
| Spec 002 US1-4 start redirect | `start_redirect_to_www_is_adopted` |
| Spec 002 US2-3 list mode | `list_mode_depth_zero_and_hosts_internal` |

## Not covered yet

- SC-006..SC-008 (100,000-page throughput, flat memory, 1,000,000 URLs) need
  the large fixture and a benchmark run; the bounded queue window is tested
  at small scale (`crawl::window` tests).
- SC-009 (desktop and CLI parity) needs the desktop crawl commands in
  contracts/desktop-commands.md, which are not wired into the app yet.
