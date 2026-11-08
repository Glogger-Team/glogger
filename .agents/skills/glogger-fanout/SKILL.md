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

- One per slice: verify `feature/<slug>` exists on neither side — local (`git branch --list feature/<slug>` empty) nor destination remote (`git ls-remote --heads origin feature/<slug>` empty) — then `herdr worktree create --branch feature/<slug> --base main --no-focus` (an existing branch is checked out as-is, stale commits included — never reuse a slug). Read checkout paths back from the command's JSON response (branch slashes become hyphens on disk: `feature-esc-stack-guard`).
- Split panes in the current tab: `herdr pane split --current --direction right|down --cwd <worktree-path> --no-focus` (wide→right, narrow/tall→down). Split with the slice's worktree as `--cwd` BEFORE `agent start` — start inherits the pane cwd. Parse new pane IDs from `.result.pane.pane_id`.
- Start one agent per pane: `herdr agent start <name> --kind omp --pane <id>` (`omp` is this repo's kind — verify against `herdr agent start --help`; `muse-spark` is a model, not a `--kind` value).
- Dispatch every brief first (`herdr agent prompt <name> "<brief>"` with no `--wait`), then monitor all agents (`herdr agent wait <name>`, `--timeout` requires `--wait` on `prompt` only). Sequential `--wait` stalls each slice behind the last; `wait` can return before a fresh turn starts, so inspect each agent before treating its work as done.

## 3. Brief shape (paste verbatim slots)

Each brief MUST contain: existing checkout path + branch (Do NOT create another worktree) · task + TODO origin · scope file list (touch ONLY these) · explicit non-goals (sibling slices' files) · gates (`vue-tsc` / markdownlint / `cargo check` as applicable) · commit prefix (`impv:`/`fix:`/`docs:`, one small commit) · report contract (files changed, gate output, commit hash).

## 4. Traps (learned 2026-11-08, three-slice run)

- **Pane cwd defaults to main.** Agents spawn in the main checkout, NOT their worktree — their status lines read `main`. Every brief must `cd` to the worktree path first; verify with `git branch --show-current`. Herdr's `agent list` `cwd` field is the launch cwd and never updates — trust the shell, not the listing. (A worker editing main misfired a whole slice once; caught via `git status` in main, moved via patch, main restored.) Order matters: `--cwd` (or `cd`) BEFORE `agent start`, never after — a later shell `cd` cannot move the omp session root, so the status line keeps reading `main` until respawn. Fix = respawn idle workers in their worktrees.
- **Each worktree needs its own `node_modules`.** `npm install` once per checkout (lockfile-pinned). The `npm` shim dies in this shell (`/dev/null`); use `mise exec -- npm install`.
- **Don't trust self-reported hashes.** Workers report stale hashes; read commit IDs from branch HEAD (`git rev-parse --short HEAD`) before freezing reviews.
- **Pre-existing drift is not yours.** `package-lock.json` version stamp and similar worktree dirt: leave uncommitted and say so in reports. But stash/remove it BEFORE freezing a review — with a dirty tree `/skill:frozen-review` reads `git diff HEAD` (drift only) instead of the committed range and the slice commit goes unreviewed — and keep it out of the checkout at teardown (restored drift makes `worktree remove` refuse a dirty tree; move it outside or into an identified stash, and drop it only with explicit authorization). Restore afterward only if the worktree survives.
- **Pre-push hook needs node_modules too.** A worktree without install fails `lint:typecheck` on push (`vue-tsc not recognized`) — install before first push.
- **Pane-close doesn't free worktrees (husks).** Closing a pane leaves the worktree dir locked. Teardown order: stop/close the worker pane first (it still holds the checkout as cwd), then `herdr worktree remove` while the owning workspace still exists (close first → `workspace_not_found`, checkout left registered), then close the workspace (`wX`), then delete files — or `git worktree remove <path>` from the parent checkout after closing. Pane-close alone ends in Permission denied.

## 5. A–D per slice (values: `AGENTS.md` Code review loop)

- **A (parent):** in EACH slice worktree (never `main`): stash pre-existing drift first (drift touching Rust code taints `cargo test --lib`), then `mise run lint:full` + pre-commit self-review checklist before B, re-gated after each fix batch; re-run A if the tree changes before B; record freeze (`rev-parse --short HEAD`, `status --porcelain`, `log --oneline base..HEAD`, `diff --stat`).
- **B (task reviewers, read-only):** 1 reviewer per slice at this size (<100 lines or ≤2 files → 1). First pass: target the full slice range from its `main` base through the frozen HEAD (`/skill:frozen-review <base>..<slice-sha>` — a bare SHA reviews only that commit and the no-marker fallback at the release tag can sweep unrelated `main` commits); later batches use the `review-last` marker delta. Paste frozen hunks into briefs with "do not re-run git diff"; carry facts sheet + three intake categories + report contract. Tag the behaviour slice "may run the thing".
- **Fix batch (pane author, in its worktree):** forward findings only; one batch; re-gate; commit; never point a reviewer at its own fix (delta-only verification pass at most once).
- **C (pane author, local):** capacity first (`coderabbit usage`, CLI ≥0.9.1); `coderabbit review --agent --gate major --base main` — on exhausted quota use the simulated-C fallback (frozen subagent review at major threshold) instead of a paid run. Never C on a revision with D in flight or on the same push as D.
- **D (parent):** push branch → open PR (title carries `impv:`/`fix:`/`docs:` prefix — squash subjects feed `next-version.py`) → `@coderabbitai full review` alone on its line, top-level comment. After findings: `skill://autofix` verbatim, fix batch returns to A gates → B-delta → `@coderabbitai review`. Push mid-D can auto-pause the bot ("branch under active development") — post one `@coderabbitai review`; `already reviewed the last commit` + zero threads is clean.
- **Close:** 0 critical / 0 high / 0 fix-now mediums in changed code. Any commit past the B freeze — even a 4-line D-fix — needs its own delta A/B before merge; no new A–B = the loop silently skips two steps. Marker `review-last` is local, never pushed.
