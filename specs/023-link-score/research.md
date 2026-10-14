# Research: Link Score and Internal Link Analysis

## R1. Which link-weight method

**Decision**: classic damped PageRank (d = 0.85, uniform teleport, dangling mass spread uniformly), log-scaled to 0-100.
**Why**: the method users know from other crawlers and SEO literature; explainable in one sentence; ranking is stable under small graph changes.
**Alternatives**: HITS (hubs and authorities) is two scores and confusing in a single column; personalised PageRank from the start URL over-weights the home page's neighbourhood; raw inlink counts ignore where links come from.

## R2. Iteration strategy and determinism

**Decision**: sequential push-style power iteration over a CSR edge list, `f64`, fixed tolerance (1e-9 L1) and round cap (100).
**Why**: identical results on every platform need a fixed summation order; at 5M edges a single thread finishes in well under a second per 50 rounds, so parallelism buys little and costs determinism.
**Alternatives**: Gauss-Seidel converges faster but its result depends on node order more visibly; parallel pull iteration (transposed CSR) doubles memory and needs a deterministic reduction.

## R3. Where the score lives

**Decision**: a `link_metrics` table per crawl file, written by a 009 Crawl Analysis pass; read through unqualified names so a `temp` copy shadows it.
**Why**: a reopened or shared file shows scores instantly (FR-015); read-only files and stage 1 compute into `temp` with the same columns.
**Alternatives**: computing on every open (seconds at 1M URLs, every time); storing scores in `urls` (a wide-table rewrite of every row, and it mixes crawl facts with derived data).

## R4. Link positions: during extraction or after

**Decision**: after the crawl, from stored HTML, in a `link_positions` pass.
**Why**: keeps the fetch path untouched (top priority: crawl speed), and lets users edit rules and re-apply them without recrawling (FR-023).
**Alternatives**: classify during extraction (cheap for built-in rules, but custom rules would need a recrawl to change); classify by text heuristics (link repeated on most pages = navigation), which is useful later as a hint but not reliable enough as the definition.

## R5. Selector engine

**Decision**: `scraper` (html5ever + `selectors`), already a dependency of the engine.
**Why**: same tree builder as spec 005, so link order matches `links.pos`; full CSS selector syntax users already know.
**Alternatives**: XPath (less familiar to SEO users; spec 013 custom extraction may add it, and position rules can reuse it then).
