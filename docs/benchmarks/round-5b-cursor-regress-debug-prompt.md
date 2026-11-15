# Round 5b — log reader replays already-processed lines (pin the fault, then fix)

## Target

A user reports: after a background catch-up finishes late, the log reader
restarts partway through lines it already processed, so recent loot, kills,
or chat rows appear twice. Restarting the app or switching logs makes the
duplicates return until the log file grows past the old offset.

## Change

1. The fault is in cursor persistence: `log_positions::update_position` in
   `src-tauri/src/db/queries.rs`. Read it plus its contrast
   (`force_position`, for legitimate rotation resets) and the call sites in
   `src-tauri/src/coordinator.rs`. The stored cursor must never move
   backward on a normal update — find the guard that fails to enforce this.
2. The existing suite is GREEN with the fault (nothing pins this module) —
   verify with `cargo test --lib db::queries` from `src-tauri/` with
   `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`, and record that.
3. Write a pinning test FIRST (new tests module in `queries.rs`, in-memory
   DB via the crate's migration helper): advance the cursor to a large
   offset, commit a stale smaller offset through the normal update path,
   assert the stored cursor keeps the larger value; assert the legitimate
   reset path still forces downward. Show it FAILING on the current code.
4. Fix with the minimal edit. Allowed files: `src-tauri/src/db/queries.rs`
   (fix + your new tests only).

## Non-goals

No refactors, no changes outside that one file. Do not touch
`src-tauri/target-bench/` (untracked harness scratch). Do not alter
`force_position` semantics or any coordinator call site.

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Fresh-compile proof FIRST: `cargo clean -p glogger` (warm deps kept), then
  all cargo below runs with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`.
- Your new pinning test(s) FAIL before the fix (show the output) and PASS
  after; full `cargo test --lib db::queries` is green post-fix (run from
  `src-tauri/`), full output returned.
- Return: the fault in one sentence, the unified diff (`git diff`), the
  failing-then-passing test output.
- Exactly one tracked file modified (`M src-tauri/src/db/queries.rs`):
  the fix plus your new tests, nothing else.
  (cargo clippy runs harness-side; do not run it.)
