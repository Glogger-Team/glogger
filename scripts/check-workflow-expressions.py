#!/usr/bin/env python3
"""Gate: workflow expression identifiers + LF policy for .github/workflows.

Two mechanical defect shapes from the selective-lint rollout (PR #137):

1. Hyphenated filter/output names (e.g. `run-all`) are invalid GitHub
   expression identifiers: `needs.changes.outputs.run-all` never resolves,
   and the workflow fails at admission with `startup_failure` and zero
   jobs — no logs, banner text only. Every dorny/paths-filter key and every
   `needs.<job>.outputs.<name>` / `steps.<id>.outputs.<name>` dereference
   must be `[A-Za-z_][A-Za-z0-9_]*`.
2. `.gitattributes` mandates LF everywhere (`* text=auto eol=lf`), but a
   Python `str.replace` write silently flips a file to CRLF, failing
   `yamllint new-lines` at commit time. Workflows must be pure LF.

Stdlib only (matches check-lint-parity.py conventions).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"

# `${{ needs.<job>.outputs.<name> }}` / `${{ steps.<id>.outputs.<name> }}`
DEREF_RE = re.compile(r"(?:needs|steps)\.[A-Za-z0-9_.]+\.outputs\.([A-Za-z0-9_.-]+)")
VALID_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def check_file(path: Path) -> list[str]:
    findings: list[str] = []
    raw = path.read_bytes()
    if b"\r\n" in raw:
        n = raw.count(b"\r\n")
        findings.append(
            f"workflow-expressions: {path.name} has {n} CRLF line "
            "endings (repo policy is LF-only; normalize before commit)"
        )
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        print(
            f"workflow-expressions: cannot decode {path.name} as UTF-8",
            file=sys.stderr,
        )
        return ["decode-error"]
    for i, line in enumerate(text.splitlines(), start=1):
        for m in DEREF_RE.finditer(line):
            name = m.group(1)
            if not VALID_RE.match(name):
                findings.append(
                    f"workflow-expressions: {path.name}:{i}: "
                    f"output name '{name}' is not a valid expression "
                    "identifier (hyphens/dots fail admission with "
                    "startup_failure, zero jobs) — rename to underscores"
                )
    return findings


def main() -> int:
    if not WORKFLOWS.is_dir():
        print("workflow-expressions: .github/workflows/ missing", file=sys.stderr)
        return 2
    fail = 0
    for path in sorted(WORKFLOWS.glob("*.yml")):
        for finding in check_file(path):
            print(finding)
            fail = 1
    return fail


if __name__ == "__main__":
    sys.exit(main())
