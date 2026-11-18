#!/usr/bin/env python3
"""Gate: every Pinia store touched by this commit must accept HMR updates.

Scope (commit-stage semantics, mirroring scripts/lint-markdown-changed.py's
no---base behavior): worktree files (staged + unstaged + untracked) PLUS
files changed in unpushed commits (fork point via @{u} then origin/HEAD,
unpushed_base() logic). No --push variant: the commit hook is the contract,
and push certifies the same worktree-or-committed content it already gated.

In-scope store files: src/stores/*.ts and src/stores/**/*.ts. A store file in
scope that does not contain the literal acceptHMRUpdate is a finding (printed
as its path). Unmigrated stores elsewhere in the tree are grandfathered -
only touched stores are gated, same pre-existing-findings principle as the
markdown gate.
"""
# Scope resolution (git()/unpushed_base() and the worktree+unpushed collection)
# is mirrored from scripts/lint-markdown-changed.py - do not import across
# scripts; keep the logic duplicated here intentionally.

import subprocess
import sys
from pathlib import Path


def git(*args):
    out = subprocess.run(
        ["git", *args, "-z"], capture_output=True, text=False, check=True
    ).stdout
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def unpushed_base():
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


def in_scope(path):
    if not path.endswith(".ts"):
        return False
    norm = path.replace(chr(92), "/")
    return norm.startswith("src/stores/")


def main():
    root = Path(__file__).resolve().parent.parent
    files = git("diff", "--name-only", "HEAD")
    files += git("ls-files", "--others", "--exclude-standard")
    base = unpushed_base()
    if base is not None:
        files += git("diff", "--name-only", base, "HEAD")

    store_files = []
    seen = set()
    for f in files:
        if in_scope(f) and f not in seen:
            seen.add(f)
            store_files.append(f)

    findings = []
    for f in sorted(store_files):
        full = root / f
        if not full.is_file():
            continue
        try:
            body = full.read_text(encoding="utf-8")
        except OSError:
            print("check-store-hmr: failed to read " + f, file=sys.stderr)
            return 2
        if "acceptHMRUpdate" not in body:
            findings.append(f)

    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
