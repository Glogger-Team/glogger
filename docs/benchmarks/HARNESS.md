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

Prerequisites per round: all `bench-<model>` worktrees reset to the base
(`git checkout -- <tracked-files>` only — NEVER delete `target-bench/`;
it stays warm across rounds, deleted only when the base commit moves),
prompt file hash recorded, and one upfront `cargo fetch` from any
`src-tauri/` to warm the shared registry (kills package-cache lock waits).

```bash
export CARGO_TARGET_DIR="<wt>/src-tauri/target-bench"
omp --cwd "<wt>" --model "opencode-go/<id>" --thinking <level> \
  --yolo --no-title --max-time 30m -p "$(cat <prompt-file>)"
```

No `RUSTC_WRAPPER`/sccache in the recipe (measured 2026-11-15: 0% Rust
cache hits across worktrees — every rustc invocation embeds the worktree's
absolute `target-bench` path in `-L`/`--extern`/`--out-dir`, so keys never
match — plus one wrapper crash `0xfffffffe` on a clean tree; C/C++ hits
don't justify the flakiness). Speed comes from the warm `target-bench/`
instead. (`mise.toml` carries sccache for single-checkout dev iteration,
where paths are stable; opt in there with `RUSTC_WRAPPER=sccache`.)

- One supervised background service per model (visible in `proc://`,
  live output via `read proc://<name>/streams`).
- Per-worktree `CARGO_TARGET_DIR=<wt>/src-tauri/target-bench` isolates
  builds; `target-bench/` is untracked harness scratch, never scored.
- Same frozen prompt file for every run; record its sha256 per round.
- `--max-time 30m` bounds unattended runs; record wall time + exit per run.

## Scoring (gates first, then review)

Per worktree: `git status --porcelain` (exactly one modified file),
`git diff --stat`. Agents run `cargo test --lib <module>` in-turn (proof of
execution); the harness runs `cargo clippy --all-targets -- -D warnings`
at score time from `src-tauri/` with the worktree's `CARGO_TARGET_DIR` +
`RUSTC_WRAPPER` (no need to spend agent wall time on the second world-build).
Rank: gates-pass first, then fewest review findings
(correct arm placement, channel gate, near-miss rejections, no out-of-scope
edits), then lower wall time. Cost/token spend is not exposed by headless
`-p`; record manually if needed.

## Anti-gaming (learned rounds 2–2b)

Low-effort runs may claim success while doing nothing — and one round-2b
transcript claimed a full diff + passing gates with a clean tree
(phantom-diff). Score the disk, never the transcript: a run counts only
with BOTH a tracked worktree diff AND fresh-compile proof, and the
transcript is evidence, not the artifact.
Fresh-compile proof WITHOUT nuking the warm cache: the prompt orders
`cargo clean -p glogger` (rebuild only the changed local crate, deps stay
warm) instead of deleting `target-bench/`, then `cargo test` in the same
turn under the worktree's `CARGO_TARGET_DIR` + `RUSTC_WRAPPER`. Full
`target-bench/` deletion only when the base commit moves.
