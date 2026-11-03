#!/usr/bin/env python3
"""Lint changed hand-maintained Markdown files with markdownlint-cli2.

Scope (single source of truth shared by CI `lint.yml: markdown` and local
`mise run lint:all`): every `*.md` EXCEPT generated/append-only logs —
`CHANGELOG.md` (rewritten by `npm run changelog` each release) and
`HANDOFF*.md` (append-only session record, never revised).

Grandfathering: only changed files are linted; pre-existing findings in
untouched files stay out of scope until the full `--fix` cleanup pass. CI
passes `--base <sha>` (PR base or push `before`; an all-zero sha, as on
manual dispatch, falls back to `HEAD~1`). A local run with no `--base`
lints staged + unstaged + untracked files.

The linter command defaults to `markdownlint-cli2` from PATH; override
with `MARKDOWNLINT` (CI uses `npx --yes markdownlint-cli2@0.23` — floor,
not a freeze, matching lint.yml policy). Full-tree cleanup (not gated):
`markdownlint-cli2 --fix --config .markdownlint-cli2.json "**/*.md"
"!CHANGELOG.md" "!HANDOFF*.md"`.
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
    if base == "CHANGELOG.md":
        return False
    return not base.startswith("HANDOFF")


def git(*args: str) -> list[str]:
    out = subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout
    return [line for line in out.splitlines() if line]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=None)
    args = parser.parse_args()

    if args.base is None:
        files = git("diff", "--name-only", "HEAD")
        files += git("ls-files", "--others", "--exclude-standard")
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
    cmd += ["--config", ".markdownlint-cli2.json", *files]
    print("+", " ".join(cmd))
    return subprocess.run(cmd, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
