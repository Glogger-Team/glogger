# Benchmark status — resume here

Date: 2026-11-15 (Round 3 complete). Branch: `feature/model-benchmark-2`.
Prior work happened in another session; this file is the handoff. Read
`RESULTS.md` and `HARNESS.md` in this directory first.

## Done

- Round 1 (high thinking, sequential): 4/4 green on the frozen
  friend-presence parser task (`round-1-friend-presence-prompt.md`).
  Smoke validated the harness; explicitly NOT a ranking, no winner.
- Round 2 (minimal thinking, parallel supervised services,
  per-worktree `CARGO_TARGET_DIR`): spark + longcat produced real diffs
  (49 pass each); beam + iceman returned vacuous success with clean trees.
- Recorded in `RESULTS.md`. Repeatable protocol in `skill://model-benchmark`.
- Round 2b (minimal thinking, hardened anti-gaming prompt
  `round-1b-friend-presence-antigaming-prompt.md`): spark/beam/longcat real
  diffs (49/50/49 pass, clippy clean, independently re-run); iceman clean
  tree under a success transcript (phantom-diff fail). Recorded in `RESULTS.md`.
- Round 3 (high thinking, 5 harder tasks): spark 5/5, iceman 4/5 (3c),
  beam 3/5 (3c, 3d); longcat not run (429 rate-limited — quota issue, never
  a model-fail). Discriminator was 3c. Recorded in `RESULTS.md` with prompt
  SHAs.
- Round 4 (high thinking, 2 planted-bug debugging tasks, longcat tabled):
  6/6 pass — single-operator plants did not discriminate.
- Round 5 (high thinking, 2 pin-the-fault tasks, green suite with fault):
  6/6 pass. Format worked (agents wrote the failing test) but single-expression
  faults did not discriminate; next step up is cross-file/state-machine faults.
  Prompts + plants + tally in `RESULTS.md`.
- Harness protocol synced (`HARNESS.md` + skill): max 2 concurrent launches,
  keep `target-bench/` warm with `clean -p` proof, tests in-turn + clippy
  harness-side, score-disk-never-transcript. Cost per run recoverable via
  session-jsonl usage snapshot-delta. sccache REJECTED for the harness
  (0% Rust hits across worktrees + a wrapper crash); kept in `mise.toml` for
  single-checkout dev only.

## Open

- `bench-*` worktrees reset to clean base after Round 5 (faults reverted;
  `target-bench/` warm scratch remains).
  Remove worktrees with `git worktree remove --force <path>` when done.
- Push-hook failure resolved 2026-11-15: the pre-push `lint` set failed only
  because this checkout lacked `node_modules` (`mise exec -- npm install`
  fixed it; full `mise run lint` green). Environmental, not content.
- Headless cost tracking solved: per-message `usage` + `cost.total` in the
  session `.jsonl`; snapshot-before/subtract-after per run (recipe in
  HARNESS.md). Free-tier providers report $0 — use tokens as proxy.
- Longcat still 429 on 2026-11-15 morning (2 probes ~1min apart, same ~33s
  rolling retry-after that never clears). Round 3 longcat cells stay not-run.

## Next (pick one)

1. Round 6: cross-file/state-machine faults, or longcat rerun
   (quota-dependent).
2. Cleanup only: drop the `bench-*` worktrees and close out.
