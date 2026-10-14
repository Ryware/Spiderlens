# Research: Issue Rules Engine

## R1. Where to evaluate: engine write path or read side

- **Decision**: read side (`spiderlens-results`).
- **Why**: the crawl's top priority is speed; spec 005 already measured that index upkeep on random keys costs about 20% (the reason title and description hash indexes are built at run end). Writing hits during the crawl would add per-page writes and still need a site-wide pass. The reader already runs on its own connection with per-data-version temp tables (007 plan D6).
- **Rejected**: evaluating in the engine after each batch commit (slows the writer, duplicates 007 logic); a separate `rules` crate (would need the 007 SQL functions and tab definitions, so it would depend on `results` anyway).

## R2. Rule language

- **Decision**: SQL conditions in Rust statics, plus a handful of registered SQL functions.
- **Why**: 007 filters already prove the pattern (one definition for app and CLI). SQLite evaluates them next to the data, with PK lookups into `pages`, and the compiler holds them as `&'static str`, so a typo fails a test, not a user.
- **Rejected**: a custom DSL or TOML rules (a parser and an interpreter for no gain until user-written rules, which are out of scope); Rust closures per row (would pull every row into Rust, contrary to principle VI).

## R3. Materialised hits versus counting on demand

- **Decision**: `temp.issue_hits` per data version, updated incrementally.
- **Why**: the sidebar needs all counts every 5 s and the inspector needs "issues of this URL" in 300 ms (007 SC-006). Counting 74 conditions on demand is one scan, but per-URL lookup and the 1,000,000-row issue views would each re-run conditions. A two-column WITHOUT ROWID table gives all three from indexes.
- **Measured later**: the benchmark task records build time per 100k URLs; the decision stands if under 1 s.

## R4. Thresholds

- **Decision**: a non-deterministic SQL function `thr(key)` reading shared settings.
- **Why**: no SQL rebuilt per change, no user values formatted into SQL, and the 007 filters pick up the same value for free.

## R5. Priorities and defaults

- **Decision**: High, Medium, Low, Opportunity, defaults as in the spec table. Defaults follow common audit practice: issues that block indexing or break navigation are High; issues that weaken how a page is shown in results are Medium; hygiene is Low; improvements that are not faults are Opportunity.

## R6. Hreflang code validity

- **Decision**: shape check (language 2 or 3 letters per ISO 639, optional script, optional region of 2 letters or 3 digits, or `x-default`), with the ISO 639-1 list and ISO 3166-1 alpha-2 list compiled in as two sorted string slices (about 2 KB). No new dependency.

## R7. Non-descriptive anchor text

- **Decision**: a fixed list of English phrases (click here, here, read more, more, learn more, this page, link, go, continue, details), compared after trimming and lowercasing. Other languages later with the AEO work (spec 015).

## R8. Exit code numbering

- **Decision**: taken at merge time as the next free code (expected 7). Matches the migration step rule agreed with the Storage thread, avoids reserving numbers across parallel PRs.

## R9. Readability formula

- **Decision**: Flesch reading ease for English (and pages with no declared language), counted during extraction; no score under 100 words.
- **Why**: it is the score SEO tools and editors know, needs only word, sentence and syllable counts, and the walk already has the text. Computing it later from stored HTML would re-parse every page and fail when HTML storage is off.
- **Rejected**: a text-analysis crate (new dependency for three counters); scoring every language with the English formula (meaningless numbers for Hebrew or Chinese).
