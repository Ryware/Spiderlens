# Quickstart: Issue Rules Engine

1. Crawl the issues fixture: `cargo run -p spiderlens-cli -- crawl http://127.0.0.1:<port>/ -o /tmp/issues.sqlite` (the fixture server is started by `cargo test -p spiderlens-test-fixtures --test serve_issues -- --ignored` and prints the port).
2. `spiderlens issues /tmp/issues.sqlite` lists every issue of the first rule set, High first. Each count equals the fixture's expected count in `crates/test-fixtures/src/issues.rs`.
3. `spiderlens issues /tmp/issues.sqlite --fail-on high; echo $?` prints the `ISSUES_FOUND` code; on the clean fixture site it prints 0.
4. In the app, open the file: the sidebar shows Issues with High, Medium, Low, Opportunity totals. Open "Page title: missing": the grid lists the fixture's missing-title pages and the header explains the rule.
5. Select a page with a 74-character title: the inspector's Issues section shows "Page title: over 60 characters, 74 characters".
6. Settings › Issues: set the title threshold to 80. The issue's count, its label and Page titles > "Over 80 characters" update within 2 seconds; no crawl starts.
7. Open a crawl file from before page extraction: content rules show as unavailable with the reason.
