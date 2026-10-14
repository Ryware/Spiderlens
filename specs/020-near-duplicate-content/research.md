# Research: Near Duplicate Content

## R1. Similarity measure

**Decision**: Jaccard similarity of the sets of 5-word shingles of the main content.

**Rationale**: It matches the spec's definition (FR-007): every changed, added, removed or moved passage removes shingles from the intersection. It is symmetric, between 0 and 1, and is the measure SEO users meet in other desktop crawlers ("near duplicate, 90% similarity, minhash"), so percentages read the same way. Five words is the common choice for web pages: shorter shingles make unrelated pages share common phrases, longer ones make one changed word remove too many shingles (one changed word removes up to five shingles of a 600-word page, under 1%).

**Alternatives**: cosine over word counts (ignores order, "a b" equals "b a" text); edit distance (quadratic per pair, no compact summary); embeddings (semantic, needs a model, out of scope).

## R2. Summary: one-permutation MinHash, 960 bins, 8 bits

> **Changed during implementation (PR #92)**: 960 bins × 8 bits replaced 480 × 16 bits at the same 960 bytes. The test suite's 400-pair accuracy check measured p95 3.9 points at 80% with 480 × 16, and 2.2 points with 960 × 8 after the b-bit correction `(m − 1/256) / (1 − 1/256)`. Twice the bins halves the variance; the 1 in 256 chance match is corrected exactly. The alternatives below are kept as written.

**Decision**: one-permutation hashing (OPH) with optimal densification, K = 480 bins, low 16 bits stored per bin.

**Prototype** (scratch Python, exact Jaccard as reference, 60 random pairs per row):

| Shingles per page | True similarity | Mean error | Std dev | 95th percentile of abs error |
|---|---|---|---|---|
| 1,000 | 90% | +0.2 pts | 1.3 pts | 2.0 pts |
| 1,000 | 95% | 0.0 pts | 0.9 pts | 1.6 pts |
| 1,000 | 80% | −0.2 pts | 1.6 pts | 2.9 pts |
| 200 | 90% | −0.2 pts | 1.4 pts | 2.3 pts |
| 60 | 90% | −0.3 pts | 1.6 pts | 3.3 pts |

SC-003 (±3 points for 95% of pairs) holds from about 100 words; at the 50-word minimum the estimate is slightly noisier, which is acceptable because the true similarity of such short texts moves by 2 points per changed shingle anyway. The code PR repeats this as a Rust test with fixed seeds.

**Alternatives**:

- **Classic MinHash with 480 hash functions**: same accuracy, 480 multiply-adds per shingle; about 100 µs per 1,000-word page, which would cost several percent of crawl speed against the 2% budget (SC-004).
- **SimHash, 64 bits**: 8 bytes per page and very fast, but it estimates the angle between weighted feature vectors, not shared text; Hamming distance to percentage is not linear and differs by page length, and it is reliable mainly for "almost identical" (above about 95%). Users set 80 to 90%.
- **Exact shingle sets**: exact, but 4 bytes per word per page (about 400 MB per 100k pages) and still needs an index to avoid all pairs.
- **b-bit with fewer bits (b = 8)**: halves storage but a 1 in 256 false match per bin adds a 0.4-point bias that must be corrected; 16 bits keeps the estimator simple.

## R3. Candidate search: banded LSH

**Decision**: split the 480 bins into `b` bands of `r` bins; pages sharing a band key are candidates. Choose the largest `r` with P(candidate | J = floor) ≥ 0.999 and `b = 480 / r`.

| Floor | r | b | P(candidate) at floor | at 50% | at 30% |
|---|---|---|---|---|---|
| 50% | 4 | 120 | 0.9996 | 0.9996 | 0.62 |
| 60% | 5 | 96 | 0.9996 | 0.95 | 0.21 |
| 70% | 6 | 80 | 0.99996 | 0.72 | 0.057 |
| **80% (default)** | **9** | **53** | **0.9995** | **0.098** | **0.001** |
| 85% | 11 | 43 | 0.9996 | 0.021 | 0.0001 |
| 90% | 15 | 32 | 0.9994 | 0.001 | 0 |

The last two columns show why the default floor matters: at a 50% floor nearly every pair of pages that share a template's leftover text is a candidate, and candidate scoring dominates the time. At 80% only pairs that are actually close are scored. This is spec amendment 1.

**Alternatives**: sorting by signature prefixes (misses pairs that differ in the first bins); all-pairs (5 × 10⁹ pairs for 100k pages); an inverted index on shingles (needs the shingle sets, R2).

## R4. Large groups

**Decision**: buckets larger than 256 pages merge their members into dense groups via union-find; each dense group is scored all-pairs once, in parallel; above 50,000 members a deterministic sample of 2,000 members is used for counts, shown as lower bounds.

**Rationale**: per-band pair expansion of a 20,000-page bucket is 2 × 10⁸ pairs per band and up to 53 bands; scoring the group once is 2 × 10⁸ comparisons of 480 `u16`, about 10 s on 4 cores. Above 50,000 the quadratic cost exceeds the time budget (a 200,000-page group is 2 × 10¹⁰), hence amendment 2.

## R5. Main content rules

**Decision**: `<main>`, else all `<article>` elements, else body minus boilerplate. Boilerplate everywhere: `nav`, `aside`, `form[role=search]`, roles `navigation`, `complementary`, `search`. Only outside `main`, `article`, `section`: `header`, `footer`, roles `banner`, `contentinfo`.

**Rationale**: these are the elements and roles the HTML accessibility mapping (HTML-AAM) treats as landmarks other than main; `header`/`footer` inside sectioning content are article headers (bylines, dates) and not site chrome. Readability-style scoring of text density was considered and rejected for the default: it is heuristic, harder to explain, and costs a second pass; users with unusual markup get selectors (FR-003).

## R6. Diff for the comparison view

**Decision**: paragraph alignment by LCS over paragraph hashes, then word-level LCS inside aligned paragraphs, implemented in about 150 lines in `near_dup/diff.rs`.

**Rationale**: inputs are two pages (a few thousand words), so an O(n·m) LCS on paragraphs and short word runs is fast enough (SC-005: under 1 s). The `similar` crate would also do it; a dependency is not justified for one view (constitution VIII).
