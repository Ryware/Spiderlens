# CLI Contract: `spiderlens`

## Commands

### `spiderlens check <URL> [options]`

Runs a single-page check.

| Option | Default | Meaning |
| --- | --- | --- |
| `--json` | off | Print one JSON object matching [page-check.schema.json](./page-check.schema.json) |
| `--connect-timeout <SECONDS>` | 10 | Connect time limit |
| `--read-timeout <SECONDS>` | 30 | Time limit for each read |
| `--max-size <MB>` | 50 | Body size limit |

If `<URL>` has no scheme, `https://` is assumed. Only `http` and `https` are
accepted.

### `spiderlens version [--json]`

Prints product, product version, engine version and build id. `--version` on the
root command prints the short form `spiderlens 0.1.0 (build <id>)`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Page fetched (any HTTP status, including 4xx and 5xx) |
| 1 | Unexpected internal error (a bug) |
| 2 | Usage error, including malformed URL |
| 3 | Blocked by robots.txt |
| 4 | Fetch failed (any error class except malformed URL) |

## Human output (stdout)

```text
URL          https://example.test/old
Redirect     301 → https://example.test/mid
Redirect     302 → https://example.test/home
Final URL    https://example.test/home
Status       200
Type         text/html
Time         142 ms
Title        Home | Example
State        fetched
```

- Title line shows `(no title)` or `(not applicable)` for the two special cases.
- On failure: `State  failed`, then `Error  timeout: <message>` and
  `Attempts  1`.
- On block: `State  blocked by robots.txt`.
- Error and usage messages for humans go to stderr; with `--json`, the JSON record
  always goes to stdout, including for failures and malformed URLs.
