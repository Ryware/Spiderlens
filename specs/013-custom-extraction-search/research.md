# Research: Custom Extraction and Custom Search

## R1. XPath engine

| Option | What it is | Fits FR-028 (one parse) | Notes |
|---|---|---|---|
| `sxd-xpath` 0.4 + `sxd-document` | XPath 1.0 over its own DOM | Yes: we copy the html5ever tree, no second parse | Pure Rust, MIT/Apache, complete XPath 1.0 function library; no release since 2019 |
| `xee-xpath` | XPath 3.1 over `xot` trees | Yes, with a tree copy | Active; large dependency tree (ICU, decimal, regex engines); XPath 3.1 semantics differ from what users copy from browsers |
| `skyscraper` | Own HTML parser and XPath | No: parses again with a non-WHATWG parser | Results would disagree with the browser for malformed pages |
| Own XPath 1.0 evaluator over `ego_tree` | About 1,500-2,000 lines | Yes, no copy | Most work and most risk; only worth it if the copy cost is high |

**Decision**: `sxd-xpath`, behind `custom::xpath`. XPath 1.0 is what
browsers' `document.evaluate` and DevTools "Copy XPath" produce, which is
how users write these rules. The copy happens only for crawls with XPath
rules. If T003's measurement shows the copy above 30% of parse time on the
bench pages, we fall back to the own-evaluator option and say so in the
code PR.

## R2. JSON-LD path syntax

Users mostly want "the price of the Product" or "the author's name of the
Article". Pure JSONPath for that is `$..[?@['@type']=='Product'].offers.price`,
which is hard to write. The item path form (`@type` + dotted path) covers
the common cases and `@graph`; JSONPath (RFC 9535, `serde_json_path`)
covers the rest. `serde_json_path` is maintained, MIT, and builds on
`serde_json`, already in the tree.

## R3. Regex dialect

`regex` crate syntax (Perl-like, no lookaround, no backreferences). Linear
time guarantees mean a regex cannot hang the crawl. Users who paste a
pattern with lookaround get a validation error naming the construct
(FR-008). Plain text searches compile to escaped regexes so both paths
count the same way (non-overlapping, left to right).

## R4. Rule budget per page

Default 2 s per page across all rules: generous for normal rules (a 30-rule
set on a typical page is expected well under 10 ms) and protects the crawl
from pathological expressions. Not a user setting (principle VIII); a
constant tuned from benchmarks.

## R5. Prior art for behaviour

The usual desktop SEO crawlers offer CSS path, XPath and regex extraction
with "Extract HTML element / Inner HTML / Text / Function value" modes,
and custom search with contains / does not contain over HTML or page
text. They require a new crawl to change rules. Our differences: reruns on
stored HTML, a JSON-LD path kind, a tester on a crawled page, and the CLI.
