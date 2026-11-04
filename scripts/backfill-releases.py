#!/usr/bin/env python3
"""Backfill GitHub Release objects for tags that have none on this repo.

Python port of scripts/backfill-releases.sh (stdlib only).

Usage: backfill-releases.py [tags-file]
  defaults to all v* tags with no Release on Glogger-Team/glogger.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = "Glogger-Team/glogger"
ROOT = Path(__file__).resolve().parent.parent

if os.name == "nt":
    LOG_PATH = str(Path(tempfile.gettempdir()) / "backfill.log")
else:
    LOG_PATH = "/tmp/backfill.log"


def _version_key(tag: str):
    return tuple(
        (0, int(chunk)) if chunk.isdigit() else (1, chunk)
        for chunk in re.findall(r"\d+|\D+", tag)
    )


def _missing_tags() -> list[str]:
    proc = subprocess.run(
        ["git", "tag", "--list", "v[0-9]*"],
        check=True,
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    all_tags = sorted(
        (line for line in proc.stdout.splitlines() if line), key=_version_key
    )
    proc = subprocess.run(
        ["gh", "release", "list", "-R", REPO, "--limit", "200", "--json", "tagName"],
        check=True,
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    existing = {entry["tagName"] for entry in json.loads(proc.stdout or "[]")}
    return [tag for tag in all_tags if tag not in existing]


def extract_section(changelog: str, tag: str) -> str:
    hdr = f"## {tag} "
    found = False
    lines: list[str] = []
    for line in changelog.splitlines():
        if not found:
            if line.startswith(hdr):
                found = True
            continue
        if line.startswith("## "):
            break
        lines.append(line)
    # Mirror BODY="$(awk ...)": command substitution strips trailing newlines only.
    return "\n".join(lines).rstrip("\n")


def main(argv: list[str]) -> int:
    tags_file = argv[0] if argv else None
    if tags_file is None:
        tags = _missing_tags()
    else:
        # Bash `read` strips only the trailing newline; a trailing \r is kept.
        with open(tags_file, encoding="utf-8", newline="") as fh:
            tags = [line.removesuffix("\n") for line in fh]
    changelog_text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")

    count = 0
    failed = 0
    for tag in tags:
        if not tag:
            continue
        body = extract_section(changelog_text, tag)
        if not body:
            print(f"SKIP {tag}: no CHANGELOG section")
            failed += 1
            continue
        title = tag if tag.startswith("v0.") else f"glogger {tag}"
        fd, notes_file = tempfile.mkstemp()
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(body + "\n")
                fh.write(
                    "\n---\n"
                    "_Backfilled historical release — installer binaries for this "
                    "version are not attached here._\n"
                )
            proc = subprocess.run(
                [
                    "gh",
                    "release",
                    "create",
                    tag,
                    "--title",
                    title,
                    "--notes-file",
                    notes_file,
                    "-R",
                    REPO,
                ],
                check=False,
                cwd=ROOT,
                capture_output=True,
                text=True,
                encoding="utf-8",
            )
            with open(LOG_PATH, "a", encoding="utf-8") as log:
                if proc.stdout:
                    log.write(proc.stdout)
                    if not proc.stdout.endswith("\n"):
                        log.write("\n")
                if proc.stderr:
                    log.write(proc.stderr)
                    if not proc.stderr.endswith("\n"):
                        log.write("\n")
            if proc.returncode == 0:
                print(f"OK {tag}")
                count += 1
            else:
                print(f"FAIL {tag} (see {LOG_PATH})")
                failed += 1
        finally:
            try:
                os.unlink(notes_file)
            except OSError:
                pass
    print(f"Created: {count}, failed/skipped: {failed}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
