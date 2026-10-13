# Contract: the AI crawler list (owned by spec 015, used by 016 and 009)

- File: `crates/config/src/ai_bots.toml`, embedded at build time. API: `spiderlens_config::ai_bots::{all, version, date, merged(custom)}`.
- Fields: see data-model.md `AiBot`.
- Stability: `token` is the identity. `index` never changes for an existing entry within the list's lifetime; new entries take the next index; removed entries keep their index reserved (`retired = true`), so stored bitmasks stay readable.
- Updates: one pull request that edits the TOML, bumps `version` and `date`, and passes `ai_bots::tests` (unique tokens and indexes, docs URL present, purpose known).
- Other specs must not keep their own crawler list; they read this one and may add fields only through a change to this contract.
