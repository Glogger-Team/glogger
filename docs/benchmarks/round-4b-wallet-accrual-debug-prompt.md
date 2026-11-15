# Round 4b — wallet payouts never accrue (diagnose + fix)

## Target

A user reports: after anchoring the wallet export and then receiving 10
Councils in chat, the dashboard GOLD estimate still shows the anchored amount
with no gain. Replaying or re-scanning the same session never recovers the
missing Councils; every payout stays absent from the estimated total.

## Change

1. Reproduce with the existing suite: from `src-tauri/`, run
   `cargo test --lib chat_status_dispatch` with
   `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`. At least one test
   currently fails; record which.
2. Find the defect by reading the code under test (the signed wallet-delta
   accrual and its anchor/recency guard). The bug is a 1–3 line logic error
   in the guard or accrual expression — do not restructure, do not touch tests.
3. Fix it with the minimal edit. The ONLY file you may modify is
   `src-tauri/src/chat_status_dispatch.rs` (non-test code only).

## Non-goals

No new tests, no refactors, no changes outside that one file's non-test code.
Do not touch `src-tauri/target-bench/` (untracked harness scratch).

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Fresh-compile proof FIRST: `cargo clean -p glogger` (warm deps kept), then
  all cargo below runs with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`.
- `cargo test --lib chat_status_dispatch` passes (run from `src-tauri/`),
  full output returned — including the previously failing test(s).
- Return: which test(s) failed before the fix, the unified diff
  (`git diff`), and the full test output.
- Exactly one tracked file modified
  (`M src-tauri/src/chat_status_dispatch.rs`) with no test-module changes;
  no other file touched. (cargo clippy runs harness-side; do not run it.)
