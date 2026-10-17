---
description: Review the current work here — takes its target from the argument or derives the frozen range, measures the facts once, fans out, reports only
argumentHint: "[target] [focus]"
---

Run this repository's code-review loop — `AGENTS.md` → **Code review loop** — end to end. Do not ask
which target: take it from the argument when one names one, else derive it; say what you chose, and go.

## 1. Resolve the frozen target (first, before anything else)

**The argument comes first.** Whatever the caller typed after `/frozen` arrives appended verbatim at the
end of this body ("Target revision and operator focus, if any, follow:"). Read it before deriving:

- a first token that names a **range** (`A..B`) is the target exactly as written;
- a first token that names a **tag** (`git tag --list <token>` names it) is the range that starts after
  it — `v1.3.2` → `v1.3.2..HEAD`, the same convention as the derived form below;
- a first token that names a **commit** is exactly that commit: freeze `git show <token>` and report the
  range as `<token>^..<token>` (the fix-batch case);
- the bare word `worktree` forces the working tree (`git diff HEAD`);
- **anything else is focus only**, and the target is derived below.

Bare `/frozen` derives, exactly as before; so a delta pass is
`/frozen b77e4bd verify the fix against the rest of the range` and a whole-range pass is bare `/frozen`.
This file must stay free of argument-placeholder tokens: the harness appends the caller's string only
while the body contains no placeholder, so one would silently swallow every word after the first.

When no target came from the argument:

- `git status --porcelain` non-empty → the target is the **working tree** (`git diff HEAD`, staged +
  unstaged). Reviewers read the files as they are, and the report calls it uncommitted.
- Clean, and `git rev-list <newest v*-tag>..HEAD` is non-empty → the target is
  **`<newest v*-tag>..HEAD`**, the in-flight series.
- Clean, and HEAD **is** the newest `v*` tag → the target is **`<previous tag>..<newest tag>`**, the
  release that just shipped (`git describe --tags --abbrev=0 <newest>^` names the base).

Print the resolved range in one line before anything else. Never review `origin/HEAD...main` unless it
resolves to the same commit.

## 2. Measure the facts once — in the parent

Run these once and paste the output into every reviewer's brief, with "do not re-run a row":
`npm run build` · `cd src-tauri && cargo test --lib` (the count) · `npm run check:grids` ·
`pwsh scripts/check-glogger-constraints.ps1` · plus the freeze state (`git rev-parse --short HEAD`,
`git status --porcelain`, `git log --oneline BASE..TARGET | wc -l`). A red row is a fact to report
once, not a per-file finding.

## 3. Fan out by risk class, not by directory

Three tiers, at most 8 concurrent reviewers, the parent as the only runner of shared commands:

1. changed **behaviour** — deep, and allowed to run the app (the live CDP smoke has found what reading
   could not);
2. **wiring/docs** — `AGENTS.md`, `HANDOFF.md`, the gate scripts and the census docs;
3. **gates/CI** — `check:grids`' four failure modes, the release workflow, the pre-push hook.

Give each reviewer its own slice of files, and tell it to run `git diff`/`git show` for exactly those.

## 4. Report to the contract, then stop

Severity (critical / high / medium / low / nit) · `file:line` · the defect · evidence (an exact quote
or command output) · the fix · `verified` or `[INFERENCE]`; then one verdict — ship /
ship-with-fixes / blocked — and an explicit fix-now vs backlog split. Intake is the three categories in
`AGENTS.md`, and the accepted-drift ledger in `docs/architecture/ux-standards.md` is decided: do not
re-file it. Fixes are a later turn's work — this round reports, and changes nothing.

Target revision and operator focus, if any, follow:
