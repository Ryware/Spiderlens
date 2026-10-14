# CLI contract: `spiderlens duplicates`

```text
spiderlens duplicates <FILE> [--pages | --pairs | --clusters]
                             [--threshold <50-100>] [--min-words <N>]
                             [--include-non-indexable]
                             [--format csv|json] [--output <PATH>]
```

- Default view `--pages`. Default format `csv` to stdout.
- Without overrides: reads stored results; if the pass is stale or missing, runs it first (into the file when writable, into memory otherwise) and says so on stderr.
- With overrides: runs into memory, never changes the file.
- `spiderlens analyse <FILE>` (spec 009) runs this pass with every other stale pass.

## Columns

`--pages`: `address, status, main_words, closest_similarity, closest_address, near_duplicates, near_duplicates_is_minimum, cluster, exact_duplicate`

`--pairs`: `address, other_address, similarity, exact, cluster` (each unordered pair once, `address < other_address`)

`--clusters`: `cluster, size, min_similarity, max_similarity, representative, exact`

JSON: an array of objects with the same keys, numbers as numbers, booleans as booleans.

## Exit codes

0 success; 2 usage error (bad option values); 3 the file cannot be read (same as `spiderlens export`); 4 the file has no near duplicate data (made before this feature), with the reason on stderr.
