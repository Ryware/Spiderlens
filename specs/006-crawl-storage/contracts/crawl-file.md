# Contract: crawl file format rules for other specs

1. Schema changes are migration steps in `crates/engine/src/crawl/migrate.rs`.
   Never edit the base `SCHEMA` in `store.rs` and never bump
   `FORMAT_VERSION` by hand (it is the last step's number).
2. A step is `additive: true` when it only adds tables, indexes or nullable
   columns (or columns with a constant default). Older readers can then
   still open the file read-only. Anything else is `additive: false` and
   raises `read_compat`.
3. Readers detect optional data by schema introspection or
   `meta.data_features`, never by `format_version`.
4. Viewers open with `SQLITE_OPEN_READ_ONLY` and never write. Only a
   `Crawler` (which holds the writer lock) writes.
5. Indexes that slow writes (random keys) are built when a run ends
   (`build_result_indexes`), as spec 005 did.
6. Local disk or write problems are `events` rows of kind `local_error`,
   never `urls.error_class` values.
