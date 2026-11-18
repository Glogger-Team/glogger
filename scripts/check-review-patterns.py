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
# A silenced DB Result outside migrations: `let _ =` on a pool/connection
# call, on a db-verb helper (insert_/record_/link_/...), or on any call
# taking the connection as an argument drops write/link failures with no
# log and no retry (kill-loot link lesson, PR #202 — the line sat one inch
# below the rescan guard and never healed). The conn-arg branch is the
# class rule: any discarded call receiving `conn`/`tx` is a write-path
# silence regardless of verb (add_/set_/tighten_ all slipped a verb list
# once — B round, PR #202). Emits, fs ops, and test cleanup are
# deliberately out of scope: this gate owns the write path only.
# Sanctioned shape (like the migration gate's EXISTS guards): ROLLBACK
# cleanup in an already-failing path — the real error is propagated
# either way, so only the compensation can fail silently.
DB_SILENCE_RE = re.compile(
    r"let\s+_\s*=\s*(?!.*\bROLLBACK\b)"
    r"(?:"
    r".*\b(?:conn|tx|stmt)\s*\.\s*"
    r"(?:execute|execute_batch|query_row|query_map|query|transaction|prepare|commit)\b"
    r"|(?:(?:crate|super|self|db|[a-z_][a-z0-9_]*)::)*"
    r"(?:link|insert|record|persist|upsert|import|delete|update|backfill|reconcile|claim|attach|tag|add|set|tighten)"
    r"_[a-z0-9_]*\s*\("
    r"|\b[a-z_][a-z0-9_:]*\s*\([^;()]*\b(?:conn|tx)\b[^;()]*\)"
    r")"
)
MOD_TESTS_RE = re.compile(r"^\s*mod\s+tests\s*\{")
FINDING_DB_SILENCE = (
    "review-patterns: silenced DB write/link ({path}:{lineno})"
    " \u2014 propagate with ? or log like the receipt side (PR #202 link lesson)"
)


def read_body(root: Path) -> str:
    path = root / "src-tauri" / "src" / "db" / "migrations.rs"
    # migrations.rs is CRLF: strip \r too or the ^...$ anchor never matches
    # and the gate scans the test module the awk excludes (latent until a
    # test-module shape trips a pattern — fixture at 4371 lives pre-anchor).
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    out = []
    for line in lines:
        if re.match(r"^mod tests \{$", line.rstrip("\r\n")):
            break
        out.append(line)
    return "".join(out)


def code_lines(path: Path) -> list[tuple[int, str]]:
    """(lineno, line) outside `mod tests` blocks (brace-depth tracked)."""
    out: list[tuple[int, str]] = []
    skip_depth = 0
    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if skip_depth:
            skip_depth = max(0, skip_depth + raw.count("{") - raw.count("}"))
            continue
        if MOD_TESTS_RE.match(raw):
            skip_depth = raw.count("{") - raw.count("}")
            continue
        out.append((i, raw))
    return out


def findings_for_tree(root: Path) -> list[str]:
    findings: list[str] = []
    src = root / "src-tauri" / "src"
    for path in sorted(src.rglob("*.rs")):
        for lineno, line in code_lines(path):
            code = line.split("//")[0]
            if DB_SILENCE_RE.search(code):
                findings.append(
                    FINDING_DB_SILENCE.format(
                        path=path.relative_to(root), lineno=lineno
                    )
                )
    return findings


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
    findings += findings_for_tree(root)
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
