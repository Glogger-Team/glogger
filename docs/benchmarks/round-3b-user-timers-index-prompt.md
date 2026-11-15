# Benchmark task (frozen — implement exactly, no scope changes)

## Target

File: `src-tauri/src/db/migrations.rs` — the ONLY file you may modify.

## Change

The `get_user_timers` query (`src-tauri/src/db/timer_commands.rs:41-48,
WHERE character_name=? AND server_name=? ORDER BY started_at DESC`) sorts
without an index: `user_timers` has only
`idx_user_timers_char_server ON (character_name, server_name)` (v40), the
exact temp-B-tree-sort shape that v69 fixed for `chat_messages` with
`idx_chat_messages_channel_timestamp`. Add migration v91 in the established
pattern:

1. Insert AFTER the v90 block and BEFORE `tx.commit()` in `run_migrations`:
   `if current_version < 91 { migration_v91_user_timers_started_index(conn)?; super::record_migration(conn, 91)?; }`
2. New function (placed with the other `migration_vN_*` fns):
   `fn migration_v91_user_timers_started_index(conn: &Connection) -> Result<()>`
   executing exactly:
   `CREATE INDEX IF NOT EXISTS idx_user_timers_char_server_started ON user_timers(character_name, server_name, started_at DESC);`
3. Tests in the in-file `mod tests` (in-memory SQLite + `run_migrations`, mirroring the v69/v60 test style):
   - index exists: `SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_user_timers_char_server_started'` returns 1;
   - insert 2 timers out of chronological order for one character + 1 timer for another character; the `get_user_timers`-shaped query (`WHERE character_name=? AND server_name=? ORDER BY started_at DESC`) returns the character's rows newest-first and excludes the other character;
   - `EXPLAIN QUERY PLAN` for that query uses the new index (no `TEMP B-TREE FOR ORDER BY`).

## Non-goals (do NOT do any of these)

- Never edit any shipped migration (v90 and below are frozen). No table/column changes, no data rewrite, no backfill, no changes to `timer_commands.rs` or any other file. Composite key order must match the query (`character_name, server_name, started_at DESC` — not a bare `started_at` index).

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Rebuild ONLY the changed local crate first: `cargo clean -p glogger` (NOT `rm -rf target-bench/` — deps stay warm).
- Run with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench` set.
- `cargo test --lib db::migrations` passes (run from `src-tauri/`), full output returned. (`cargo clippy` runs harness-side; do not run it.)
- Return: the unified diff of your change (`git diff`) plus the full test output.
