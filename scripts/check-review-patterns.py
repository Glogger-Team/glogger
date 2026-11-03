#!/usr/bin/env python3
"""Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit rounds."""

import re
import sys
from pathlib import Path

FILTER_MAP_RE = re.compile(
    r"filter_map\(\s*\|\s*([A-Za-z_][A-Za-z0-9_]*)\s*\|\s*\1\.ok\(\)",
    re.DOTALL,
)
LET_DISCARD_RE = re.compile(
    r"let _\s*=\s*(conn|tx|stmt)\.(execute|execute_batch|query_row|query_map)",
    re.DOTALL,
)

FINDING_FILTER_MAP = (
    "review-patterns: filter_map(|x| x.ok()) row-read discard in migrations.rs body"
    " \u2014 collect::<Result<..>>()? instead"
)
FINDING_LET_DISCARD = (
    "review-patterns: discarded migration write/row-read (let _ = ...)"
    " \u2014 propagate with ? (v81 lesson)"
)


def read_body(root: Path) -> str:
    path = root / "src-tauri" / "src" / "db" / "migrations.rs"
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    out = []
    for line in lines:
        if re.match(r"^mod tests \{$", line.rstrip("\n")):
            break
        out.append(line)
    return "".join(out)


def findings_for(body: str) -> list[str]:
    findings: list[str] = []
    try:
        m1 = FILTER_MAP_RE.search(body)
    except re.error:
        print("review-patterns: filter_map search failed", file=sys.stderr)
        sys.exit(2)
    else:
        if m1:
            findings.append(FINDING_FILTER_MAP)
    try:
        m2 = LET_DISCARD_RE.search(body)
    except re.error:
        print("review-patterns: let-discard search failed", file=sys.stderr)
        sys.exit(2)
    else:
        if m2:
            findings.append(FINDING_LET_DISCARD)
    return findings


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    try:
        body = read_body(root)
    except OSError:
        print("review-patterns: failed to read migrations.rs body", file=sys.stderr)
        return 2
    findings = findings_for(body)
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
