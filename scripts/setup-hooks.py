#!/usr/bin/env python3
"""Install the project's pre-commit hooks (pre-commit + pre-push + commit-msg
stages, defined in .pre-commit-config.yaml) via the mise-provided runner.

Idempotent: safe to re-run. Skipped-locally work is never silent — CI runs
the same mise tasks, and `git commit/push --no-verify` bypasses loudly.

Port of scripts/setup-hooks.sh (stdlib only, works in cmd/PowerShell/
git-bash alike).

Usage: python scripts/setup-hooks.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    proc = subprocess.run(
        ["git", "-C", str(ROOT), "config", "--get", "core.hooksPath"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    # Clean up the pre-framework layout: .githooks/ no longer exists.
    if proc.returncode == 0 and proc.stdout.strip() == ".githooks":
        unset = subprocess.run(
            ["git", "-C", str(ROOT), "config", "--unset", "core.hooksPath"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        if unset.returncode != 0:
            print(f"setup-hooks: {unset.stderr.strip()}", file=sys.stderr)
            return 1
        print("Removed legacy core.hooksPath=.githooks.")
    install = subprocess.run(
        [
            "mise",
            "exec",
            "--",
            "pre-commit",
            "install",
            "--hook-type",
            "pre-commit",
            "--hook-type",
            "pre-push",
            "--hook-type",
            "commit-msg",
        ],
        cwd=ROOT,
        check=False,
    )
    if install.returncode != 0:
        return install.returncode
    print("pre-commit hooks installed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
