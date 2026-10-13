# Quickstart: Page Data Extraction

```sh
# Tests: unit tests for extraction, fixture-site tests for every scenario,
# malformed corpus against Chromium's golden output
cargo test -p spiderlens-engine extract
cargo test -p spiderlens-engine --test extraction

# Crawl the local benchmark site and look at one page
cargo run --release -p spiderlens-test-fixtures --bin bench-site -- --pages 100000 &
cargo run --release -p spiderlens -- crawl http://127.0.0.1:8700/p/0 --speed fast --out /tmp/bench.spiderlens
cargo run --release -p spiderlens -- page /tmp/bench.spiderlens http://127.0.0.1:8700/p/5 --json
cargo run --release -p spiderlens -- html /tmp/bench.spiderlens http://127.0.0.1:8700/p/5 | head

# Without raw HTML
cargo run --release -p spiderlens -- crawl http://127.0.0.1:8700/p/0 --no-store-html --out /tmp/nohtml.spiderlens

# Regenerate the browser golden file after changing the malformed corpus
node scripts/browser-golden.mjs crates/test-fixtures/malformed
```
