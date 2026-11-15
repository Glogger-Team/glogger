# Benchmark task (frozen — implement exactly, no scope changes)

## Target

- `src-tauri/src/chat_status_dispatch.rs` — dedupe gate inside `accrue_currency_delta`.
- `src-tauri/src/db/migrations.rs` — append-only v91 migration for the seen-table.

These are the ONLY files you may modify.

## Change

Wallet-delta dispatch is the one non-idempotent arm: `accrue_currency_delta`
is a bare `UPDATE currency_estimate SET delta_since = delta_since + ?`
guarded only by the export anchor, with no per-line memory. A crash between
the chat drain and the watcher-position save re-drains the same lines and
double-accrues. (`count_currency: false` scan paths must still accrue
nothing; the `> anchor_at` guard and `amount == 0` early return stay.)

1. New migration (append-only, never touch shipped versions): `v91` creates
   `currency_accrual_seen(event_timestamp TEXT, character_name TEXT, server_name TEXT, amount INTEGER, PRIMARY KEY(event_timestamp, character_name, server_name, amount))`
   via `CREATE TABLE IF NOT EXISTS`, wired as
   `if current_version < 91 { migration_v91_currency_accrual_seen(conn)?; super::record_migration(conn, 91)?; }`
   in `run_migrations` after the v90 block. (Note: this battery's index task
   also uses v91 in its own isolated worktree; the two never meet. Follow
   the exact code here.)
2. In `accrue_currency_delta`, before the `UPDATE`: `INSERT OR IGNORE` the
   `(event_ts, character, server, amount)` key into `currency_accrual_seen`;
   accrue only when the insert wins (i.e. the line was not seen before).
   Same-connection ordering (insert-then-update) so a crash between drains
   cannot double-count.
3. Tests in the in-file `mod tests`, using the existing helpers
   (`status_msg(text, ts)`, `state()` with `count_currency: true`,
   `state_scan()`, `dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st)`) and
   mirroring the existing anchor seeding (`INSERT INTO currency_estimate ...
   VALUES ('Alice','Dreva','GOLD',1000,'2026-04-01 05:00:00',0)` with an event
   ts newer than the anchor):
   - dispatch the identical `"You received 10 Councils."` line 3× with
     `count_currency: true` → `delta_since == 10` (not 30);
   - dispatch a second distinct wallet line → it adds;
   - `count_currency: false` still accrues nothing.
   Genuine distinct lines (including same-second repeats with different
   amounts) must each count — key on the FULL natural identity
   `(event_timestamp, character, server, amount)`, never timestamp alone.

## Non-goals (do NOT do any of these)

- No changes to the coordinator, `count_currency` plumbing, the anchor guard, any other dispatch arm, or any other file. Do not move the gate to callers. Do not "verify" with a single-dispatch test (it cannot catch double-apply).

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Rebuild ONLY the changed local crate first: `cargo clean -p glogger` (NOT `rm -rf target-bench/` — deps stay warm).
- Run with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench` set.
- `cargo test --lib chat_status_dispatch` passes (run from `src-tauri/`), full output returned. (`cargo clippy` runs harness-side; do not run it.)
- Return: the unified diff of your change (`git diff`) plus the full test output.
