# CLI contract: Link Score

```text
spiderlens links analyse <file> [--quiet]
    Runs link positions (when HTML is stored) and Link Score on a crawl file.
    Stage 1 writes nothing to the file and prints the summary; stage 2 stores
    the results (same as `spiderlens analyse`, spec 009) unless the file is
    read-only. Progress lines: "Analysing: Link Score 40% (2,000,000 of 5,000,000 links)".
    Exit 0 on success, 2 on usage error, 3 when the file cannot be opened.

spiderlens links positions <file> --url <URL> [--rules <json-file>]
    Prints each link on the page with its position and the rule that decided it,
    tab-separated: pos, position, rule, target, anchor text. Uses the file's stored
    rules unless --rules is given. Writes nothing.

spiderlens crawl … [--no-analyse]
    Runs Crawl Analysis when the crawl ends unless --no-analyse. The exit code
    reflects the crawl, never the analysis.

spiderlens export <file> --tab links [--filter all|depth4|one-followed|no-followed|no-outlinks|nofollow-outlinks|nav-only] [--out file.csv]
    Same rows and default columns as the desktop Links tab (FR-028).
```
