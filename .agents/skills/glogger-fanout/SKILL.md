---
name: glogger-fanout
description: Fan out N small disjoint TODO slices, one branch + worktree + Herdr pane each, with per-slice briefs and the A-D review loop to green. Use when the user asks to parallelize backlog items across panes.
---

# Glogger Fanout

Owns the fan-out recipe: TODO slices → branches → worktrees → panes → A–D loops. For the review loop itself see `/skill:frozen-review` (repo values in `AGENTS.md`); for live visual QA see `skill://glogger-smoke`; for release see `skill://glogger-release`.

## 1. Pick slices

- Read `TODO.md` whole (note `:raw:` whole-line reads for truncated bullets).
- Prefer entries marked small with file-level scope; reject report-gated / Large / blocked items.
- Slices MUST be file-disjoint. Name each `feature/<slug>` cut from `main`.
- Confirm `main` clean (`git status --porcelain=v1 --branch`) before cutting.

## 2. Cut worktrees + panes

- One per slice: `herdr worktree create --branch feature/<slug> --base main --no-focus`. Read checkout paths back from the command's JSON response (branch slashes become hyphens on disk: `feature-esc-stack-guard`).
- Split panes in the current tab: `herdr pane split --current --direction right|down --cwd "$PWD" --no-focus` (wide→right, narrow/tall→down). Keep user focus; parse new pane IDs from `.result.pane.pane_id`.
- Start one agent per pane: `herdr agent start <name> --kind omp --pane <id>` (`omp` is this repo's kind — verify against `herdr agent start --help`; `muse-spark` is a model, not a `--kind` value).
- Brief via `herdr agent prompt <name> "<brief>" --wait --timeout <ms>` (`--timeout` requires `--wait`).

## 3. Brief shape (paste verbatim slots)

Each brief MUST contain: existing checkout path + branch (Do NOT create another worktree) · task + TODO origin · scope file list (touch ONLY these) · explicit non-goals (sibling slices' files) · gates (`vue-tsc` / markdownlint / `cargo check` as applicable) · commit prefix (`impv:`/`fix:`/`docs:`, one small commit) · report contract (files changed, gate output, commit hash).

## 4. Traps (learned 2026-11-08, three-slice run)

- **Pane cwd defaults to main.** Agents spawn in the main checkout, NOT their worktree — their status lines read `main`. Every brief must `cd` to the worktree path first; verify with `git branch --show-current`. Herdr's `agent list` `cwd` field is the launch cwd and never updates — trust the shell, not the listing. (A worker editing main misfired a whole slice once; caught via `git status` in main, moved via patch, main restored.)
- **Each worktree needs its own `node_modules`.** `npm install` once per checkout (lockfile-pinned). The `npm` shim dies in this shell (`/dev/null`); use `mise exec -- npm install`.
- **Don't trust self-reported hashes.** Workers report stale hashes; read commit IDs from branch HEAD (`git rev-parse --short HEAD`) before freezing reviews.
- **Pre-existing drift is not yours.** `package-lock.json` version stamp and similar worktree dirt: verify via `git show --stat HEAD` (3 files only = drift excluded), leave uncommitted, say so in reports.
- **Pre-push hook needs node_modules too.** A worktree without install fails `lint:typecheck` on push (`vue-tsc not recognized`) — install before first push.

## 5. A–D per slice (values: `AGENTS.md` Code review loop)

- **A (parent):** `vue-tsc --noEmit` per worktree; markdownlint for docs slices; record freeze (`rev-parse --short HEAD`, `status --porcelain`, `log --oneline base..HEAD`, `diff --stat`).
- **B (task reviewers, read-only):** 1 reviewer per slice at this size (<100 lines or ≤2 files → 1). Paste frozen hunks into briefs with "do not re-run git diff"; carry facts sheet + three intake categories + report contract. Tag the behaviour slice "may run the thing".
- **Fix batch (pane author, in its worktree):** forward findings only; one batch; re-gate; commit; never point a reviewer at its own fix (delta-only verification pass at most once).
- **C (pane author, local):** capacity first (`coderabbit usage`, CLI ≥0.9.1); `coderabbit review --agent --gate major --base main`. Never C on a revision with D in flight or on the same push as D.
- **D (parent):** push branch → open PR (title carries `impv:`/`fix:`/`docs:` prefix — squash subjects feed `next-version.py`) → `@coderabbitai full review` alone on its line, top-level comment. After findings: `skill://autofix` verbatim, fix batch returns to A gates → B-delta → `@coderabbitai review`. Push mid-D can auto-pause the bot ("branch under active development") — post one `@coderabbitai review`; `already reviewed the last commit` + zero threads is clean.
- **Close:** 0 critical / 0 high / 0 fix-now mediums in changed code. Marker `review-last` is local, never pushed.
