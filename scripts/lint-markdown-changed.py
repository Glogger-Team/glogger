#!/usr/bin/env python3
"""Lint changed hand-maintained Markdown files with markdownlint-cli2.

Scope (single source of truth shared by CI `lint.yml: markdown` and local
`mise run lint:markdown`): every `*.md` EXCEPT the generated changelog —
`CHANGELOG.md` (rewritten by `npm run changelog` each release).

Grandfathering: only changed files are linted; pre-existing findings in
untouched files stay out of scope until the full `--fix` cleanup pass. CI
passes `--base <sha>` (PR base or push `before`; an all-zero sha, as on
manual dispatch, falls back to `HEAD~1`). A local run with no `--base`
lints staged + unstaged + untracked files PLUS files changed in unpushed
commits (base via unpushed_base(); when neither ref resolves, worktree
files only). `--push` (the pre-push hook) lints ONLY unpushed-commit
files, and fails closed listing any file that also has uncommitted
worktree changes — the linter reads the worktree, so checking those files
would mask the committed content actually being pushed. The commit hook
never runs `--push`, so amend/refine flows are unaffected.

The linter command defaults to `markdownlint-cli2` from PATH; override
with `MARKDOWNLINT`. Full-tree cleanup (not gated):
`markdownlint-cli2 --fix --config .markdownlint-cli2.json "**/*.md"
"!CHANGELOG.md"`.
"""

import argparse
import os
import shlex
import subprocess
import sys


def in_scope(path: str) -> bool:
    if not path.endswith(".md"):
        return False
    base = path.replace("\\", "/").rsplit("/", 1)[-1]
    return base != "CHANGELOG.md"


def git(*args: str) -> list[str]:
    # -z: NUL-delimited output; git quotes non-ASCII paths by default
    # (docs/café.md would arrive as "docs/caf\\303\\251.md" and silently
    # miss in_scope). text=False: split raw bytes on NUL, decode per path
    # with surrogateescape so undecodable bytes survive as names.
    out = subprocess.run(
        ["git", *args, "-z"], capture_output=True, text=False, check=True
    ).stdout
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def unpushed_base() -> str | None:
    """Fork point of HEAD vs the remote, for linting unpushed commits.

    Tries the upstream first (`@{u}`), then the remote default
    (`origin/HEAD`, which covers `git push -u` on a fresh branch with no
    upstream yet). Returns None when neither ref resolves.
    """
    for base_ref in ("@{u}", "origin/HEAD"):
        try:
            base = subprocess.run(
                ["git", "merge-base", "HEAD", base_ref],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
        except subprocess.CalledProcessError:
            continue
        if base:
            return base
    return None


def main() -> int:
    """Lint hand-maintained Markdown changed by the selected scope.

    `--base <sha>` (CI): files changed between base and HEAD. `--push`
    (pre-push hook): files changed in unpushed commits only, failing
    closed on worktree overlap. Neither flag (commit hook / manual):
    worktree files plus unpushed commits. Returns the linter exit code,
    1 on overlap or gate failure, 0 when nothing in scope.
    """
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=None)
    parser.add_argument(
        "--push",
        action="store_true",
        help="lint only unpushed-commit files; fail if any has "
        "uncommitted worktree changes (worktree content would mask "
        "the committed content being pushed)",
    )
    args = parser.parse_args()

    if args.push:
        base = unpushed_base()
        if base is None:
            print("No upstream or remote default; nothing pushed to check.")
            return 0
        files = git("diff", "--name-only", base, "HEAD")
        # Worktree content masks committed content for these files: the
        # linter reads the worktree, but the push ships HEAD. Fail closed
        # with an actionable message instead of checking the wrong bytes.
        # (The commit hook never runs --push, so amend/refine flows that
        # legitimately stack worktree tweaks on unpushed commits are
        # unaffected.)
        masked = sorted(
            f
            for f in set(files)
            & set(git("diff", "--name-only", "--no-renames", "HEAD"))
            if in_scope(f)
        )
        if masked:
            print(
                "Refusing to lint pushed Markdown with uncommitted changes "
                "(worktree content would mask the committed content):",
                file=sys.stderr,
            )
            for f in masked:
                print(f"  {f}", file=sys.stderr)
            print(
                "Commit (or stash) these files first, then push again.",
                file=sys.stderr,
            )
            return 1
    elif args.base is None:
        files = git("diff", "--name-only", "HEAD")
        files += git("ls-files", "--others", "--exclude-standard")
        # Unpushed commits: invisible to the worktree diff above, but
        # exactly what the pre-push hook must check on a clean tree.
        base = unpushed_base()
        if base is not None:
            files += git("diff", "--name-only", base, "HEAD")
    else:
        base = args.base
        if base.strip("0") == "":
            print(
                "zero base sha (manual dispatch); falling back to HEAD~1",
                file=sys.stderr,
            )
            base = "HEAD~1"
        files = git("diff", "--name-only", base, "HEAD")

    files = [f for f in files if in_scope(f) and os.path.isfile(f)]
    if not files:
        print("No changed hand-maintained markdown files.")
        return 0

    cmd = shlex.split(
        os.environ.get("MARKDOWNLINT", "markdownlint-cli2"),
        posix=os.name != "nt",
    )
    # ":" prefix: markdownlint-cli2 literal-path mode; without it args are
    # globs and a file like docs/notes[1].md can skip or match elsewhere.
    cmd += ["--config", ".markdownlint-cli2.json"] + [f":{f}" for f in files]
    print("+", " ".join(cmd))
    return subprocess.run(cmd, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
