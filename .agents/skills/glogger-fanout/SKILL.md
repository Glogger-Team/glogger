---
name: glogger-fanout
description: Fan out N small disjoint TODO slices, one branch + git worktree + Herdr pane each; parent orchestrates only, panes execute. Use when the user asks to parallelize backlog items across panes.
---

# Glogger Fanout

## 0. Orchestrator contract (read first)

- Parent NEVER works a slice directly: no edits, no gates, no reviews, no pushes, no bot commands in any slice checkout. Parent cuts worktrees, opens panes, writes briefs, dispatches prompts, polls verdicts, merges clean slices, tears down.
- All slice execution (A gates, B/C/D loops, fix batches, pushes, PR bodies, bot posts) happens in the owning pane via `herdr agent prompt <name>`. A pane blocked on a parent decision (rebase, merge order, scope call) states the decision needed and stops — parent decides, pane resumes.
- One agent per pane, one pane per slice. No mirror panes, no second agent in a slice checkout: two writers in one checkout caused the 11-08 over-close (parent closed a mirror it owned, killing the worker's tab).

## 1. Pick slices

- Read `TODO.md` whole (note `:raw:` whole-line reads for truncated bullets).
- Prefer entries marked small with file-level scope; reject report-gated / Large / blocked items.
- Slices MUST be file-disjoint. Name each `feature/<slug>` cut from `main`.
- Confirm `main` clean (`git status --porcelain=v1 --branch`) before cutting.

## 2. Cut worktrees (plain git) + split one pane each

- One per slice, from the `main` checkout via `eval` (git is off the `bash`-tool PATH): verify `feature/<slug>` exists on neither side — local (`git branch --list feature/<slug>` empty) nor remote (`git ls-remote --heads origin feature/<slug>` empty) — then `git worktree add ../.herdr/worktrees/glogger/<slug-with-hyphens> -b feature/<slug> main` (an existing branch is checked out as-is, stale commits included — never reuse a slug). Confirm with `git worktree list`.
- Do NOT use `herdr worktree create` or `herdr worktree open`: both create a *workspace* (left-sidebar entry), not a pane in the current view — verified 2026-11-09 (`open` takes `--path` XOR `--branch`; the opened `w31:p1` never surfaced visibly). The visible pane comes from the split below.
- Split one pane per slice into the current tab: `herdr pane split --current --direction right|down --cwd <worktree-path> --no-focus` (wide→right, narrow/tall→down; read the new pane ID back from `.result.pane.pane_id`). The split `--cwd` is the agent's session root — verified live (`w2N:pG` opened visibly at the scratch checkout). No later shell `cd` can move the omp session root.
- Start one agent per pane: `herdr agent start <name> --kind omp --pane <id> --timeout 120000` (`omp` is this repo's kind — verify against `herdr agent start --help`; `muse-spark` is a model, not a `--kind` value). If the pane sits at a bare shell prompt the start still binds (idle/unknown → interactive_ready).
- Dispatch every brief first (`herdr agent prompt <name> "<brief>"`, no `--wait`), then monitor (`herdr agent wait <name>` takes `--until <state>` — `--state` is rejected — and bare `wait` matches idle/done/blocked). Sequential `--wait` stalls each slice behind the last; `wait` can return before a fresh turn starts, so `herdr pane read <id>` each agent before treating its work as done.

## 3. Brief shape (paste verbatim slots)

Each brief MUST contain: existing checkout path + branch (Do NOT create another worktree) · task + TODO origin · scope file list (touch ONLY these) · explicit non-goals (sibling slices' files) · gates (`vue-tsc` / markdownlint / `cargo check` as applicable) · commit prefix (`impv:`/`fix:`/`docs:`, one small commit) · report contract (files changed, gate output, commit hash).

## 4. Traps (learned 2026-11-08/09, fanout runs)

- **One pane per checkout — never split a second pane in the same checkout.** Each slice gets exactly one split pane; a second pane in the same checkout is a second writer and a close-the-wrong-one hazard (11-08 over-close: parent closed a mirror it owned, killing the worker's tab). Parent closes a slice pane only after that slice is merged: close the worker pane (`herdr pane close <id>`), remove the checkout with plain git (`git worktree remove --force <path>`, never `herdr worktree remove` — slice workspaces auto-vanish and it errors `workspace_not_found`), delete the branch, confirm with `git worktree list`. If removal hits transient Permission denied (Herdr still releasing the cwd handle — seen 11-09 on a just-closed pane), retry once after a beat; the husk dir deletes by hand once empty.
- **Pane cwd IS the worktree — verify, don't cd.** The split `--cwd` sets the pane root and `agent start` inherits it; no later shell `cd` can move the omp session root. Trust the shell (`git branch --show-current` in-pane), not Herdr's `agent list` cwd (launch cwd, never updates).

- **Each worktree needs its own `node_modules` — install before first gate AND first push.** `mise exec -- npm install` once per checkout (lockfile-pinned, ~10s; bare `npm` dies in the `bash` tool — use `npm.cmd`, see AGENTS.md Gotchas). Without it `vue-tsc` fails both the A gate and the pre-push `lint:typecheck`.
- **Don't trust self-reported hashes.** Workers report stale hashes; read commit IDs from branch HEAD (`git rev-parse --short HEAD`) before freezing reviews.
- **Pre-existing drift (e.g. `package-lock.json` stamp) is never yours — preserve it outside the checkout, never silently drop it.** Stash/move it before A (drift touching Rust taints `cargo test`; a dirty tree makes frozen-review read drift instead of the range) and before B-freeze; re-gate if the tree changes. At teardown it blocks `worktree remove` — same handling. Drop only with explicit authorization; say so in reports.

## 5. A–D per slice (values: `AGENTS.md` Code review loop)

- **A (pane executes, parent verifies):** worker runs drift handling, `mise run lint:full` + self-review checklist, re-gates after each fix batch, records freeze (`rev-parse --short HEAD`, `status --porcelain`, `log --oneline base..HEAD`, `diff --stat`) in its report. Parent verifies the freeze from `main` (branch HEAD, `git worktree list`) — never re-runs slice gates. Re-run A if the tree changes before B.
- **B (parent dispatches reviewers, read-only):** 1 reviewer per slice at this size (<100 lines or ≤2 files → 1). Parent spawns `task` reviewers itself (non-isolated, read-only `scout`/`reviewer`) — workers never review. First pass: target the full slice range from its `main` base through the frozen HEAD (`/skill:frozen-review <base>..<slice-sha>` — a bare SHA reviews only that commit and the no-marker fallback at the release tag can sweep unrelated `main` commits); if the slice froze as multiple commits (worker + stage-A fix), record a B verdict per commit at each matching frozen revision. Later batches use the `review-last` marker delta. Paste frozen hunks into briefs with "do not re-run git diff"; carry facts sheet + three intake categories + report contract. Tag the behaviour slice "may run the thing".
- **Fix batch (pane author, in its worktree):** parent forwards findings only; one batch; worker re-gates, commits, reports. Never point a reviewer at its own fix (delta-only verification pass at most once).
- **C (pane executes):** worker checks capacity (`coderabbit usage`, CLI ≥0.9.1), runs `coderabbit review --agent --gate major --base main` in-pane — on exhausted quota the simulated-C fallback (frozen subagent review at major threshold) instead of a paid run. Never C on a revision with D in flight or on the same push as D. Parent only verifies the exit code from the report.
- **D (pane executes, parent merges):** worker pushes branch → opens PR (title carries `impv:`/`fix:`/`docs:` prefix — squash subjects feed `next-version.py`) → posts `@coderabbitai full review` alone on its line, top-level comment, and polls to verdict. After findings: worker runs `skill://autofix` verbatim; fix batch returns to A gates → B-delta (parent re-dispatches) → worker posts `@coderabbitai review`. Push mid-D can auto-pause the bot ("branch under active development") — post one `@coderabbitai review`; `already reviewed the last commit` + zero threads is clean. Parent merges only A-D-clean slices (fast-forward, PR-only `main` — never push `main` directly).
- **Close:** 0 critical / 0 high / 0 fix-now mediums in changed code. Any commit past the B freeze — even a 4-line D-fix — needs its own delta A/B before merge; no new A–B = the loop silently skips two steps. Marker `review-last` is local, never pushed.
