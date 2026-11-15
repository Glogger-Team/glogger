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

## Round 3 — harder battery, high thinking (2026-11-15)

Five harder frozen tasks, three models (longcat dropped for the night: 4/4
launches 429 rate-limited — a longcat-specific quota, other models fine — so
its cells are "not run", never model-fail). Max 2 concurrent launches
(HARNESS.md now mandates it). Prompts require in-turn tests with a
fresh-compile proof (`cargo clean -p glogger`, warm `target-bench/` kept);
clippy runs harness-side at score time. All gates below were independently
re-run from the worktree with its own `CARGO_TARGET_DIR`.

Prompt SHAs (all in this directory):

| Task | File | sha256 (short) |
|---|---|---|
| 3a widget | `round-3a-active-effects-widget-prompt.md` | `4e837e2a…` |
| 3b migration | `round-3b-user-timers-index-prompt.md` | `03a5ca39…` |
| 3c replay | `round-3c-replay-safe-dispatch-prompt.md` | `e82e7b3c…` |
| 3d timer IPC | `round-3d-timer-ipc-prompt.md` | `1279c63d…` |
| 3e pinia | `round-3e-pinia-refresh-prompt.md` | `70b2f4ec…` |

3b and 3c both specify migration v91 in isolated worktrees reset to the same
base — the number collides on paper, never on disk.

### 3a — ActiveEffects widget (typecheck + build gates)

| Model | Diff | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | real | typecheck 0 + build 0 | PASS, no findings |
| beam-free | real | typecheck 0 + build 0 | PASS; style nit (double-quote/semicolon) |
| iceman | real | typecheck 0 + build 0 | PASS; transcript never arrived, scored by disk + gates (service went silent after writing artifacts) |
| longcat-2.5 | real | typecheck FAIL (`TS2322` `:title="effect.effect_name"` string\|null vs string\|undefined) + unreverted 2-line `package-lock.json` drift | FAIL |

### 3b — user_timers started_at index migration (25–27 tests + clippy)

| Model | Diff | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | 101-line, real | 25/25 `db::migrations` + clippy 0 | PASS. `rm -rf`'d target-bench despite prompt ordering `clean -p` — stronger proof, no penalty |
| beam-free | real | 27/27 + clippy 0 | PASS; three focused tests (exists / ordering+isolation / plan) |
| iceman | real | 27/27 + clippy 0 | PASS; nit: missing blank line before V79 comment |
| longcat-2.5 | — | — | not run (rate-limited) |

### 3c — replay-safe currency dispatch (dispatch + migrations + clippy)

| Model | Diff | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | real | 10/10 `chat_status_dispatch` + migrations + clippy 0 | PASS. Anchor-guarded `INSERT...SELECT WHERE EXISTS`: caught the pre-anchor key-burn trap (unconditional claim breaks the existing scan test — pre-anchor drain burns the key, post-anchor accrual skipped) |
| beam-free | real | dispatch 9/9 (all pre-existing); migrations FAILED (`run_migrations_is_idempotent_and_atomic` — first head assert still expects 90) | FAIL: ships a red suite. Zero new dispatch tests, so its gate is unexercised |
| iceman | real | dispatch FAILED (`scan_persists_status_lines_like_live_drain` — unconditional claim burns the key pre-anchor); migrations FAILED (same stale head pin) | FAIL: behavioral + gate |
| longcat-2.5 | — | — | not run (rate-limited) |

### 3d — timer IPC store (typecheck + review; no frontend test harness exists)

| Model | Diff | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | `timerStore.ts` only | typecheck clean | PASS: async actions, `presetId` carried across loads, no-identity local fallback, failures warn with state untouched |
| beam-free | `timerStore.ts` only | typecheck clean | FAIL: `loadUserTimers` maps rows without `presetId` carry-over, violating the explicit "never overwritten by loads" rule. (Sync-preserving fire-and-forget persists and `started_at` reconstruction are good.) |
| iceman | `timerStore.ts` only | typecheck clean | PASS: `presetId` preserved everywhere, regex-validated `parseLocal`, correct remove semantics. (Transcript claimed "no changes required" while the disk holds 248 insertions — score the disk.) |
| longcat-2.5 | — | — | not run (rate-limited) |

### 3e — Pinia work-orders refresh (typecheck + review)

| Model | Diff | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | `characterStore.ts` only | typecheck clean | PASS: `wasLoaded` captured before invalidate, reload after `selectSnapshot` block, exact placement |
| beam-free | `characterStore.ts` only | typecheck clean | PASS with note: reload nested one level deeper (inside `if (snapshots.length > 0)`) — skips the reload in the empty-snapshots edge; identical in all realistic paths |
| iceman | `characterStore.ts` only | typecheck clean | PASS: minimal, exact placement |
| longcat-2.5 | — | — | not run (rate-limited) |

Round 3 tally (longcat excluded — not run): spark 5/5, iceman 4/5 (3c),
beam 3/5 (3c, 3d). The discriminating task was 3c: only spark saw that the
dedup claim itself must carry the anchor guard, because the pre-existing
scan test drains the same line before any anchor exists.

## Round 4 — debugging, high thinking (2026-11-15)

Two planted-bug tasks (longcat tabled — still 429). The harness plants the
identical 1-line mutation in each worktree before launch (frozen by
construction); agents must record the pre-fix failure, apply the minimal
fix, and show green module tests with fresh-compile proof (`cargo clean -p
glogger`, warm `target-bench/`); clippy harness-side. Correct fix restores
HEAD content, so a clean tree here is proof of fix — distinguished from
vacuous-clean by the pre-fix failure evidence plus independent gate re-runs.

Prompt SHAs: 4a stall-year `bedf522d…`, 4b wallet-accrual `2daf7055…`.
Plants (verified pre-freeze — each breaks exactly its target test):
4a `<` → `<=` in the rollover guard (`stall_year_resolver.rs:76`);
4b `?4 > anchor_at` → `?4 < anchor_at` (`chat_status_dispatch.rs:546`).

### 4a — stall sale credited to the wrong year (7 tests)

| Model | Pre-fix failure recorded | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | `same_day_out_of_order_does_not_bump` (left `2027-04-13`, right `2026-04-13`) | 7/7 + clippy 0 | PASS. One-line fix + clarifying comment; cited the module docs. First launch attempt died at service boot with no transcript — clean retry succeeded |
| beam-free | same test | 7/7 + clippy 0 | PASS. Minimal revert, no comment |
| iceman | (transcript never arrived) | 7/7 + clippy 0 | PASS by disk + gates (plant gone, green suite). Residual uncertainty: no transcript, so the diagnosis path is unverified — same pattern as its 3a/3b silences |

### 4b — wallet payouts never accrue (9 tests)

| Model | Pre-fix failure recorded | Verified gates | Notes |
|---|---|---|---|
| muse-spark-1.3 | `scan_persists_status_lines_like_live_drain` (left 0, right 10) | 9/9 + clippy 0 | PASS. Honest clean-tree report (fix restores HEAD; refused to pad the diff). Strongest transcript of the round |
| beam-free | same test | 9/9 + clippy 0 | PASS. Fix byte-identical to HEAD modulo CRLF — its editor wrote CRLF into an LF file (status `M` with empty diff); hygiene nit, not scored |
| iceman | same test, captured by deliberate revert (`<` in, fail, `>` back, pass) | 9/9 + clippy 0 | PASS. Exemplary transcript: pre/post failure-pass pair, clean-p proof, minimal diff |

Round 4 tally: 6/6 pass. Debugging did not discriminate — both plants were
single-operator inversions caught by the first test run. A harder battery
would need multi-step faults (wrong behavior with green existing tests,
requiring a new test to pin it).
