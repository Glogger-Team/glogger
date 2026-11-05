#!/usr/bin/env python3
"""Warn (but don't block) when a commit message doesn't start with a known
conventional prefix. This improves changelog generation quality.

Known prefixes: feat, fix, impv, docs, test, build, release

Install: python scripts/setup-hooks.py (pre-commit commit-prefix hook)
"""

import re
import sys
from pathlib import Path

MERGE_RE = re.compile(r"^(Merge |release:)", re.IGNORECASE)
PREFIX_RE = re.compile(r"^(feat|fix|impv|docs|test|build)(\(.*\))?!?:")


def self_test() -> int:
    import subprocess
    import tempfile

    cases = [
        ("fix: correct scroll behavior", "silent"),
        ("feat(api)!: breaking change", "silent"),
        ("impv(scope): tidy", "silent"),
        ("Merge branch 'x' into y", "silent"),
        ("release: cut v1.2.3", "silent"),
        ("no prefix here", "warn"),
        ("chore: not a known prefix", "warn"),
    ]
    passed = 0
    with tempfile.TemporaryDirectory() as tmp:
        for message, expect in cases:
            msg = Path(tmp) / "msg"
            msg.write_text(message + "\n", encoding="utf-8")
            proc = subprocess.run(
                [sys.executable, __file__, str(msg)],
                capture_output=True,
                check=False,
                text=True,
            )
            if proc.returncode != 0:
                print(
                    f"self-test FAIL: {message!r} exited {proc.returncode} (must always exit 0)"
                )
                continue
            warned = bool(proc.stdout.strip())
            if (expect == "silent") == warned:
                print(f"self-test FAIL: {message!r} warned={warned}, expected {expect}")
                continue
            passed += 1
    print(f"commit-msg self-test: {passed}/{len(cases)} passed")
    return 0 if passed == len(cases) else 1


def main() -> int:
    # Self-test: `python scripts/check-commit-msg.py --self-test` (wired
    # into `mise run lint:commit`). Pins the warn-but-never-block contract:
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        return self_test()
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    msg_file = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if msg_file is None:
        print("usage: commit-msg <msg-file>", file=sys.stderr)
        return 1
    first_line = msg_file.read_text(encoding="utf-8", errors="replace").splitlines()
    first = first_line[0] if first_line else ""

    # Allow merge commits and release commits unconditionally.
    if MERGE_RE.match(first):
        return 0

    # Check for a known prefix (with optional scope).
    if PREFIX_RE.match(first):
        return 0

    print()
    print("  ⚠  Commit message has no conventional prefix.")
    print("     Expected: feat|fix|impv|docs|test|build")
    print("     Example:  fix: correct scroll behavior on gourmand screen")
    print(
        "     Write the subject for end users (what changed in the app), not the process (round N, CodeRabbit, backlog batch, SYNC_PAT)."
    )
    print(
        "     Example:  fix: StallTracker counts match after login without LocalPlayer sections"
    )
    print()
    print(f"     Your message: {first}")
    print()
    print("  Committing anyway — this is just a warning to help changelog quality.")
    print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
