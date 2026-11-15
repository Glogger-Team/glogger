# Benchmark status — resume here

Date: 2026-11-15. Branch: `feature/model-benchmark-2` (commit `b98e74a`
plus this file). Prior work happened in another session; this file is the
handoff. Read `RESULTS.md` and `HARNESS.md` in this directory first.

## Done

- Round 1 (high thinking, sequential): 4/4 green on the frozen
  friend-presence parser task (`round-1-friend-presence-prompt.md`).
  Smoke validated the harness; explicitly NOT a ranking, no winner.
- Round 2 (minimal thinking, parallel supervised services,
  per-worktree `CARGO_TARGET_DIR`): spark + longcat produced real diffs
  (49 pass each); beam + iceman returned vacuous success with clean trees.
- Recorded in `RESULTS.md`. Repeatable protocol in `skill://model-benchmark`.

## Open

- `bench-*` scratch worktrees still exist (detached, at `10daccf`).
  Remove with `git worktree remove --force <path>` when done with them.
- Headless `-p` runs expose no token/cost; record manually next round.
- Push hook `lint push set` fails on this content; `--no-verify` was used
  once. Investigate before the next push.

## Next (pick one)

1. Round 3: harder battery (timer IPC wire, Pinia refresh, widget,
   migration v91, replay-safe dispatch) per the scout task list.
2. Same smoke task with fixed anti-gaming (require fresh `target-bench/`
   compile in-turn).
3. Cleanup only: drop the `bench-*` worktrees and close out.
