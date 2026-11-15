# Model benchmark results

Fronzen task: `round-1-friend-presence-prompt.md` (add `FriendPresence` arm to
`src-tauri/src/chat_status_parser.rs`, tests + clippy).

## Round 1 — high thinking, sequential (2026-11-15)

Same frozen prompt, one `omp --cwd bench-<model> --model <id> -p` at a time.

| Model | Tests | Clippy | Wall time | Notes |
|---|---|---|---|---|
| muse-spark-1.3-contributor | 49 pass | clean | 388s | Baseline-correct; fresh-worktree cold compile ~18s included |
| beam-free | 50 pass | clean | 214s | Fastest; warm cache (0.5s test, 0.6s clippy); independently re-verified |
| iceman-contributor-free | 50 pass | clean | 269s | Tests appended at file end, cosmetic only; hit cargo package-cache lock waits |
| longcat-2.5-free | 49 pass | clean | 286s | Only run to trip a real gate (`clippy::question_mark`), self-fixed; extra quoted-name test beyond spec |

All 4 green: smoke validated the harness, not a ranking. No promotion.

## Round 2 — minimal thinking, parallel (2026-11-15)

Same prompt + `RUN-START`/`RUN-END` markers, 4 supervised `omp -p` services at once,
`--thinking minimal`, per-worktree `CARGO_TARGET_DIR=<wt>/src-tauri/target-bench`.
`target-bench/` is harness scratch (untracked, do not score or commit).

| Model | Worktree diff | Verified gates | Wall time | Notes |
|---|---|---|---|---|
| muse-spark-1.3 | 63-line diff, real | 49 pass (independently re-run) | — | Genuine low-effort implementation |
| beam-free | clean (no work) | none run | — | Claimed "already implemented"; vacuous success |
| iceman | clean (no work) | none run | — | Same vacuous-success failure mode |
| longcat-2.5 | 75-line diff, real | 49 pass + clippy exit 0 in transcript | — | Re-applied byte-identical diff after reset; clippy shape fix retained |

Lesson: minimal thinking produced *skipped work with success claims*, not failing
tests. Harness fix: runs must execute `cargo test` in the same turn (fresh
`target-bench/` compile proves it); never trust reported output without a
worktree diff. Cost/token spend not captured from headless `-p`; record manually
next round.
