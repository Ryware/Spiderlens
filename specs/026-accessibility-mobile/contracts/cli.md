# CLI contract: accessibility and mobile checks

Same defaults as the desktop app (FR-032). No window is opened.

## Crawl flags (`spiderlens crawl`)

| Flag | Values | Default | Setting |
|---|---|---|---|
| `--accessibility` | `off`, `static`, `full` | `static` | `accessibility.level` |
| `--wcag` | `2.0-a` ... `2.2-aaa` | `2.2-aa` | `accessibility.wcag` |
| `--no-a11y-best-practice` | | off | `accessibility.best_practice = false` |
| `--no-a11y-mobile` | | off | `accessibility.mobile = false` |
| `--a11y-budget` | 1 to 30 (seconds) | 5 | `accessibility.budget_secs` |

`--accessibility full` without `--render javascript` prints `notice: full accessibility checks need JavaScript rendering; static checks will run` and continues (FR-002).

## `spiderlens accessibility <file>`

Prints the summary (FR-033) and the violations grouped by check, with page and element counts, preceded by the disclaimer line (FR-025).

| Option | Meaning |
|---|---|
| `--wcag 2.1-aa` | Count against another target without checking again (FR-020) |
| `--needs-review` | Show "needs review" results instead of violations |
| `--format text\|csv\|json` | `csv` and `json` are the FR-026 exports: one row per page, check and element |
| `--summary` | One row per check: pages, elements, impact, priority, example page |
| `--mobile` | The Mobile tab's rows instead |

Exit codes: 0 when printed (even with violations), 1 when the file cannot be read, 2 for usage errors. Failing a build on violations is `spiderlens issues <file> --fail-on high` (spec 009).

## `spiderlens accessibility run <file> [--static | --full]`

Runs checks on an existing crawl (FR-031). Asks for confirmation when results exist, unless `--yes`. Exits 2 with a message for files without stored HTML or opened view-only (another process holds the writer lock). `--full` needs a usable browser (spec 014 FR-045 rules apply).
