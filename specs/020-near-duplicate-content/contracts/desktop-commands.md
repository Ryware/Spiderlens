# Desktop commands (Tauri)

All read from the open crawl through the results crate; types exported with ts-rs.

| Command | Input | Output |
|---|---|---|
| `near_dup_status` | — | `{ state: "not_run" \| "running" \| "done" \| "stale" \| "unavailable", reason?, settings?, compared, excluded: { too_short, not_indexable, no_signature }, clusters }` |
| `near_dup_clusters` | `{ offset, limit, sort }` | `{ total, rows: [{ id, size, min_sim, max_sim, representative: { id, address }, exact }] }` |
| `near_dup_pairs` | `{ url_id }` | `{ total, shown, rows: [{ other: { id, address }, sim, exact }], cluster? }` |
| `compare_pages` | `{ a, b, full: bool }` | `{ sim, exact, a: Side, b: Side } \| { unavailable: "html_not_stored", sim }`, `Side = { address, source, paragraphs: [{ spans: [{ text, op: "equal" \| "added" \| "removed" \| "excluded" }] }] }` |
| `run_near_dup` | — | starts the pass (on demand); progress arrives on the spec 009 `analysis_progress` event |
| `export_near_dup` | `{ kind: "pairs" \| "cluster", cluster?, path }` | `{ rows }` |

The Content tab itself uses the existing 007 `view`, `counts` and `export` commands with `tab = "content"`.
