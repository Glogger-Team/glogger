---
name: glogger-fanout
description: Fan out N small disjoint TODO slices, one branch + git worktree + worker each; parent orchestrates only, workers execute. Use when the user asks to parallelize backlog items.
---

# Glogger Fanout

## 0. Orchestrator contract (read first)

- Parent NEVER works a slice directly: no edits, no gates, no pushes, no bot commands in any slice checkout. Parent cuts worktrees, launches workers, writes briefs, dispatches prompts AND review rounds, polls verdicts, merges clean slices, tears down.
- All slice execution (A gates, C/D loops, fix batches, pushes, PR bodies, bot posts) happens in the owning worker. B reviewers are parent-spawned (`task` scout/reviewer, non-isolated, read-only) — workers never review. A worker blocked on a parent decision (rebase, merge order, scope call) states the decision needed and stops — parent decides, worker resumes.
- One worker per slice, one slice per checkout. No second writer in a slice checkout: two writers in one checkout caused the 11-08 over-close (parent closed a mirror it owned, killing the worker's tab).

## 1. Pick slices

- Read `TODO.md` whole (note `:raw:` whole-line reads for truncated bullets).
- Prefer entries marked small with file-level scope; reject report-gated / Large / blocked items.
- Slices MUST be file-disjoint. Name each `feature/<slug>` cut from `main`.
- Confirm `main` clean (`git status --porcelain=v1 --branch`) before cutting.

## 2. Cut worktrees (plain git) + launch one worker each

- Two transports. Default is **2A (`hc`, headless)** — no visible panes unless the user asked to watch. **2B (TUI)** is legacy, only when `herdr status` shows a running server (verified down on this box 2026-11-10 — 2B is dead until the user starts Herdr).
- Worktree cut is identical on both paths, from the `main` checkout via `eval` (git is off the `bash`-tool PATH): verify `feature/<slug>` exists on neither side — local (`git branch --list feature/<slug>` empty) nor remote (`git ls-remote --heads origin feature/<slug>` empty) — then `git worktree add ../.herdr/worktrees/glogger/<slug-with-hyphens> -b feature/<slug> main` (an existing branch is checked out as-is, stale commits included — never reuse a slug). Confirm with `git worktree list`. Do NOT use `herdr worktree create` / `open`: both make a sidebar workspace, not a worker.

### 2A. `hc` headless workers (default)

- Prereq: `hc ls` answers (core runs standalone, no Herdr server needed — verified live 2026-11-10: `run` → `wait` → `read` → `rm` round-trips). Run `hc` via `eval`, same PATH reason as git.
- Launch one worker per slice with the brief inline: `hc run --bg --name <slice> --cwd <worktree-path> -- omp "<brief>"`. The brief is omp's first message — a `send` to a booting agent can be dropped, so never launch bare then `send` the task. Interactive only, never `omp -p`; no subagents unless the brief assigns them. Record the returned address (`tN`) — names are unique among live terminals (`[a-z0-9_-]{1,32}`). (`--kind omp` is a TUI concept; on `hc` the program IS the command — verified `agent: omp` on a plain `hc run -- omp ...` launch.)
- If the launch carried a task, check `first_message.sent` in `hc get <t>`; `false` means the task was NOT sent — `hc read` first, resolve readiness, then `send`. Never blind-resend: it may still have arrived.
- Monitor with ONE collective wait, not sequential per-slice waits: `hc wait --children all` (next news from any slice) or per-slice `hc wait <t>`. The default 180s timeout is a check-in, not failure — wait again. Child reports arrive tagged `<hc from="tN name">…</hc>` as parent input — read the actual result, never infer success from an expected outcome.
- Read/steer: `hc read --lines 100 <t>`, `hc get <t>`, `hc send <t> "<follow-up>"`. Fix batches go via `send` (same no-blind-resend rule: `read` before re-`send` after a stall/timeout).
- Visibility — ONLY if the user asked to watch: `herdr-app ls` for space/tab IDs, then adopt the worker with `hc label <t> herdr.space=<id>` (+ `herdr.tab=<id>` to pick a tab), or `herdr-app do tab.new --name <slice> --argv ... --task ...`. Confirm exact shapes via `herdr-app commands` first. Never add labels just for background workers.

### 2B. TUI visible panes (legacy)

- Split one pane per slice into the current tab: `herdr pane split --current --direction right|down --cwd <worktree-path> --no-focus` (wide→right, narrow/tall→down; read the new pane ID back from `.result.pane.pane_id`). The split `--cwd` is the agent's session root — verified live (`w2N:pG` opened visibly at the scratch checkout). No later shell `cd` can move the omp session root.
- Start one agent per pane: `herdr agent start <name> --kind omp --pane <id> --timeout 120000` (`omp` is this repo's kind — verify against `herdr agent start --help`; `muse-spark` is a model, not a `--kind` value). If the pane sits at a bare shell prompt the start still binds (idle/unknown → interactive_ready).
- Dispatch every brief first (`herdr agent prompt <name> "<brief>"`, no `--wait`), then monitor (`herdr agent wait <name>` takes `--until <state>` — `--state` is rejected — and bare `wait` matches idle/done/blocked). Sequential `--wait` stalls each slice behind the last; `wait` can return before a fresh turn starts, so `herdr pane read <id>` each agent before treating its work as done.
- Read pane buffers with flags: `herdr pane read <id> --lines 60 --format text` (bare JSON mode breaks on terminal graphics — `Unrecognized token '│'`; `--format text` is the only parseable form).

## 3. Brief shape (paste verbatim slots)

Each brief MUST contain: existing checkout path + branch (Do NOT create another worktree) · task + TODO origin · scope file list (touch ONLY these) · explicit non-goals (sibling slices' files) · gates (`vue-tsc` / markdownlint / `cargo check` as applicable) · commit prefix (`impv:`/`fix:`/`docs:`, one small commit) · report contract (files changed, gate output, commit hash). On the `hc` path, end the brief with: `Report back with hc send $HC_TERMINAL_ID <result or blocker> when done or stuck.` (Workers are children of the parent terminal — the core records the parent automatically and routes the note.)

## 4. Traps (learned 2026-11-08/09, fanout runs)

- **One worker per checkout — never a second writer in the same checkout.** Transport-agnostic version of the 11-08 rule: on `hc` that means no second `run` in a slice worktree; on TUI no second split pane in it. Parent removes a slice worker only after that slice is merged: `hc kill <t>; hc rm <t>` (read results first — `rm` on an unread terminal discards its scrollback), or on TUI close the worker pane (`herdr pane close <id>`) plus `pane.close`/`tab.close` for any opt-in visibility panes. Then remove the checkout with plain git (`git worktree remove --force <path>`, never `herdr worktree remove` — slice workspaces auto-vanish and it errors `workspace_not_found`), delete the branch, confirm with `git worktree list`. If removal hits transient Permission denied (something still holding the cwd handle — seen 11-09 on just-closed panes), retry once after a beat; the husk dir deletes by hand once empty.
- **Worker cwd IS the worktree — verify, don't cd.** `--cwd` on `hc run` (or split `--cwd` on TUI) sets the root; trust the shell (`git branch --show-current` in the worker), not the launch-time listing.
- **`hc wait` returning is not verdict.** `reason: timeout` is a check-in (wait again); `reason: message` arrives as parent input, not as `read` output — process it, then wait again if slices remain. Never sleep/poll around `hc`.
- **Each worktree needs its own `node_modules` — install before first gate AND first push.** `mise exec -- npm install` once per checkout (lockfile-pinned, ~10s; bare `npm` dies in the `bash` tool — use `npm.cmd`, see AGENTS.md Gotchas). Without it `vue-tsc` fails both the A gate and the pre-push `lint:typecheck`.
- **Don't trust self-reported hashes.** Workers report stale hashes; read commit IDs from branch HEAD (`git rev-parse --short HEAD`) before freezing reviews.
- **Pre-existing drift (e.g. `package-lock.json` stamp) is never yours — preserve it outside the checkout, never silently drop it.** Stash/move it before A (drift touching Rust taints `cargo test`; a dirty tree makes frozen-review read drift instead of the range) and before B-freeze; re-gate if the tree changes. At teardown it blocks `worktree remove` — same handling. Drop only with explicit authorization; say so in reports.

## 5. A–D per slice (values: `AGENTS.md` Code review loop)

- **A (worker executes, parent verifies):** worker runs drift handling, `mise run lint:full` + self-review checklist, re-gates after each fix batch, records freeze (`rev-parse --short HEAD`, `status --porcelain`, `log --oneline base..HEAD`, `diff --stat`) in its report. Parent verifies the freeze from `main` (branch HEAD, `git worktree list`) — never re-runs slice gates. Re-run A if the tree changes before B.
- **B (parent dispatches reviewers, read-only):** 1 reviewer per slice at this size (<100 lines or ≤2 files → 1). Parent spawns `task` reviewers itself (non-isolated, read-only `scout`/`reviewer`) — workers never review. First pass: target the full slice range from its `main` base through the frozen HEAD (`/skill:frozen-review <base>..<slice-sha>` — a bare SHA reviews only that commit and the no-marker fallback at the release tag can sweep unrelated `main` commits); if the slice froze as multiple commits (worker + stage-A fix), record a B verdict per commit at each matching frozen revision. Later batches use the `review-last` marker delta. Paste frozen hunks into briefs with "do not re-run git diff"; carry facts sheet + three intake categories + report contract. Tag the behaviour slice "may run the thing".
- **Fix batch (worker author, in its worktree):** parent forwards findings only; one batch; worker re-gates, commits, reports. Never point a reviewer at its own fix (delta-only verification pass at most once).
- While a slice has a review in flight (B, C, or D), queue new asks — do NOT send follow-up work into the same branch (11-09: a mid-review addition forced a second delta review; on `hc` that means no `hc send` with new scope mid-review).
- **C (worker executes):** worker checks capacity (`coderabbit usage`, CLI ≥0.9.1), runs `coderabbit review --agent --gate major --base main` in its worktree — on exhausted quota the simulated-C fallback (frozen subagent review at major threshold) instead of a paid run. Never C on a revision with D in flight or on the same push as D. Parent only verifies the exit code from the report.
- **D (worker executes, parent merges):** worker pushes branch → opens PR (title carries `impv:`/`fix:`/`docs:` prefix — squash subjects feed `next-version.py`) → posts `@coderabbitai full review` alone on its line, top-level comment, and polls to verdict. After findings: worker runs `skill://autofix` verbatim; fix batch returns to A gates → B-delta (parent re-dispatches) → worker posts `@coderabbitai review`. Push mid-D can auto-pause the bot ("branch under active development") — post one `@coderabbitai review`; `already reviewed the last commit` + zero threads is clean. Parent merges only A-D-clean slices via the PR (merge button, never a local fast-forward + push — direct pushes to `main` are rejected, GH013).
- Parent reads PR state via `pr://<N>` (state, reviews, bot threads, merge state) — never shell `gh api` + hand-parse JSON in eval (11-09: quoting bugs, truncated bodies, 3+ re-fetches).
- **Close:** 0 critical / 0 high / 0 fix-now mediums in changed code. Any commit past the B freeze — even a 4-line D-fix — needs its own delta A/B before merge; no new A–B = the loop silently skips two steps. Marker `review-last` is local, never pushed.
