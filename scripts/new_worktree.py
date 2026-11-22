#!/usr/bin/env python3
"""Cut a git worktree for slice work and install its node_modules.

Usage: mise run worktree <branch> [base] [--path <dir>]
   or: python scripts/new_worktree.py <branch> [base] [--path <dir>]

Defaults: base `main`; path `.herdr/worktrees/glogger/<slug>` where slug
is the branch with `/` replaced by `-` (mirrors the fanout layout).
Runs `mise exec -- npm install` in the new checkout: every worktree needs
its own node_modules (lockfile-pinned, ~10s) or both the A gate and the
pre-push lint:typecheck fail on missing vue-tsc.
"""

import argparse
import subprocess
import sys
from pathlib import Path


def run(cmd, **kw):
    r = subprocess.run(cmd, check=False, capture_output=True, text=True, **kw)
    if r.returncode != 0:
        print(f"Error: {' '.join(cmd)}\n{(r.stderr or r.stdout).strip()}")
        sys.exit(1)
    return r.stdout.strip()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("branch")
    ap.add_argument("base", nargs="?", default="main")
    ap.add_argument("--path", default=None)
    args = ap.parse_args()
    root = Path.cwd()
    slug = args.branch.replace("/", "-")
    # Sibling of the repo root, mirroring the fanout layout — never inside
    # the repo tree (nested worktrees confuse tooling and pollute status).
    path = (
        Path(args.path)
        if args.path
        else root.parent / ".herdr" / "worktrees" / "glogger" / slug
    )

    if run(["git", "branch", "--list", args.branch]):
        print(f"Error: local branch {args.branch} already exists.")
        return 1
    if run(["git", "ls-remote", "--heads", "origin", args.branch]):
        print(f"Error: remote branch {args.branch} already exists.")
        return 1
    if path.exists():
        print(f"Error: {path} already exists.")
        return 1

    run(["git", "worktree", "add", "-b", args.branch, str(path), args.base])
    run(["mise", "exec", "--", "npm", "install"], cwd=str(path))
    print(f"Ready: {path} on {args.branch} (node_modules installed).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
