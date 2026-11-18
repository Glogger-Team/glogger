#!/usr/bin/env python3
"""Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit rounds."""

import re
import sys
from pathlib import Path

FILTER_MAP_RE = re.compile(
    r"filter_map\(\s*Result::ok\s*\)"
    r"|filter_map\(\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\.ok\(\)\s*\)"
    r"|filter_map\(\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\s*\|\s*\{[^}]*\.ok\(\)[^}]*\}\s*\)",
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
# A silenced DB Result outside migrations: `let _ =` on any call whose
# method name is a SQL op (execute/.../commit) — those only exist on
# rusqlite types, so a discarded call is a DB silence by definition (any
# receiver, incl. pool.get_write().unwrap()); on a db-verb helper
# (insert_/record_/link_/...); or on any call taking `conn` as an arg
# (conn-arg branch is the class rule). `tx`-only args no longer trip
# branch 3; read-verb callees (get/fetch/...) are excluded. Multi-line
# rustfmt-wrapped writes are joined (cap 5) before matching. Test modules
# (`mod \w*tests?`, cfg(test) mod blocks) are skipped. Drops write/link
# failures with no log and no retry (kill-loot link lesson, PR #202).
# Emits, fs ops, and test cleanup are deliberately out of scope.
# Sanctioned shape (like the migration gate's EXISTS guards): ROLLBACK
# cleanup in an already-failing path — the real error is propagated
# either way, so only the compensation can fail silently.
DB_SILENCE_RE = re.compile(
    r"let\s+_\s*=\s*(?!.*\bROLLBACK\b)"
    r"(?:"
    r".*\.\s*(?:execute|execute_batch|query_row|query_map|query|transaction|prepare|commit)\b"
    r"|(?:::)?(?:(?:crate|super|self|db|[a-z_][a-z0-9_]*)::)*"
    r"(?:link|insert|record|persist|upsert|import|delete|update|backfill|"
    r"reconcile|claim|attach|tag|add|set|tighten)"
    r"_[a-z0-9_]*\s*\("
    r"|\b(?:[a-z_][a-z0-9_]*::)*+"
    r"(?!(?:get|fetch|load|read|find|search|list|check|exists|count|lookup|validate)(?:_|\b))"
    r"[a-z_][a-z0-9_]*\s*\([^;()]*\bconn\b[^;()]*\)"
    r")"
)
MOD_TESTS_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+\w*tests?\b")
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
        if re.match(r"^mod \w*tests?\b", line.rstrip("\r\n")):
            break
        out.append(line)
    return "".join(out)


def code_lines(path: Path) -> list[tuple[int, str]]:
    """(lineno, line) outside `mod tests` blocks (brace-depth tracked)."""
    out: list[tuple[int, str]] = []
    skip_depth = 0
    prev_sig = ""
    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if skip_depth:
            skip_depth = max(0, skip_depth + raw.count("{") - raw.count("}"))
            continue
        stripped = raw.strip()
        is_test_mod = (
            MOD_TESTS_RE.match(raw) is not None
            or (
                re.match(r"^\s*(?:pub\s+)?mod\s+\w+\s*\{", raw) is not None
                and prev_sig == "#[cfg(test)]"
            )
            or (
                re.search(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]", raw) is not None
                and re.search(r"\bmod\b\s+\w+\s*\{", raw) is not None
            )
        )
        if is_test_mod:
            skip_depth = raw.count("{") - raw.count("}")
            if stripped:
                prev_sig = stripped
            continue
        if stripped:
            prev_sig = stripped
        out.append((i, raw))
    return out


def findings_for_tree(root: Path) -> list[str]:
    findings: list[str] = []
    src = root / "src-tauri" / "src"
    for path in sorted(src.rglob("*.rs")):
        cl = code_lines(path)
        i = 0
        n = len(cl)
        while i < n:
            lineno, line = cl[i]
            code = line.split("//")[0]
            joined = code
            consumed = 1
            if "let _" in code and "=" in code:
                opens = code.count("(") - code.count(")")
                if opens > 0:
                    cur = code
                    while opens > 0 and consumed < 5 and i + consumed < n:
                        _nlineno, nline = cl[i + consumed]
                        ncode = nline.split("//")[0]
                        cur = cur + " " + ncode
                        opens += ncode.count("(") - ncode.count(")")
                        consumed += 1
                        if opens <= 0:
                            break
                    joined = cur
            if DB_SILENCE_RE.search(joined):
                findings.append(
                    FINDING_DB_SILENCE.format(
                        path=path.relative_to(root), lineno=lineno
                    )
                )
            i += consumed
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
