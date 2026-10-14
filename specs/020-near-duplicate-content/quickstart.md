# Quickstart: Near Duplicate Content

1. `cargo run -p spiderlens-test-fixtures --bin fixture-site -- near-dup` serves the near-duplicate fixture (groups of known similarity, shared navigation, a large related-products block, a 300-page template group).
2. `cargo run -p spiderlens-cli -- crawl http://127.0.0.1:<port>/ --output nd.sqlite`
3. `cargo run -p spiderlens-cli -- duplicates nd.sqlite --clusters` lists the expected clusters (see `crates/test-fixtures/src/near_dup.rs` for the truth table).
4. `cargo run -p spiderlens-cli -- duplicates nd.sqlite --pages --threshold 80` adds the 85% pair (US1.5) without changing the file.
5. In the app, open `nd.sqlite`: Content tab › Near duplicates; switch to Clusters; select a page and use Compare in the Duplicates section.
6. Settings › Content: add `.related-products` to excluded elements; the analysis reruns from stored HTML and the related-products pair disappears (US4.1).
