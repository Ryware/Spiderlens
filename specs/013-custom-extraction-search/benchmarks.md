# Spec 013 benchmarks

Machine: 4-core Linux cloud container. bench-site runs on the same cores
(`bench-site --pages 100000`), so figures include the server's CPU use.
Release builds, `spiderlens crawl http://127.0.0.1:8700/p/0 --speed fast`.
Rule set: [`bench-rules.json`](bench-rules.json) (5 CSS, 5 XPath, 5 regex,
5 JSON-LD paths, 5 HTML and 5 visible-text searches), via `--rules`.

## Crawl, 100,000 pages (T001, T032)

Three runs each, interleaved.

| Build | Wall time | Peak memory |
|---|---|---|
| main (merge base) | 32.2, 32.8, 34.7 s | 30-31 MB |
| this branch, no rules | 32.0, 32.8, 35.0 s | 32 MB |
| this branch, 30 rules | 33.3, 34.1, 35.3 s | 48-51 MB |

- SC-001 (no rules within 2% of throughput and 5% of memory): met. Without
  rules nothing is compiled and no rule tables are created.
- SC-002 (30 rules keep 85% of throughput): met, about 96%. CPU time rises
  from about 63 s to about 82 s per run, which the crawl absorbs because it
  waits on network and the writer.
- Memory stays flat as the crawl grows: 44 MB at 10,000 pages, 43 MB at
  30,000, 48-51 MB at 100,000. The fixed extra 15-20 MB is the per-thread
  regex caches and the in-flight results.
- The crawl file grows from 336 MB to 473 MB: about 620 bytes per page in
  `custom_results` (slots and first values) and 520 bytes in
  `custom_values` (zstd JSON of the other values and up to 10 search
  matches in context per search).

## Cost per rule kind, one 3 KB bench page (T003, T032)

`custom::tests::cost_by_kind` (ignored test, see its comment), µs per page
on top of extraction without rules (about 90 µs):

| Rules | First version | Tuned |
|---|---|---|
| 5 CSS | 10 | 10-19 |
| 5 XPath | 175 | 11 |
| 5 regex | 5 | 5-7 |
| 5 JSON-LD | 39 | 14 |
| 10 searches | 72 | 61 |
| all 30 | 281 | 102 |

The first version reached only 66-80% throughput on the crawl. What changed:

1. **XPath fast path** (`custom/xpath_fast.rs`). Copying the tree into
   sxd-document costs about 32 µs per page and each sxd evaluation about
   18-22 µs, since `//` builds node sets over the whole document. Simple
   location paths (`/` and `//` steps, lowercase names or `*`, `[@a]`,
   `[@a='v']`, one `[n]`, a last `/@attr` or `/text()`, inside `count()` or
   `string()`) have an exact CSS form and run on the page's own tree. A test
   checks that both engines give the same answer on every such expression.
   Anything else still uses sxd, with compiled expressions cached per
   thread.
2. **JSON-LD** blocks are parsed once per page for all rules, not once per
   rule.
3. **Encoding off the writer.** Packing the slots, the first values JSON and
   the zstd values row now happens on the extraction thread, so the single
   SQLite writer only inserts.

Kept for later if needed: a RegexSet pre-filter for searches, and cheaper
search contexts.
