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
commits (diff of `merge-base @{u} HEAD` to `HEAD`; no upstream, e.g. a
never-pushed branch, falls back to worktree files only). The unpushed half
is what makes the pre-push hook meaningful on a clean tree: with worktree
files alone, committed-but-unpushed Markdown would pass unchecked locally
(CI would still catch it via `--base`).

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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=None)
    args = parser.parse_args()

    if args.base is None:
        files = git("diff", "--name-only", "HEAD")
        files += git("ls-files", "--others", "--exclude-standard")
        # Unpushed commits: invisible to the worktree diff above, but
        # exactly what the pre-push hook must check on a clean tree.
        try:
            upstream_base = subprocess.run(
                ["git", "merge-base", "HEAD", "@{u}"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
            if upstream_base:
                files += git("diff", "--name-only", upstream_base, "HEAD")
        except subprocess.CalledProcessError:
            pass  # no upstream (never-pushed branch): worktree files only
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
