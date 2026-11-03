#!/usr/bin/env python3
"""Point git at the project's .githooks directory (port of setup-hooks.sh)."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    subprocess.run(
        ["git", "-C", str(root), "config", "core.hooksPath", ".githooks"],
        check=True,
    )
    sys.stdout.buffer.write("Git hooks configured — using .githooks/ directory.\n".encode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
