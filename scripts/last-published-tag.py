#!/usr/bin/env python3
"""Print newest published vX.Y.Z release tag (port of last-published-tag.sh)."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys

_TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")


def _version_key(tag):
    return tuple(int(n) for n in tag[1:].split("."))


def main(argv: list[str]) -> int:
    repo = argv[1] if len(argv) > 1 else ""
    if not repo:
        sys.stderr.buffer.write(f"Usage: {argv[0]} <owner/repo>\n".encode())
        return 1
    if shutil.which("gh") is None:
        sys.stderr.buffer.write(b"last-published-tag: gh not installed\n")
        return 1
    try:
        proc = subprocess.run(
            [
                "gh",
                "release",
                "list",
                "-R",
                repo,
                "--exclude-drafts",
                "--exclude-pre-releases",
                "--limit",
                "200",
                "--json",
                "tagName",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        entries = json.loads(proc.stdout) if proc.stdout.strip() else None
    except (json.JSONDecodeError, ValueError):
        entries = None
    best = ""
    if isinstance(entries, list):
        for entry in entries:
            name = entry.get("tagName") if isinstance(entry, dict) else None
            if (
                isinstance(name, str)
                and _TAG_RE.match(name)
                and (not best or _version_key(name) > _version_key(best))
            ):
                best = name
    tag = best
    if not tag:
        sys.stderr.buffer.write(
            f"last-published-tag: no published v* release found for {repo} (or gh auth failed)\n".encode()
        )
        return 1
    sys.stdout.buffer.write((tag + "\n").encode())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
