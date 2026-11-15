# Parallel benchmark harness

Repeatable protocol for comparing opencode-go subscription models on glogger.

## Roster source (authoritative, offline)

`~/.omp/agent/models.db`, table `model_cache`:

```sql
SELECT json_extract(value,'$.id') AS id
FROM model_cache, json_each(models)
WHERE provider_id LIKE 'opencode-go%'
ORDER BY 1;
```

30 IDs observed 2026-11-15. Active roles in `~/.omp/agent/config.yml`
(`modelRoles`: default muse-spark-1.3-contributor, task beam-free,
advisor iceman-contributor-free, judge jev-1.13-free).

## Launch (one service per model, all at once)

Prerequisites per round: all `bench-<model>` worktrees reset
(`git checkout -- <files>`), prompt file hash recorded.

```bash
export CARGO_TARGET_DIR="<wt>/src-tauri/target-bench"
omp --cwd "<wt>" --model "opencode-go/<id>" --thinking <level> \
  --yolo --no-title --max-time 30m -p "$(cat <prompt-file>)"
```

- One supervised background service per model (visible in `proc://`,
  live output via `read proc://<name>/streams`).
- Per-worktree `CARGO_TARGET_DIR=<wt>/src-tauri/target-bench` isolates
  builds; `target-bench/` is untracked harness scratch, never scored.
- Same frozen prompt file for every run; record its sha256 per round.
- `--max-time 30m` bounds unattended runs; record wall time + exit per run.

## Scoring (gates first, then review)

Per worktree: `git status --porcelain` (exactly one modified file),
`git diff --stat`, `cargo test --lib <module>` + `cargo clippy
--all-targets -- -D warnings` from `src-tauri/` with the worktree's
`CARGO_TARGET_DIR`. Rank: gates-pass first, then fewest review findings
(correct arm placement, channel gate, near-miss rejections, no out-of-scope
edits), then lower wall time. Cost/token spend is not exposed by headless
`-p`; record manually if needed.

## Anti-gaming (learned round 2)

Low-effort runs may claim success while doing nothing. Require in the same
turn: a worktree diff showing the change AND a fresh `target-bench/`
compile proving `cargo test` actually executed. Never score reported
output without the diff.
