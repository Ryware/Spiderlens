# Research: Crawl Frontier and Scope

Each entry: Decision, Rationale, Alternatives considered.

## R1. Normalization: WHATWG parser plus a percent-encoding pass

- **Decision**: `normalize_link(base, href, rules)` in `spiderlens-config`
  resolves with `Url::join` (WHATWG), which already lower-cases scheme and host,
  converts IDN hosts to punycode, removes default ports, resolves `.` and `..`
  and writes an empty path as `/`. A second pass rewrites path and query
  percent-encoding: unreserved characters (`A-Z a-z 0-9 - . _ ~`) are decoded and
  hex digits are upper-cased (FR-001). Then the fragment is removed and the query
  rules applied (FR-005). Path and query case are never changed (FR-002).
- **Rationale**: the WHATWG parser is what browsers use, so a URL means the same
  thing to us as to Chrome. The extra pass covers the one place the standard
  leaves several spellings: `%7E` and `~` are the same URL but WHATWG keeps both.
- **Query rules**: `keep` (default), `remove_all`, or `remove` a list of names
  (case-sensitive, as servers treat them); `sort` is a separate switch that
  sorts by name, keeping the order of repeated names (stable sort).
- **Failure kinds**: an href that cannot be resolved is `Malformed`, a non-http(s)
  scheme is `UnsupportedScheme`, over the length limit is `TooLong`. Empty and
  fragment-only hrefs point to the page itself and are not new URLs (spec 005
  edge case).
- **Alternatives**: the `url-normalize`-style crates (unmaintained, different
  rules); normalizing by string manipulation (would re-implement URL parsing).

## R2. One function decides scope, in the FR-038 order

- **Decision**: `ScopeRules::classify(url, &Discovery) -> Verdict` where
  `Verdict` is `Crawl`, `Check` (external status check) or `NotCrawled(reason,
  detail)`. The checks run in exactly the FR-038 order: malformed, unsupported
  scheme, external, outside start folder, excluded, not included, over depth,
  nofollow. robots.txt and the URL limit come later in the order and depend on
  state the pure function does not have, so the runner applies them at dispatch
  time, in that same order. The scope tester calls `classify` and then the host's
  robots policy if it is known (FR-016).
- **Rationale**: a Rust `match` on an enum plus one ordered function gives a
  single source of truth; the tester and the crawler cannot drift. Compare a
  Chain-of-Responsibility in Java, but as one readable function rather than
  objects.
- **Seeds**: the start URL and List mode URLs skip include rules (FR-013). List
  URLs apply exclude rules (FR-041; see R12).

## R3. Path-dependent reasons can be upgraded later

- **Decision**: only "Over depth limit" and "Nofollow" depend on how a URL was
  reached. When a URL holding one of those reasons is rediscovered through an
  eligible path, its row goes back to the queue with the new depth (FR-039). A
  queued URL found again at a shorter depth takes the shorter depth (edge case).
  Every other reason depends only on the URL, so it never changes.
- **Rationale**: keeps FR-039 cheap: one indexed lookup on the URL, which the
  de-duplication step does anyway.

## R4. robots.txt: our own RFC 9309 matcher

- **Decision**: replace `texting_robots` with a `robots` module in the engine.
  It parses groups (consecutive `User-agent` lines share a group, groups with the
  same agent are merged as RFC 9309 says), matches the product token
  case-insensitively, falls back to `*`, supports `*` and `$`, uses longest match
  with Allow winning ties, and reads `Crawl-delay` and `Sitemap`. The decision
  returns the rule line that decided it, for example `Disallow: /private/`.
  Files over 500 KiB are cut at 500 KiB. A response that looks like HTML is
  treated as "allow all" with a warning (edge case).
- **Rationale**: FR-026 shows the exact rule and FR-016 explains decisions before
  a crawl. `texting_robots` keeps its rules private, so we would have to fork it.
  The matcher is about 250 lines with table tests from RFC 9309's examples; the
  single-page check from spec 001 uses it too, so there is one implementation.
- **Matching**: glob matching with `*` is done by a linear two-pointer algorithm,
  not regular expressions, so a hostile robots.txt cannot slow the crawl.
- **Alternatives**: `robotstxt` (Google parser port; also hides the matched
  rule), forking `texting_robots`.

## R5. Frontier on disk: SQLite crawl file

- **Decision**: the frontier lives in the crawl's SQLite file from the start:
  table `urls` (one row per distinct normalized URL, with a unique index on the
  URL, which is the seen-set), `links`, `hosts` (robots outcome per host) and
  `sitemaps`, plus a `meta` key-value table (format version, stored
  configuration, status). `rusqlite` with the `bundled` feature compiles SQLite
  into the binary.
- **Rationale**: FR-043 and SC-007 need the seen-set and queue off the heap.
  SQLite's B-tree index gives "insert if new" with one statement
  (`INSERT ... ON CONFLICT DO NOTHING RETURNING id`). The same file becomes the
  crawl file of spec 006, so nothing is thrown away. WAL mode plus one
  transaction per processed page makes each page's discoveries all-or-nothing
  (spec 006 FR-004) and fast (about 20,000 row writes per second per core
  without fsync on each).
- **Bundled**: the same SQLite version on Windows, macOS and Linux, no system
  library to install, like the rustls decision in spec 001.
- **Threading**: the coordinator task owns the connection (a `rusqlite`
  connection is `Send` but not `Sync`, like a JDBC connection that must not be
  shared). Store calls are short and run inline; spec 008 may move them to a
  dedicated writer thread if benchmarks show the coordinator waiting on disk.
- **Alternatives**: `sled` or RocksDB (another format beside SQLite, which spec
  006 needs anyway); an in-memory `HashSet` plus a Bloom filter (fails SC-007);
  `sqlx` (async, but adds a runtime layer we do not need for an embedded file).

## R6. A bounded window over the queue

- **Decision**: the queue order is `(depth, queue_seq)`. The runner keeps an
  in-memory `BTreeMap` of at most a few thousand queued entries with the
  smallest keys, plus a high-water key `hi`. Invariant: every queued row with a
  key up to `hi` is in the window. New rows with a key up to `hi` are added to
  the window; others stay on disk. When the window runs low, the next rows after
  `hi` are loaded with one indexed query.
- **Rationale**: breadth-first order needs "smallest key first", which the window
  gives in O(log n), while memory stays bounded whatever the crawl size.
- **Alternatives**: query the database for every dispatch (too many queries at
  500 URLs/s); keep the whole queue in memory (fails SC-007).

## R7. Scheduling: breadth-first, relaxed only for blocked hosts

- **Decision**: on every event the runner walks the window in key order and
  starts the first entry whose host can take a request now, until the global
  concurrency is used up. A host can take a request when its robots.txt is known,
  its in-flight count is under the per-host limit (or under 1 with a crawl-delay)
  and its crawl-delay gap has passed. The global rate cap spaces request starts.
  The runner sleeps until the next result or the earliest host or rate deadline.
- **Rationale**: this is exactly FR-020 with the FR-021 relaxation: an entry at
  depth d+1 can only start before one at depth d when the depth-d entry's host is
  blocked. Adaptive concurrency and back-off belong to specs 004 and 008 and plug
  into the same host state.
- **Crawl-delay**: when honoured (default), at most one request in flight and at
  least the delay between request starts on that host (FR-027). Very long delays
  (60 s or more) produce a warning with the resulting rate per hour.

## R8. Redirects are frontier entries, not inline hops

- **Decision**: the crawler sends one request per URL with redirects off. A 3xx
  response is recorded on the source URL (status and target), and the target
  enters the frontier as a discovery of kind "redirect" at depth + 1.
- **Rationale**: US3-8 and spec 004 FR-010 require scope and robots rules for
  every redirect target, and that a destination already fetched is not fetched
  again. Treating the target as a discovered URL gives both for free through the
  normal de-duplication and scope path. The single-page check of spec 001 keeps
  inline following.
- **Start URL redirect**: a same-site target host joins the internal hosts
  (`InternalHosts::adopt_redirect`, spec 002) and the target is treated as a
  seed: include rules do not apply to it.

## R9. Sitemaps: robots.txt only, streaming parse, three index levels

- **Decision**: with "Use sitemaps from robots.txt" on, once the start host's
  robots.txt is known the runner reads each `Sitemap:` URL. Files are parsed with
  `quick-xml` as a stream (`<urlset><url><loc>` and `<sitemapindex><sitemap><loc>`),
  gzip is detected by its magic bytes and decoded with `flate2`, and index files
  are followed up to 3 levels. Each sitemap's URL, parent, number of URLs and any
  error is stored. Listed URLs go through the normal scope path at depth 1.
- **Rationale**: FR-034 to FR-036. Streaming keeps memory flat for the 50,000-URL
  sitemaps the protocol allows. Sitemap requests use the configured user agent
  (FR-029) and do not count toward the URL limit (FR-019).
- **Alternatives**: `sitemap` crate (unmaintained); DOM parsing (memory).

## R10. Where the performance criteria are proven

- **Decision**: this feature adds a guard test that crawls a 2,000-page fixture
  and a store test that inserts 200,000 URLs while checking that the window stays
  bounded. The 100,000-page and 1,000,000-URL benchmarks of SC-006 to SC-008 are
  built with the benchmark harness of spec 008, which owns throughput targets.
- **Rationale**: the benchmarks need a fixture site that serves 500+ pages per
  second and a memory sampler, both of which spec 008 defines. Building them twice
  is waste.

## R11. Pause, stop and resume

- **Decision**: the runner listens on a `tokio::sync::watch` channel for
  `Run`, `Pause` and `Stop`. Pause stops dispatching and waits for in-flight
  requests, which are recorded normally. Stop does the same, then marks every
  queued crawl entry "Crawl stopped". Resume opens the file, returns rows that
  were `in_flight` (a crash) or "Crawl stopped" to the queue, and continues. In
  the CLI, Ctrl-C pauses and prints the resume command; a second Ctrl-C exits at
  once (the file is still consistent thanks to per-page transactions).
- **Rationale**: FR-022 and US4-4. Spec 006 adds the desktop pause and resume UI
  and crash-recovery tests on top of the same file.

## R12. Conflicts with spec 002, and how they are resolved

The owner should confirm these; the plan uses the resolution shown.

1. **Start URL redirecting to another site.** Spec 003 FR-009 says the target's
   host becomes internal; spec 002 (FR-003 and its edge case) says only a
   same-site target does, and an off-site redirect is reported and not followed.
   **Resolution**: spec 002's rule, because it is the more specific one and an
   off-site redirect usually means the user typed an old domain.
2. **List mode depth and exclude rules.** Spec 002 says depth and patterns do not
   apply in List mode; spec 003 FR-041 says exclude rules apply to listed URLs and
   links may be followed up to a user-set depth (default 0). **Resolution**: spec
   003, because it is the frontier spec. A new setting `limits.list_depth`
   (default 0) holds the List mode depth so the Spider mode `max_depth` keeps its
   meaning; include rules still do not apply to listed URLs.
3. **Ignoring robots.txt.** Spec 002 has two override modes: "Ignore but report"
   (crawl, and mark what robots.txt would block) and "Ignore" (robots.txt not
   fetched). Spec 003 FR-028 says an override must still record what would have
   been blocked. **Resolution**: "Ignore but report" satisfies FR-028 and is what
   the CLI help recommends; "Ignore" stays as the explicit "do not even fetch
   robots.txt" choice from spec 002, and its stored configuration says so.

## R13. Fixture with several host names on one port

- **Decision**: the test site answers by `Host` header (`example.test`,
  `www.example.test`, `blog.example.test`, `other.test`), and the engine accepts a
  test-only list of DNS overrides (`reqwest::ClientBuilder::resolve`) that point
  those names at 127.0.0.1. The site logs every request with its time, which the
  tests use for "fetched exactly once" and crawl-delay gaps.
- **Rationale**: internal versus external, `www` handling and subdomains can only
  be tested with real host names, and Principle VII forbids the internet. The
  `.test` TLD is reserved (RFC 2606) and never resolves for real.
