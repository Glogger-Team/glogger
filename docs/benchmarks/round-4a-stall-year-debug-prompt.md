# Round 4a — stall sale credited to the wrong year (diagnose + fix)

## Target

A user reports: two stall sales from the same afternoon show up a year apart
in the Stall Tracker history. The day's revenue total is split — the
later-logged sale is attributed to next year and missing from the
current-year view.

## Change

1. Reproduce with the existing suite: from `src-tauri/`, run
   `cargo test --lib stall_year_resolver` with
   `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`. At least one test
   currently fails; record which.
2. Find the defect by reading the code under test (pure year-assignment
   logic for year-less shop-log timestamps). The bug is a 1–3 line logic
   error in the rollover guard — do not restructure, do not touch tests.
3. Fix it with the minimal edit. The ONLY file you may modify is
   `src-tauri/src/stall_year_resolver.rs`.

## Non-goals

No new tests, no refactors, no changes outside that one file. Do not touch
`src-tauri/target-bench/` (untracked harness scratch).

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Fresh-compile proof FIRST: `cargo clean -p glogger` (warm deps kept), then
  all cargo below runs with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`.
- `cargo test --lib stall_year_resolver` passes (run from `src-tauri/`),
  full output returned — including the previously failing test(s).
- Return: which test(s) failed before the fix, the unified diff
  (`git diff`), and the full test output.
- Exactly one tracked file modified
  (`M src-tauri/src/stall_year_resolver.rs`); no test file touched.
  (cargo clippy runs harness-side; do not run it.)
