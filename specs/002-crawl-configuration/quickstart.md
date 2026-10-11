# Quickstart: Crawl Configuration

```sh
cargo run -p spiderlens -- config defaults > my-profile.json
cargo run -p spiderlens -- config show example.com --speed fast --include '/blog/'
cargo run -p spiderlens -- config validate --profile my-profile.json example.com
cargo run -p spiderlens -- config test-patterns --include '/blog/' --exclude '/tag/' \
  https://a.test/blog/x https://a.test/blog/tag/y
pnpm dev   # settings screen
```

## Scenario-to-test map

"Here" means closed by this feature. Otherwise the named spec closes it and
must list it in its own plan.

| Scenario | Closed by | Test |
| --- | --- | --- |
| US1-1 defaults used, start URL first | here (config) + 003 (first request) | `config::defaults::tests::all_defaults_match_spec` |
| US1-2 scheme added | here | `config::validate::tests::start_url_gets_https`, UI `StartField.test` |
| US1-3 invalid start URL | here | `validate::tests::bad_start_url_is_error`, UI `StartField.test` |
| US1-4 start redirect sets internal host | 003 | uses `site::same_site` from here |
| US1-5 every value visible | here (serialization) + 006 (reopen) | `model::tests::serializes_every_field` |
| US2-1 counts and invalid lines | here | `list_input::tests::counts_7_2_1` |
| US2-2 text and CSV upload | here | `list_input::tests::csv_every_cell` |
| US2-3 only listed URLs requested | 003 | |
| US2-4 empty list refused | here | `validate::tests::empty_list_is_error` |
| US2-5 per-site robots and speed | 003, 008 | |
| US3-1/2 URL and depth limits | 003 | |
| US3-3 include and exclude | here (matcher) + 003 (crawl) | `patterns::tests::exclude_beats_include` |
| US3-4 invalid pattern | here | `validate::tests::pattern_syntax_error_names_field` |
| US3-5 pattern tester | here | `patterns::tests::tester_names_deciding_pattern`, CLI `test_patterns` |
| US3-6 subdomains | here (rule) + 003 | `site::tests::subdomain_modes` |
| US3-7 external checking | 003 | |
| US4-1 Googlebot preset and token | here (preset) + 004 (requests) | `presets::tests::googlebot_token` |
| US4-2 custom UA needs token | here | `validate::tests::custom_ua_requires_token` |
| US4-3 header sent to internal URLs | 004 | |
| US4-4 reserved header refused | here | `validate::tests::reserved_headers` |
| US4-5 Polite limits honoured | 008 | |
| US4-6 Fast notice | here | `validate::tests::fast_warns`, UI `SpeedPanel.test` |
| US4-7 "Custom" label | here | `presets::tests::edited_values_are_custom`, UI `SpeedPanel.test` |
| US4-8 robots ignore warning | here (warning) + 004 (marking) | `validate::tests::robots_modes_warn` |
| US4-9 retry attempts | 004 | |
| US5-1 save, survives restart | here | `store::tests::save_reopen` |
| US5-2 name exists | here | `store::tests::name_taken` |
| US5-3 default profile | here | `store::tests::default_profile_applies` |
| US5-4 modified indicator | here (UI) | UI `ProfileBar.test` |
| US5-5 export/import identical, CLI | here | `profile::tests::round_trip`, CLI `profile_matches_flags` |
| US5-6 reset to defaults | here (UI) | UI `ProfileBar.test` |
| US6-1 read-only stored config | 006 | uses `StoredCrawlConfig` from here |
| US6-2 new crawl from stored | here (conversion) + 006 | `stored::tests::to_profile` |
| US6-3 mid-crawl speed/retry only | here (type) + 006/008 | `stored::tests::changes_apply_in_order` |
| US6-4 older file shows "not available" | here | `stored::tests::missing_fields_reported` |
