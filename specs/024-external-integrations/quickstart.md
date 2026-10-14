# Quickstart: External Data Integrations

Validation scenarios against the fake providers (plan D12). Every scenario runs in CI on all three platforms; none calls a live provider.

1. **Opt-in** (US7.1): fresh profile, crawl the fixture site; the fake providers' request logs are empty.
2. **Search Console** (US1): connect the fake Google account, pick the suggested property, fetch 28 days; every crawled URL shows the fake's numbers, URLs without rows show 0 with "no data", unmatched rows appear as orphan candidates; paging fetched every row (fake holds 60,000 rows).
3. **No requests during a crawl** (FR-002): start a crawl, try to fetch; the command is refused and the fakes log nothing.
4. **URL Inspection quota** (US2): fake allowance 50/day, select 120 URLs; run pauses after 50 with the reset time; advance the fake clock; resume inspects only the 70 left.
5. **Field data and lab tests** (US3): 300 URLs; the fake's rate log never exceeds the limit; URLs without data show origin values labelled "origin"; second run the same day sends 0 requests.
6. **Analytics** (US4): rows with and without query strings combine by default and stay separate with the option.
7. **Link metrics** (US5): estimate within 10% of the fake's metered units; cap stops the run; cached rerun consumes nothing.
8. **Sheets and Looker Studio** (US6): export 5,000 rows to the fake Sheets; the sheet matches the CSV export; the Looker link names that sheet and worksheet.
9. **No secrets in files** (US7.2, SC-004): after all of the above, byte-search the crawl file, exported profile, cache file, settings and logs for every token and key; none found.
10. **Forget** (US7.3): Forget Google; the keychain entry is gone and the fake received a revoke call.
11. **CLI parity** (US8): repeat scenario 2 with `spiderlens integrations fetch`; the two crawl files hold the same integration rows.
