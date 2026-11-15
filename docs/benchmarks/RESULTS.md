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

## Round 2b — same smoke, hardened anti-gaming (2026-11-15)

Prompt: `round-1b-friend-presence-antigaming-prompt.md` (sha256
`c262d8bc8d1a31c3ac3f68c4060767554ecc9358ff148272d872d1dd427a564e`):
same frozen task, Acceptance hardened to require `RUN-START`/`RUN-END`,
deleting `src-tauri/target-bench/` first, `CARGO_TARGET_DIR` gates in-turn,
and returning the diff plus full test/clippy output. All four launched at
`--thinking minimal` in parallel; wall times are service launch-to-exit uptimes.

| Model | Worktree diff | Fresh target-bench? | Verified gates (independent re-run) | Wall time | Notes |
|---|---|---|---|---|---|
| muse-spark-1.3 | 63-line, real | yes | 49 pass + clippy exit 0 | 6m31s | Clean: exact-shape channel-gate test, no findings |
| beam-free | 69-line, real | yes | 50 pass + clippy exit 0 | 4m29s | 1 minor: channel-gate test has an extra `Player:` + space prefix (passes even without channel check); 5 separate tests |
| iceman-contributor-free | none (clean) | dir exists, cargo ran | n/a — no scorable diff | RUN-END printed, lingered 8m35s | FAIL phantom-diff: transcript claimed a diff + 50 passed, disk holds no change |
| longcat-2.5 | 75-line, real | yes | 49 pass + clippy exit 0 | 7m18s | Clean + extra quoted-name rejection test; transcript confabulated "preceding turn" continuity, artifacts verify |

Rank (gates, then findings, then wall): spark ~= longcat (0 findings),
beam (1 minor test-shape note), iceman fail.

Lesson: the hardened prompt ended vacuous success — beam did real work this
time — but revealed a new mode: a complete success transcript with no
worktree diff (iceman). Score the disk, never the transcript: a run counts
only with BOTH a tracked diff AND fresh-compile proof, and the transcript is
evidence, not the artifact. Cost/token spend still not captured from
headless `-p`.
