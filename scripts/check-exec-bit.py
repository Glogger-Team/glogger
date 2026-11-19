#!/usr/bin/env python3
"""Gate: scripts in the worktree must carry the owner-exec bit.

Scope: files new to the worktree (untracked + staged-added). Only *.py and
*.sh are considered (the repo's script languages; CI invokes via
interpreters so the bit is conventional). For staged-added files the git
index mode is authoritative (on core.fileMode=false boxes the worktree bit
is decorative); untracked files fall back to os.access.

CI passes --base <sha> (the PR base / push `before`; the one sanctioned
invocation difference, same as the markdown job): scope is then the files
added between base and HEAD, because a clean CI checkout has no untracked
or staged files - the local scope would be empty and the gate could never
fire. An all-zero base (manual dispatch) falls back to HEAD~1, same as the
markdown script. In-range files are committed, so the HEAD index mode
(`git ls-files -s`) is authoritative for them.
"""

from __future__ import annotations

import argparse
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


def resolve_base(explicit: str | None) -> str | None:
    if explicit is None:
        return None
    if explicit.strip("0") == "":
        print(
            "zero base sha (manual dispatch); falling back to HEAD~1",
            file=sys.stderr,
        )
        return "HEAD~1"
    return explicit


def new_files(base: str | None) -> list[str]:
    files: list[str] = []
    if base is not None:
        files += git_lines("diff", "--name-only", "--diff-filter=A", base, "HEAD")
        return files
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


def parse_raw_modes(raw_text: str) -> dict[str, tuple[str, str]]:
    """Index/worktree `:oldmode newmode ... status\\tpath` lines keyed by path."""
    modes: dict[str, tuple[str, str]] = {}
    for ln in raw_text.splitlines():
        if not ln.startswith(":"):
            continue
        meta, _, path = ln[1:].partition("\t")
        parts = meta.split(" ")
        if len(parts) < 5:
            continue
        # Rename/copy lines carry two paths; the new path decides.
        modes[path.split("\t")[-1]] = (parts[0], parts[1])
    return modes


def lost_exec_flips() -> list[str]:
    """Tracked scripts flipped 755->644 in the staged or unstaged diff."""
    flips: dict[str, str] = {}
    staged = parse_raw_modes(
        run_git("diff", "--cached", "--raw", "--diff-filter=MR").stdout
    )
    unstaged = parse_raw_modes(run_git("diff", "--raw", "--diff-filter=MR").stdout)
    for path, (old, new) in staged.items():
        if old == "100755" and new == "100644":
            flips[path] = f"staged {old}->{new}"
    for path, (old, new) in unstaged.items():
        if old == "100755" and new == "100644" and path not in flips:
            flips[path] = f"unstaged {old}->{new}"
    return [
        f"{path}: executable bit lost ({how}) - fix: git update-index --chmod=+x {path}"
        for path, how in sorted(flips.items())
        if Path(path).suffix in SCRIPT_EXTS
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--base",
        default=None,
        help="CI range: files added between base and HEAD "
        "(PR base sha / push before; all-zero falls back to HEAD~1)",
    )
    args = parser.parse_args()
    base = resolve_base(args.base)
    scoped: list[str] = []
    for f in new_files(base):
        if Path(f).suffix in SCRIPT_EXTS and os.path.isfile(ROOT / f):
            scoped.append(f)
    # Index modes look like `100755 <sha> 0\\t<path>`. Staged-added files
    # are in the index, so the recorded mode decides; untracked files are
    # absent and keep the os.access fallback. In-range files are committed,
    # so the HEAD index mode decides for them the same way.
    indexed: dict[str, str] = {}
    for ln in git_lines("ls-files", "-s"):
        left, _, path = ln.partition("	")
        indexed[path] = left.split(" ", 1)[0]
    findings: list[str] = []
    for f in scoped:
        mode = indexed.get(f)
        if mode is not None:
            if mode != "100755":
                where = "committed with" if base is not None else "staged with"
                findings.append(
                    f"{f}: {where} index mode {mode}, not executable"
                    f" - fix: git update-index --chmod=+x {f}"
                )
        elif not os.access(ROOT / f, os.X_OK):
            findings.append(f"{f}: missing exec bit - fix: chmod +x {f}")
    findings += lost_exec_flips()
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
