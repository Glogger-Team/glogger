# Round 5a — arena fight recorded twice (pin the fault, then fix)

## Target

A user reports: after a casino fight finishes and its result is announced,
hearing the same announcement repeated records the fight a second time. The
dashboard then shows two identical results for a single fight.

## Change

1. Run the existing suite first: from `src-tauri/`, `cargo test --lib arena`
   with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`. It is GREEN —
   that is the trap. No existing test observes any line after a resolution,
   so the fault hides. Record the green result.
2. Read `ArenaTracker::observe` in `src-tauri/src/arena_parser.rs` (intro-pair
   state machine + result resolution). Find why a repeated result line
   re-emits an already-recorded fight.
3. Write a pinning test FIRST in the file's existing tests module proving the
   fault (resolve once, repeat the same result line, assert the repeat emits
   nothing). Show it FAILING on the current code.
4. Fix with the minimal edit. Allowed files: `src-tauri/src/arena_parser.rs`
   (fix + your new test only — no other test touched).

## Non-goals

No refactors, no changes outside that one file. Do not touch
`src-tauri/target-bench/` (untracked harness scratch).

## Acceptance (all REQUIRED in this same turn — a run missing any one scores as fail)

- Print RUN-START as your first line, RUN-END as your last line.
- Fresh-compile proof FIRST: `cargo clean -p glogger` (warm deps kept), then
  all cargo below runs with `CARGO_TARGET_DIR=<worktree>/src-tauri/target-bench`.
- Your new pinning test FAILS before the fix (show the output) and PASSES
  after; full `cargo test --lib arena` is green post-fix (run from
  `src-tauri/`), full output returned.
- Return: the fault in one sentence, the unified diff (`git diff`), the
  failing-then-passing test output.
- Exactly one tracked file modified (`M src-tauri/src/arena_parser.rs`):
  the fix plus your new test, nothing else.
  (cargo clippy runs harness-side; do not run it.)
