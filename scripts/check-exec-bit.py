#!/usr/bin/env python3
"""Gate: new scripts in the worktree must carry the owner-exec bit.

Scope: files new to the worktree (untracked + staged-added). Only *.py and
*.sh are considered (the repo's script languages; CI invokes via
interpreters so the bit is conventional). For staged-added files the git
index mode is authoritative (on core.fileMode=false boxes the worktree bit
is decorative); untracked files fall back to os.access.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT_EXTS = {".py", ".sh"}


def run_git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def git_lines(*args: str) -> list[str]:
    proc = run_git(*args)
    if proc.returncode != 0:
        print(
            f"check-exec-bit: git {' '.join(args)} failed: {proc.stderr.strip()}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    return [ln for ln in proc.stdout.splitlines() if ln.strip()]


def new_files() -> list[str]:
    files: list[str] = []
    files += git_lines("ls-files", "--others", "--exclude-standard")
    files += git_lines("diff", "--cached", "--name-only", "--diff-filter=A")
    seen: set[str] = set()
    out: list[str] = []
    for f in files:
        if f in seen:
            continue
        seen.add(f)
        out.append(f)
    return out


def main() -> int:
    scoped: list[str] = []
    for f in new_files():
        if Path(f).suffix in SCRIPT_EXTS and os.path.isfile(ROOT / f):
            scoped.append(f)
    # Index modes look like `100755 <sha> 0\\t<path>`. Staged-added files
    # are in the index, so the recorded mode decides; untracked files are
    # absent and keep the os.access fallback.
    indexed: dict[str, str] = {}
    for ln in git_lines("ls-files", "-s"):
        left, _, path = ln.partition("	")
        indexed[path] = left.split(" ", 1)[0]
    findings: list[str] = []
    for f in scoped:
        mode = indexed.get(f)
        if mode is not None:
            if mode != "100755":
                findings.append(
                    f"{f}: staged with index mode {mode}, not executable"
                    f" - fix: git update-index --chmod=+x {f}"
                )
        elif not os.access(ROOT / f, os.X_OK):
            findings.append(f"{f}: missing exec bit - fix: chmod +x {f}")
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
