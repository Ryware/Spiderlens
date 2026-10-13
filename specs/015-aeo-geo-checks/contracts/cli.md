# CLI contract: `spiderlens aeo` (spec 015)

Every command takes a crawl file. Output is human-readable by default; `--json` prints the same data as JSON with sorted keys.

| Command | What it does | Spec |
|---|---|---|
| `spiderlens aeo run <file> [--profile <name>] [--fail-under <score>]` | Runs the AI checks (migrating the file with a backup if needed), prints the site score, parts and top 10 fixes. Exits with the quality-gate code when the site score is below `--fail-under`. | FR-002, FR-048 |
| `spiderlens aeo summary <file>` | Prints stored results without running anything. Exit 2 with the reason when the file has no AI results. | FR-005 |
| `spiderlens aeo export <file> --format csv\|json [--filter <id>] [-o <path>]` | Exports the AI search tab rows (same columns and filters as the app). | FR-041, US7 |
| `spiderlens aeo page <file> <url>` | Prints one page's explanation: parts, checks with points and basis, access per crawler. | FR-043 |
| `spiderlens aeo llms-txt <file> [--max-links 100] -o <path>` | Writes a draft llms.txt. Never uploads. | FR-047 |
| `spiderlens aeo probe <file> --yes [--bots gptbot,claudebot,...] [--pages 10]` | Runs the user agent probe. Refuses without `--yes`, printing what it would do. | FR-012 |
| `spiderlens aeo bots [--json]` | Lists the built-in and custom AI crawlers with the list version. | FR-006, FR-007 |

`spiderlens crawl` gains `--no-ai-checks` and `--no-site-files` (defaults on, FR-003, FR-010).

Exit codes: existing ones unchanged (0, 2 to 6). Below `--fail-under` uses the same quality-gate code as spec 009's failing-issues exit, so CI scripts check one code; 009 assigns it (7 unless 009 picks another).
